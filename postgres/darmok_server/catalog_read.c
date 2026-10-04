/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#include "postgres.h"

#include "access/heapam.h"
#include "access/table.h"
#include "access/tableam.h"
#include "catalog/namespace.h"
#include "catalog/pg_attribute.h"
#include "catalog/pg_class.h"
#include "catalog/pg_extension.h"
#include "catalog/pg_namespace.h"
#include "catalog/pg_type.h"
#include "commands/extension.h"
#include "funcapi.h"
#include "mb/pg_wchar.h"
#include "miscadmin.h"
#include "utils/builtins.h"
#include "utils/guc.h"
#include "utils/hsearch.h"
#include "utils/inval.h"
#include "utils/json.h"
#include "utils/jsonb.h"
#include "utils/memutils.h"
#include "utils/snapmgr.h"
#include "utils/syscache.h"

#include "catalog_read.h"

#define REQUEST_BYTES (1024 * 1024)
#define REQUEST_PAIRS 4096
#define READ_ATTEMPTS 16
#define PHASE_BYTES ((Size) 64 * 1024 * 1024)
#define FACT_HEAPS 4

static char *catalog_request = NULL;
static const Oid heap_oids[FACT_HEAPS] = {
	NamespaceRelationId, RelationRelationId, AttributeRelationId, TypeRelationId
};

typedef struct NamespaceFact
{
	Oid oid;
	NameData name;
} NamespaceFact;

typedef struct NamespaceName
{
	NameData name;
	Oid oid;
} NamespaceName;

typedef struct RelationKey
{
	Oid schema_oid;
	NameData name;
} RelationKey;

typedef struct RequestedRelation
{
	RelationKey key;
	Oid oid;
} RequestedRelation;

typedef struct RequestPair
{
	char *schema;
	char *name;
	RelationKey key;
} RequestPair;

typedef struct ColumnFact
{
	int16 number;
	NameData name;
	Oid type_oid;
	int32 typmod;
	int16 dimensions;
	bool not_null;
	bool has_expression;
	char identity;
	char generated;
	Oid collation;
} ColumnFact;

typedef struct RelationFact
{
	Oid oid;
	Oid schema_oid;
	NameData name;
	char kind;
	char persistence;
	bool is_partition;
	List *columns;
} RelationFact;

typedef struct TypeFact
{
	Oid oid;
	Oid schema_oid;
	NameData name;
	char kind;
	char category;
	Oid element;
	Oid base;
	int32 typmod;
	bool not_null;
	int32 dimensions;
	uint8 visited;
} TypeFact;

typedef struct ReadState
{
	MemoryContext invocation;
	MemoryContext attempt;
	MemoryContext result;
	Relation heaps[FACT_HEAPS];
	TableScanDesc scans[FACT_HEAPS];
	Snapshot snapshot;
	HTAB *namespaces;
	HTAB *namespace_names;
	HTAB *requested;
	HTAB *relations;
	HTAB *types;
	RequestPair *pairs;
	uint32 count;
	bool first_snapshot_set;
	DarmokCatalogStamp observed;
	DarmokCatalogStamp stamp;
	SubTransactionId subid;
	volatile DarmokRelationAttempt seed;
} ReadState;

void
darmok_catalog_seed_acquire(volatile DarmokRelationAttempt *attempt,
							const Oid *heaps, int count)
{
	DarmokRelationRequest *requests;

	if (heaps == NULL || count <= 0 || count > DARMOK_RELATION_REQUEST_LIMIT - 2)
		ereport(ERROR,
				(errcode(ERRCODE_INVALID_PARAMETER_VALUE),
				 errmsg("native catalog seed has an invalid heap count")));
	requests = palloc(sizeof(DarmokRelationRequest) * (count + 2));
	for (int i = 0; i < count; i++)
		requests[i] = (DarmokRelationRequest) {
			.relation_oid = heaps[i], .lock_mode = AccessShareLock
		};
	requests[count] = (DarmokRelationRequest) {
		.relation_oid = ClassOidIndexId, .lock_mode = AccessShareLock
	};
	requests[count + 1] = (DarmokRelationRequest) {
		.relation_oid = AttributeRelidNumIndexId, .lock_mode = AccessShareLock
	};
	PG_TRY();
	{
		darmok_relation_attempt_acquire(attempt, requests, count + 2);
	}
	PG_FINALLY();
	{
		pfree(requests);
	}
	PG_END_TRY();
}

void
darmok_catalog_define_guc(void)
{
	DefineCustomStringVariable(DARMOK_CATALOG_REQUEST,
							   "Literal native catalog discovery request.", NULL,
							   &catalog_request, "[]", PGC_USERSET,
							   GUC_NOT_IN_SAMPLE, NULL, NULL, NULL);
}

static void
invalid_request(void)
{
	ereport(ERROR,
			(errcode(ERRCODE_INVALID_PARAMETER_VALUE),
			 errmsg("catalog_request_v1 requires an array of two-string name pairs")));
}

static void
check_data_snapshot(ReadState *state)
{
	/* Detect an unexpected callback, never reset or repair native snapshot
	 * state. The private owner treats this profile failure as disposal-only. */
	if (FirstSnapshotSet != state->first_snapshot_set)
		ereport(ERROR,
				(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
				 errmsg("catalog discovery changed the native data-snapshot state")));
}

static void
parse_request(ReadState *state)
{
	Jsonb *json;
	JsonbIterator *iter;
	JsonbValue value;
	uint32 i;

	if (strlen(catalog_request) > REQUEST_BYTES)
		ereport(ERROR,
				(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
				 errmsg("catalog discovery request exceeds 1 MiB")));
	/* The pinned builtin parser runs outside every fence. It invokes no SQL
	 * type lookup, user input function or transaction data-snapshot API. */
	json = DatumGetJsonbP(DirectFunctionCall1(jsonb_in,
										   CStringGetDatum(catalog_request)));
	if (!JB_ROOT_IS_ARRAY(json) || JB_ROOT_IS_SCALAR(json))
		invalid_request();
	state->count = JB_ROOT_COUNT(json);
	if (state->count > REQUEST_PAIRS)
		ereport(ERROR,
				(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
				 errmsg("catalog discovery request exceeds 4096 name pairs")));
	state->pairs = palloc0(sizeof(RequestPair) * state->count);
	iter = JsonbIteratorInit(&json->root);
	if (JsonbIteratorNext(&iter, &value, true) != WJB_BEGIN_ARRAY)
		invalid_request();
	for (i = 0; i < state->count; i++)
	{
		JsonbIterator *pair;
		char **parts[2] = {&state->pairs[i].schema, &state->pairs[i].name};
		int j;

		if (JsonbIteratorNext(&iter, &value, true) != WJB_ELEM ||
			value.type != jbvBinary ||
			!JsonContainerIsArray(value.val.binary.data) ||
			JsonContainerIsScalar(value.val.binary.data) ||
			JsonContainerSize(value.val.binary.data) != 2)
			invalid_request();
		pair = JsonbIteratorInit(value.val.binary.data);
		if (JsonbIteratorNext(&pair, &value, true) != WJB_BEGIN_ARRAY)
			invalid_request();
		for (j = 0; j < 2; j++)
		{
			if (JsonbIteratorNext(&pair, &value, true) != WJB_ELEM ||
				value.type != jbvString)
				invalid_request();
			*parts[j] = pnstrdup(value.val.string.val, value.val.string.len);
		}
		if (JsonbIteratorNext(&pair, &value, true) != WJB_END_ARRAY)
			invalid_request();
	}
	if (JsonbIteratorNext(&iter, &value, true) != WJB_END_ARRAY)
		invalid_request();
}

void
darmok_catalog_verify_installation(void)
{
	Oid oid = get_extension_oid("darmok_server", false);
	HeapTuple tuple = SearchSysCache1(EXTENSIONOID, ObjectIdGetDatum(oid));
	Form_pg_extension extension;
	bool is_null;
	Datum version;
	Oid namespace_oid;
	bool version_matches = false;

	if (!HeapTupleIsValid(tuple))
		elog(ERROR, "darmok_server extension disappeared during catalog preparation");
	extension = (Form_pg_extension) GETSTRUCT(tuple);
	namespace_oid = extension->extnamespace;
	version = SysCacheGetAttr(EXTENSIONOID, tuple, Anum_pg_extension_extversion,
							 &is_null);
	if (!is_null)
	{
		char *version_text = TextDatumGetCString(version);

		version_matches = strcmp(version_text, "1.0") == 0;
		pfree(version_text);
	}
	/* Do not carry our extension tuple pin through another native lookup. */
	ReleaseSysCache(tuple);
	if (!version_matches || namespace_oid != get_namespace_oid("darmok_server", false))
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("catalog discovery requires darmok_server 1.0 in its fixed namespace")));
}

static HTAB *
fact_hash(ReadState *state, const char *name, Size keysize, Size entrysize)
{
	HASHCTL ctl;

	memset(&ctl, 0, sizeof(ctl));
	ctl.keysize = keysize;
	ctl.entrysize = entrysize;
	ctl.hcxt = state->attempt;
	return hash_create(name, 256, &ctl, HASH_ELEM | HASH_BLOBS | HASH_CONTEXT);
}

static void
prepare_attempt(ReadState *state)
{
	int i;

	if (state->count != 0 && !darmok_relation_attempt_owned(&state->seed))
		elog(ERROR, "native catalog preparation requires its owned seed references");
	/* A previously completed publication has delivered its invalidations before
	 * dropping its transaction locks. All rebuild/cache/TOAST waits are here,
	 * outside Share; the next raw acquisition must match that observation. */
	AcceptInvalidationMessages();
	InvalidateCatalogSnapshot();
	darmok_catalog_verify_installation();
	if (state->count == 0)
		return;
	for (i = 0; i < FACT_HEAPS; i++)
	{
		state->heaps[i] = table_open(heap_oids[i], NoLock);
		if (state->heaps[i]->rd_tableam != GetHeapamTableAmRoutine())
			ereport(ERROR,
					(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
					 errmsg("catalog discovery requires builtin heap catalogs")));
	}
	state->namespaces = fact_hash(state, "darmok namespace OIDs", sizeof(Oid), sizeof(NamespaceFact));
	state->namespace_names = fact_hash(state, "darmok namespace names", sizeof(NameData), sizeof(NamespaceName));
	state->requested = fact_hash(state, "darmok requested relations", sizeof(RelationKey), sizeof(RequestedRelation));
	state->relations = fact_hash(state, "darmok relation OIDs", sizeof(Oid), sizeof(RelationFact));
	state->types = fact_hash(state, "darmok type OIDs", sizeof(Oid), sizeof(TypeFact));
}

static void
check_work_budget(ReadState *state)
{
	if (MemoryContextMemAllocated(state->attempt, true) > PHASE_BYTES)
		ereport(ERROR,
				(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
				 errmsg("catalog discovery working allocation exceeds 64 MiB")));
}

static void
copy_name(NameData *dest, const NameData *source)
{
	/* Normalize padding for byte-key hashes; names themselves remain exact. */
	memset(dest, 0, sizeof(*dest));
	strlcpy(NameStr(*dest), NameStr(*source), NAMEDATALEN);
}

static NamespaceFact *
namespace_fact(ReadState *state, Oid oid)
{
	NamespaceFact *fact = hash_search(state->namespaces, &oid, HASH_FIND, NULL);

	if (fact == NULL)
		ereport(ERROR,
				(errcode(ERRCODE_DATA_CORRUPTED),
				 errmsg("catalog namespace OID %u is missing from the observation", oid)));
	return fact;
}

static void
read_namespaces(ReadState *state)
{
	HeapTuple tuple;
	uint64 count = 0;
	uint32 i;

	while ((tuple = heap_getnext(state->scans[0], ForwardScanDirection)) != NULL)
	{
		Form_pg_namespace row = (Form_pg_namespace) GETSTRUCT(tuple);
		NamespaceFact *fact;
		NamespaceName *named;
		NameData key;
		bool found;

		copy_name(&key, &row->nspname);
		fact = hash_search(state->namespaces, &row->oid, HASH_ENTER, &found);
		if (found)
			elog(ERROR, "duplicate namespace OID in catalog observation");
		fact->name = key;
		named = hash_search(state->namespace_names, &key, HASH_ENTER, &found);
		if (found)
			elog(ERROR, "duplicate namespace name in catalog observation");
		named->oid = row->oid;
		if (++count % 1024 == 0)
			check_work_budget(state);
	}
	for (i = 0; i < state->count; i++)
	{
		RequestPair *pair = &state->pairs[i];
		NameData key;
		NamespaceName *named;
		RequestedRelation *requested;
		bool found;

		memset(&pair->key, 0, sizeof(pair->key));
		if (strlen(pair->schema) >= NAMEDATALEN || strlen(pair->name) >= NAMEDATALEN)
			continue;
		memset(&key, 0, sizeof(key));
		memcpy(NameStr(key), pair->schema, strlen(pair->schema));
		named = hash_search(state->namespace_names, &key, HASH_FIND, NULL);
		if (named == NULL)
			continue;
		pair->key.schema_oid = named->oid;
		memcpy(NameStr(pair->key.name), pair->name, strlen(pair->name));
		requested = hash_search(state->requested, &pair->key, HASH_ENTER, &found);
		if (!found)
			requested->oid = InvalidOid;
	}
}

static void
read_relations(ReadState *state)
{
	HeapTuple tuple;
	uint32 i;

	while ((tuple = heap_getnext(state->scans[1], ForwardScanDirection)) != NULL)
	{
		Form_pg_class row = (Form_pg_class) GETSTRUCT(tuple);
		RelationKey key;
		RequestedRelation *requested;
		RelationFact *fact;
		bool found;

		memset(&key, 0, sizeof(key));
		key.schema_oid = row->relnamespace;
		copy_name(&key.name, &row->relname);
		requested = hash_search(state->requested, &key, HASH_FIND, NULL);
		if (requested == NULL)
			continue;
		if (OidIsValid(requested->oid))
			elog(ERROR, "duplicate relation name in catalog observation");
		requested->oid = row->oid;
		fact = hash_search(state->relations, &row->oid, HASH_ENTER, &found);
		if (found)
			elog(ERROR, "duplicate relation OID in catalog observation");
		fact->schema_oid = row->relnamespace;
		fact->name = key.name;
		fact->kind = row->relkind;
		fact->persistence = row->relpersistence;
		fact->is_partition = row->relispartition;
		fact->columns = NIL;
	}
	for (i = 0; i < state->count; i++)
	{
		RequestedRelation *requested = hash_search(state->requested, &state->pairs[i].key,
												  HASH_FIND, NULL);
		if (requested == NULL || !OidIsValid(requested->oid))
			ereport(ERROR,
					(errcode(ERRCODE_UNDEFINED_TABLE),
					 errmsg("catalog relation at request index %u does not exist", i)));
	}
}

static void
read_columns(ReadState *state)
{
	HeapTuple tuple;
	uint64 count = 0;

	while ((tuple = heap_getnext(state->scans[2], ForwardScanDirection)) != NULL)
	{
		Form_pg_attribute row = (Form_pg_attribute) GETSTRUCT(tuple);
		RelationFact *relation;
		ColumnFact *fact;

		if (row->attnum <= 0 || row->attisdropped)
			continue;
		relation = hash_search(state->relations, &row->attrelid, HASH_FIND, NULL);
		if (relation == NULL)
			continue;
		fact = palloc(sizeof(ColumnFact));
		fact->number = row->attnum;
		copy_name(&fact->name, &row->attname);
		fact->type_oid = row->atttypid;
		fact->typmod = row->atttypmod;
		fact->dimensions = row->attndims;
		fact->not_null = row->attnotnull;
		fact->has_expression = row->atthasdef;
		fact->identity = row->attidentity;
		fact->generated = row->attgenerated;
		fact->collation = row->attcollation;
		relation->columns = lappend(relation->columns, fact);
		if (++count % 1024 == 0)
			check_work_budget(state);
	}
}

static void
read_types(ReadState *state)
{
	HeapTuple tuple;
	uint64 count = 0;

	while ((tuple = heap_getnext(state->scans[3], ForwardScanDirection)) != NULL)
	{
		Form_pg_type row = (Form_pg_type) GETSTRUCT(tuple);
		TypeFact *fact;
		bool found;

		fact = hash_search(state->types, &row->oid, HASH_ENTER, &found);
		if (found)
			elog(ERROR, "duplicate type OID in catalog observation");
		fact->schema_oid = row->typnamespace;
		copy_name(&fact->name, &row->typname);
		fact->kind = row->typtype;
		fact->category = row->typcategory;
		fact->element = row->typelem;
		fact->base = row->typbasetype;
		fact->typmod = row->typtypmod;
		fact->not_null = row->typnotnull;
		fact->dimensions = row->typndims;
		fact->visited = 0;
		if (++count % 1024 == 0)
			check_work_budget(state);
	}
}

static void
read_fixed_facts(ReadState *state)
{
	int i;

	if (state->count == 0)
		return;
	/* No invalidation dispatcher, index/cache lookup, detoasting, user AM or
	 * relation/tuple/XID lock acquisition occurs in this fixed native span.
	 * Catalog snapshots do not select the first transaction data snapshot. */
	InvalidateCatalogSnapshot();
	state->snapshot = RegisterSnapshot(GetNonHistoricCatalogSnapshot(RelationRelationId));
	for (i = 0; i < FACT_HEAPS; i++)
		state->scans[i] = heap_beginscan(state->heaps[i], state->snapshot,
									   0, NULL, NULL, SO_TYPE_SEQSCAN | SO_ALLOW_PAGEMODE);
	read_namespaces(state);
	read_relations(state);
	read_columns(state);
	read_types(state);
	check_work_budget(state);
}

static void
cleanup_attempt(ReadState *state)
{
	int i;

	/* The caller releases Share first, including on a partially initialized
	 * scan. All these resources also retain native ResourceOwner ownership. */
	for (i = 0; i < FACT_HEAPS; i++)
		if (state->scans[i] != NULL)
		{
			TableScanDesc scan = state->scans[i];

			state->scans[i] = NULL;
			heap_endscan(scan);
		}
	if (state->snapshot != NULL)
	{
		Snapshot snapshot = state->snapshot;

		state->snapshot = NULL;
		UnregisterSnapshot(snapshot);
		InvalidateCatalogSnapshot();
	}
	for (i = 0; i < FACT_HEAPS; i++)
		if (state->heaps[i] != NULL)
		{
			Relation heap = state->heaps[i];

			state->heaps[i] = NULL;
			table_close(heap, NoLock);
		}
}

static int
column_order(const ListCell *left, const ListCell *right)
{
	const ColumnFact *a = lfirst(left);
	const ColumnFact *b = lfirst(right);

	return (a->number > b->number) - (a->number < b->number);
}

static void
mark_type_chain(ReadState *state, Oid oid)
{
	List *path = NIL;
	ListCell *cell;

	for (;;)
	{
		TypeFact *fact = hash_search(state->types, &oid, HASH_FIND, NULL);

		if (fact == NULL)
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("catalog type OID %u is missing from the observation", oid)));
		if (fact->visited == 2)
			break;
		if (fact->visited == 1)
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("catalog domain ancestry contains a cycle")));
		fact->visited = 1;
		path = lappend(path, fact);
		if (fact->kind != TYPTYPE_DOMAIN)
			break;
		if (!OidIsValid(fact->base))
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("catalog domain has no base type")));
		oid = fact->base;
	}
	foreach(cell, path)
		((TypeFact *) lfirst(cell))->visited = 2;
	list_free(path);
}

static void
select_types(ReadState *state)
{
	HASH_SEQ_STATUS iter;
	RelationFact *relation;

	if (state->count == 0)
		return;
	hash_seq_init(&iter, state->relations);
	while ((relation = hash_seq_search(&iter)) != NULL)
	{
		ListCell *cell;

		list_sort(relation->columns, column_order);
		foreach(cell, relation->columns)
			mark_type_chain(state, ((ColumnFact *) lfirst(cell))->type_oid);
	}
	check_work_budget(state);
}

/* Fixed-field chunks are small. Check length before StringInfo growth, so a
 * 64 MiB buffer never doubles past the documented result phase budget. */
static void
response_append(StringInfo output, const char *fmt,...) pg_attribute_printf(2, 3);

static void
response_append(StringInfo output, const char *fmt,...)
{
	char chunk[2048];
	va_list args;
	int length;

	va_start(args, fmt);
	length = vsnprintf(chunk, sizeof(chunk), fmt, args);
	va_end(args);
	if (length < 0 || (Size) length >= sizeof(chunk))
		elog(ERROR, "catalog response fixed-field chunk is too large");
	if ((Size) output->len + length >= PHASE_BYTES)
		ereport(ERROR,
				(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
				 errmsg("catalog discovery response exceeds 64 MiB")));
	appendBinaryStringInfo(output, chunk, length);
}

static void
response_name(StringInfo output, const NameData *name)
{
	StringInfoData escaped;

	initStringInfo(&escaped);
	escape_json(&escaped, NameStr(*name));
	response_append(output, "%s", escaped.data);
	pfree(escaped.data);
}

static void
response_column(StringInfo output, ColumnFact *column)
{
	response_append(output, "{\"attribute_number\":%d,\"name\":", column->number);
	response_name(output, &column->name);
	response_append(output,
					",\"declared_type_oid\":%u,\"type_modifier\":%d,\"array_dimensions\":%d,"
					"\"not_null_constraint\":%s,\"has_expression\":%s,\"identity\":%u,"
					"\"generation\":%u,\"collation_oid\":%u}",
					column->type_oid, column->typmod, column->dimensions,
					column->not_null ? "true" : "false", column->has_expression ? "true" : "false",
					(unsigned char) column->identity, (unsigned char) column->generated, column->collation);
}

static StringInfo
build_response(ReadState *state)
{
	StringInfo output = makeStringInfo();
	HASH_SEQ_STATUS iter;
	RelationFact *relation;
	TypeFact *type;
	bool first = true;
	uint32 i;

	response_append(output, "{\"protocol\":1,\"stamp\":{\"cluster_id\":\"");
	for (i = 0; i < sizeof(state->stamp.cluster_id); i++)
		response_append(output, "%02x", state->stamp.cluster_id[i]);
	response_append(output,
					"\",\"database_oid\":%u,\"backend_id\":" UINT64_FORMAT ","
					"\"generation\":" UINT64_FORMAT ",\"local_generation\":" UINT64_FORMAT "},\"relation_oids\":[",
					state->stamp.database_oid, state->stamp.backend_id,
					state->stamp.generation, state->stamp.local_generation);
	for (i = 0; i < state->count; i++)
	{
		RequestedRelation *requested = hash_search(state->requested, &state->pairs[i].key, HASH_FIND, NULL);

		response_append(output, "%s%u", i == 0 ? "" : ",", requested->oid);
	}
	response_append(output, "],\"relations\":[");
	if (state->count != 0)
	{
		hash_seq_init(&iter, state->relations);
		while ((relation = hash_seq_search(&iter)) != NULL)
		{
			ListCell *cell;
			int16 previous = 0;

			response_append(output, "%s{\"oid\":%u,\"schema_oid\":%u,\"schema_name\":",
							first ? "" : ",", relation->oid, relation->schema_oid);
			first = false;
			response_name(output, &namespace_fact(state, relation->schema_oid)->name);
			response_append(output, ",\"name\":");
			response_name(output, &relation->name);
			response_append(output, ",\"kind\":%u,\"persistence\":%u,\"is_partition\":%s,\"columns\":[",
							(unsigned char) relation->kind, (unsigned char) relation->persistence,
							relation->is_partition ? "true" : "false");
			foreach(cell, relation->columns)
			{
				ColumnFact *column = lfirst(cell);

				if (column->number <= previous)
					elog(ERROR, "duplicate or unordered catalog attribute numbers");
				if (previous != 0)
					response_append(output, ",");
				response_column(output, column);
				previous = column->number;
			}
			response_append(output, "]}");
		}
	}
	response_append(output, "],\"types\":[");
	first = true;
	if (state->count != 0)
	{
		hash_seq_init(&iter, state->types);
		while ((type = hash_seq_search(&iter)) != NULL)
		{
			if (type->visited != 2)
				continue;
			response_append(output, "%s{\"oid\":%u,\"schema_oid\":%u,\"schema_name\":",
							first ? "" : ",", type->oid, type->schema_oid);
			first = false;
			response_name(output, &namespace_fact(state, type->schema_oid)->name);
			response_append(output, ",\"name\":");
			response_name(output, &type->name);
			response_append(output,
							",\"kind\":%u,\"category\":%u,\"element_type_oid\":%u,\"base_type_oid\":%u,"
							"\"base_type_modifier\":%d,\"not_null_constraint\":%s,\"array_dimensions\":%d}",
							(unsigned char) type->kind, (unsigned char) type->category, type->element,
							type->base, type->typmod, type->not_null ? "true" : "false", type->dimensions);
		}
	}
	response_append(output, "]}");
	return output;
}

void
darmok_catalog_show(ProcessUtilityContext context, DestReceiver *dest,
					QueryCompletion *completion)
{
	MemoryContext parent = CurrentMemoryContext;
	ReadState *state;
	volatile bool finished = false;

	if (context != PROCESS_UTILITY_TOPLEVEL)
		ereport(ERROR,
				(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
				 errmsg("catalog discovery requires a top-level SHOW")));
	if (GetDatabaseEncoding() != PG_UTF8)
		ereport(ERROR,
				(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
				 errmsg("catalog discovery requires a UTF8 database")));
	darmok_native_invocation_check();
	state = palloc0(sizeof(ReadState));
	state->first_snapshot_set = FirstSnapshotSet;
	state->subid = GetCurrentSubTransactionId();
	PG_TRY();
	{
		int attempt;
		bool complete = false;
		StringInfo response;
		TupOutputState *output;

		/* Failed native resources must outlive cleanup until their owning
		 * transaction/subtransaction abort releases them. */
		state->invocation = AllocSetContextCreate(CurTransactionContext,
												 "darmok catalog invocation", ALLOCSET_DEFAULT_SIZES);
		MemoryContextSwitchTo(state->invocation);
		parse_request(state);
		check_data_snapshot(state);
		for (attempt = 0; attempt < READ_ATTEMPTS; attempt++)
		{
			/* Lifecycle waiting has no seed, reader, scan or snapshot alive. */
			darmok_catalog_reader_start();
			darmok_catalog_fence_acquire(&state->observed);
			darmok_catalog_reader_finish();
			state->attempt = AllocSetContextCreate(state->invocation, "darmok catalog attempt", ALLOCSET_DEFAULT_SIZES);
			MemoryContextSwitchTo(state->attempt);
			if (state->count != 0)
				darmok_catalog_seed_acquire(&state->seed, heap_oids, FACT_HEAPS);
			darmok_native_refresh_start();
			PG_TRY(_prepare);
			{
				prepare_attempt(state);
			}
			PG_FINALLY(_prepare);
			{
				darmok_native_refresh_finish();
			}
			PG_END_TRY(_prepare);
			check_data_snapshot(state);
			darmok_catalog_reader_start();
			if (darmok_catalog_fence_try_acquire(&state->stamp) &&
				darmok_catalog_stamp_equal(&state->observed, &state->stamp))
			{
				read_fixed_facts(state);
				darmok_catalog_reader_finish();
				cleanup_attempt(state);
				if (darmok_relation_attempt_owned(&state->seed))
					darmok_relation_attempt_release(&state->seed);
				check_data_snapshot(state);
				select_types(state);
				complete = true;
				break;
			}
			darmok_catalog_reader_finish();
			cleanup_attempt(state);
			if (darmok_relation_attempt_owned(&state->seed))
				darmok_relation_attempt_release(&state->seed);
			check_data_snapshot(state);
			MemoryContextSwitchTo(state->invocation);
			MemoryContextDelete(state->attempt);
			state->attempt = NULL;
		}
		if (!complete)
			ereport(ERROR,
					(errcode(ERRCODE_T_R_SERIALIZATION_FAILURE),
					 errmsg("catalog publication changed through 16 discovery attempts")));
		darmok_catalog_reader_finish();
		state->result = AllocSetContextCreate(state->invocation, "darmok catalog response", ALLOCSET_DEFAULT_SIZES);
		MemoryContextSwitchTo(state->result);
		response = build_response(state);
		if (MemoryContextMemAllocated(state->result, true) > PHASE_BYTES)
			ereport(ERROR,
					(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
					 errmsg("catalog discovery serialized allocation exceeds 64 MiB")));
		/* Native descriptor, TEXT output and receiver work run after release. */
		output = begin_tup_output_tupdesc(dest, GetPGVariableResultDesc(DARMOK_CATALOG_REQUEST), &TTSOpsVirtual);
		do_text_output_oneline(output, response->data);
		end_tup_output(output);
		check_data_snapshot(state);
		SetQueryCompletion(completion, CMDTAG_SHOW, 0);
		finished = true;
	}
	PG_FINALLY();
	{
		PG_TRY(_cleanup);
		{
			darmok_catalog_reader_finish();
			cleanup_attempt(state);
			if (darmok_relation_attempt_owned(&state->seed))
				darmok_relation_attempt_release(&state->seed);
		}
		PG_FINALLY(_cleanup);
		{
			if (!finished)
				darmok_native_invocation_require_abort(state->subid);
			MemoryContextSwitchTo(parent);
			if (finished && state->invocation != NULL)
				MemoryContextDelete(state->invocation);
			pfree(state);
		}
		PG_END_TRY(_cleanup);
	}
	PG_END_TRY();
}
