/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#include "postgres.h"

#include "access/heapam.h"
#include "access/htup_details.h"
#include "access/relation.h"
#include "access/table.h"
#include "access/tableam.h"
#include "catalog/catalog.h"
#include "catalog/namespace.h"
#include "catalog/pg_am_d.h"
#include "catalog/pg_attribute.h"
#include "catalog/pg_attrdef.h"
#include "catalog/pg_class.h"
#include "catalog/pg_index.h"
#include "catalog/pg_namespace.h"
#include "catalog/pg_namespace_d.h"
#include "catalog/pg_tablespace_d.h"
#include "catalog/pg_type.h"
#include "mb/pg_wchar.h"
#include "miscadmin.h"
#include "storage/proc.h"
#include "utils/array.h"
#include "utils/hsearch.h"
#include "utils/inval.h"
#include "utils/memutils.h"
#include "utils/relmapper.h"
#include "utils/rel.h"
#include "utils/resowner.h"
#include "utils/snapmgr.h"

#include "catalog_read.h"
#include "catalog_payload.h"
#include "heap_storage.h"
#include "statement_guard.h"

#define STORAGE_ATTEMPTS 16
#define STORAGE_NAME_BYTES ((Size) 1024 * 1024)
#define STORAGE_PHASE_BYTES DARMOK_CATALOG_PHASE_BYTES
#define STORAGE_HEAPS 6
#define STORAGE_TOAST_HEAPS 2
#define STORAGE_CRITICAL_INDEXES 2
#define STORAGE_USES (2 * DARMOK_RELATION_REQUEST_LIMIT)

StaticAssertDecl(ATTRIBUTE_FIXED_PART_SIZE ==
				 offsetof(FormData_pg_attribute, attcollation) + sizeof(Oid),
				 "native attribute fixed prefix must end at attcollation");

static const Oid fact_heap_oids[STORAGE_HEAPS] = {
	NamespaceRelationId, RelationRelationId, IndexRelationId,
	AttributeRelationId, TypeRelationId, AttrDefaultRelationId
};
static const Oid payload_toast_oids[STORAGE_TOAST_HEAPS] = {
	2830, 4171 /* Compiled bootstrap pg_attrdef and pg_type TOAST identities. */
};
static const Oid critical_index_oids[STORAGE_CRITICAL_INDEXES] = {
	ClassOidIndexId, AttributeRelidNumIndexId
};
static bool storage_active = false;

typedef struct StorageNamespace
{
	Oid oid;
	NameData name;
} StorageNamespace;

typedef struct StorageNamespaceName
{
	NameData name;
	Oid oid;
} StorageNamespaceName;

typedef struct StorageNameKey
{
	Oid schema_oid;
	NameData name;
} StorageNameKey;

typedef struct StorageNamedRelation
{
	StorageNameKey key;
	Oid oid;
} StorageNamedRelation;

typedef struct StorageIndex
{
	Oid oid;
	Oid heap_oid;
	bool live;
	bool ready;
	bool valid;
	bool check_xmin;
} StorageIndex;

typedef struct StorageClass
{
	Oid oid;
	DarmokHeapStorageFact fact;
	List *indexes;
	Oid rewrite_oid;
	int16 checks;
	bool rules;
	bool triggers;
	bool row_security;
	bool options_null;
} StorageClass;

typedef struct StorageInput
{
	NameData schema;
	NameData name;
	LOCKMODE mode;
} StorageInput;

typedef struct StorageMode
{
	Oid oid;
	uint8 mask;
	bool catalog;
} StorageMode;

typedef struct StorageNode
{
	Oid oid;
	int position;
} StorageNode;

typedef struct StorageRoot
{
	Oid oid;
	int attribute_offset;
} StorageRoot;

typedef struct StorageAttribute
{
	/* Exact positive ordinal key, with no native struct padding in the hash. */
	uint64 key;
	DarmokHeapAttributeFact fact;
	DarmokCatalogCarrier missing_carrier;
} StorageAttribute;

typedef struct StorageType
{
	Oid oid;
	bool copied;
	DarmokHeapTypeFact fact;
	DarmokCatalogCarrier binary_default;
	DarmokCatalogCarrier text_default;
} StorageType;

typedef struct StorageExpression
{
	uint64 key;
	Oid oid;
	DarmokCatalogCarrier carrier;
} StorageExpression;

typedef struct StorageProfileAttribute
{
	uint64 key;
	DarmokHeapAttributeFact fact;
	bool missing_null;
} StorageProfileAttribute;

typedef struct StoragePayloadSource
{
	Oid catalog_oid;
	Oid row_oid;
	Oid relation_oid;
	int16 number;
	int16 field_number;
	const DarmokCatalogCarrier *carrier;
} StoragePayloadSource;

typedef struct StorageObservation
{
	MemoryContext context;
	Relation heaps[STORAGE_HEAPS];
	TableScanDesc scans[STORAGE_HEAPS];
	Snapshot snapshot;
	Relation toast_heaps[STORAGE_TOAST_HEAPS];
	Relation critical_indexes[STORAGE_CRITICAL_INDEXES];
	HTAB *namespaces;
	HTAB *namespace_names;
	HTAB *classes;
	HTAB *relation_names;
	HTAB *indexes;
	HTAB *selected_roots;
	HTAB *attribute_rows;
	HTAB *type_rows;
	HTAB *expression_rows;
	HTAB *expression_ids;
	HTAB *profile_attributes;
	DarmokCatalogImageBudget image_budget;
	DarmokCatalogStamp stamp;
	DarmokRelationRequest *roots;
	DarmokHeapStorageRootFact *root_facts;
	DarmokHeapAttributeFact *attributes;
	DarmokHeapTypeFact *types;
	DarmokHeapMissingFact *missing;
	DarmokHeapCatalogPayloadFact *payloads;
	DarmokCatalogPayloadRequest *payload_requests;
	int payload_count;
	DarmokCatalogPayloadCost payload_cost;
	int attribute_count;
	int type_count;
	int missing_count;
	Size missing_image_bytes;
	DarmokHeapObservationCost cost;
	DarmokHeapStorageFact *facts;
	DarmokRelationRequest *references;
	DarmokHeapStorageUse *uses;
	HTAB *nodes;
	int fact_count;
	int reference_count;
	int use_count;
} StorageObservation;

typedef struct StorageState
{
	MemoryContext invocation;
	MemoryContext parent;
	ResourceOwner owner;
	ResourceOwner transaction_owner;
	SubTransactionId subid;
	Oid database_oid;
	Oid database_tablespace;
	Oid temp_namespace;
	Oid temp_toast_namespace;
	int32 proc_number;
	bool first_snapshot_set;
	StorageInput *inputs;
	int count;
	StorageObservation initial;
	StorageObservation source;
	StorageObservation final;
	volatile DarmokRelationAttempt physical;
	volatile DarmokStatementGuard semantic;
} StorageState;

static void
storage_context_check(StorageState *state)
{
	Oid temp;
	Oid toast;

	GetTempNamespaceState(&temp, &toast);
	if (FirstSnapshotSet != state->first_snapshot_set)
		ereport(ERROR,
				(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
				 errmsg("native storage changed the data-snapshot state")));
	if (CurrentResourceOwner != state->owner ||
		CurTransactionResourceOwner != state->transaction_owner ||
		GetCurrentSubTransactionId() != state->subid ||
		MyDatabaseId != state->database_oid ||
		MyDatabaseTableSpace != state->database_tablespace ||
		(int32) MyProcNumber != state->proc_number ||
		temp != state->temp_namespace || toast != state->temp_toast_namespace ||
		(OidIsValid(temp) && MyProc->tempNamespaceId != temp))
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("native storage owning context changed")));
}

static void
storage_budget(StorageObservation *observation)
{
	if (MemoryContextMemAllocated(observation->context, true) > STORAGE_PHASE_BYTES)
		ereport(ERROR,
				(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
				 errmsg("native storage copied phase exceeds 64 MiB")));
}

static void
storage_name_copy(NameData *dest, const NameData *source)
{
	const char *end = memchr(NameStr(*source), '\0', NAMEDATALEN);

	if (end == NULL)
		ereport(ERROR,
				(errcode(ERRCODE_DATA_CORRUPTED),
				 errmsg("unterminated native catalog name")));
	memset(dest, 0, sizeof(*dest));
	memcpy(NameStr(*dest), NameStr(*source), end - NameStr(*source));
}

static HTAB *
storage_hash(StorageObservation *observation, const char *name,
			 Size key_size, Size entry_size)
{
	HASHCTL ctl = {0};

	ctl.keysize = key_size;
	ctl.entrysize = entry_size;
	ctl.hcxt = observation->context;
	return hash_create(name, 256, &ctl, HASH_ELEM | HASH_BLOBS | HASH_CONTEXT);
}

static void
storage_observation_init(StorageState *state, StorageObservation *observation)
{
	observation->context = AllocSetContextCreate(state->invocation,
											   "darmok heap storage observation",
											   ALLOCSET_DEFAULT_SIZES);
	MemoryContextSwitchTo(observation->context);
	observation->namespaces = storage_hash(observation, "storage namespace OIDs",
										 sizeof(Oid), sizeof(StorageNamespace));
	observation->namespace_names = storage_hash(observation, "storage namespace names",
											   sizeof(NameData), sizeof(StorageNamespaceName));
	observation->classes = storage_hash(observation, "storage relation OIDs",
									  sizeof(Oid), sizeof(StorageClass));
	observation->relation_names = storage_hash(observation, "storage relation names",
											 sizeof(StorageNameKey), sizeof(StorageNamedRelation));
	observation->indexes = storage_hash(observation, "storage index OIDs",
									  sizeof(Oid), sizeof(StorageIndex));
	observation->selected_roots = storage_hash(observation, "storage selected roots",
											 sizeof(Oid), sizeof(StorageRoot));
	observation->attribute_rows = storage_hash(observation, "storage positive slots",
											 sizeof(uint64), sizeof(StorageAttribute));
	observation->type_rows = storage_hash(observation, "storage live type OIDs",
									 sizeof(Oid), sizeof(StorageType));
	observation->expression_rows = storage_hash(observation, "storage column expressions",
												 sizeof(uint64), sizeof(StorageExpression));
	observation->expression_ids = storage_hash(observation, "storage expression OIDs",
												 sizeof(Oid), sizeof(Oid));
	observation->profile_attributes = storage_hash(observation, "storage TOAST profile slots",
													 sizeof(uint64), sizeof(StorageProfileAttribute));
	observation->image_budget.context = observation->context;
	observation->image_budget.limit = STORAGE_PHASE_BYTES;
}

static void
storage_target_layout(Relation relation, int natts, AttrNumber number,
					  Oid type_oid, char alignment)
{
	TupleDesc descriptor = RelationGetDescr(relation);
	Form_pg_attribute target;

	if (descriptor->natts != natts)
		elog(ERROR, "native storage catalog descriptor has an unexpected column count");
	target = TupleDescAttr(descriptor, number - 1);
	if (target->attrelid != RelationGetRelid(relation) || target->attnum != number ||
		target->attisdropped || target->atttypid != type_oid || target->attlen != -1 ||
		target->attbyval || target->attalign != alignment)
		elog(ERROR, "native storage catalog carrier layout is inconsistent");
}

static void
storage_prepare(StorageObservation *observation)
{
	darmok_native_refresh_start();
	PG_TRY();
	{
		/* Even exact already-clear modes can skip native SI. All descriptor,
		 * namespace/extension cache and mapping work is outside exclusion. */
		AcceptInvalidationMessages();
		InvalidateCatalogSnapshot();
		darmok_catalog_verify_installation();
		if (!criticalRelcachesBuilt)
			elog(ERROR, "native storage requires completed native critical relcache initialization");
		for (int i = 0; i < STORAGE_HEAPS; i++)
		{
			observation->heaps[i] = table_open(fact_heap_oids[i], AccessShareLock);
			if (observation->heaps[i]->rd_tableam != GetHeapamTableAmRoutine())
				ereport(ERROR,
						(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
						 errmsg("native storage requires builtin heap catalogs")));
		}
		if (RelationGetDescr(observation->heaps[0])->natts != Natts_pg_namespace ||
			RelationGetDescr(observation->heaps[2])->natts != Natts_pg_index)
			elog(ERROR, "native storage fixed catalog layout is inconsistent");
		storage_target_layout(observation->heaps[1], Natts_pg_class,
			Anum_pg_class_reloptions, TEXTARRAYOID, TYPALIGN_DOUBLE);
		storage_target_layout(observation->heaps[3], Natts_pg_attribute,
			Anum_pg_attribute_attmissingval, ANYARRAYOID, TYPALIGN_DOUBLE);
		storage_target_layout(observation->heaps[4], Natts_pg_type,
			Anum_pg_type_typdefaultbin, PG_NODE_TREEOID, TYPALIGN_INT);
		storage_target_layout(observation->heaps[4], Natts_pg_type,
			Anum_pg_type_typdefault, TEXTOID, TYPALIGN_INT);
		storage_target_layout(observation->heaps[5], Natts_pg_attrdef,
			Anum_pg_attrdef_adbin, PG_NODE_TREEOID, TYPALIGN_INT);
	}
	PG_FINALLY();
	{
		darmok_native_refresh_finish();
	}
	PG_END_TRY();
}

static Oid
storage_literal_oid(StorageObservation *observation, const StorageInput *input, int position)
{
	StorageNamespaceName *schema = hash_search(observation->namespace_names,
											 &input->schema, HASH_FIND, NULL);
	StorageNameKey key = {0};
	StorageNamedRelation *named;

	if (schema == NULL)
		ereport(ERROR,
				(errcode(ERRCODE_UNDEFINED_TABLE),
				 errmsg("native storage schema at root %d does not exist", position)));
	key.schema_oid = schema->oid;
	key.name = input->name;
	named = hash_search(observation->relation_names, &key, HASH_FIND, NULL);
	if (named == NULL)
		ereport(ERROR,
				(errcode(ERRCODE_UNDEFINED_TABLE),
				 errmsg("native storage relation at root %d does not exist", position)));
	return named->oid;
}

static void
storage_copy_attribute(DarmokHeapAttributeFact *fact, const FormData_pg_attribute *row)
{
	/* Compiled native fixed prefix only: no attcacheoff/CompactAttribute or
	 * variable field deformation. Normalize names and compare explicit fields. */
	memset(fact, 0, sizeof(*fact));
	fact->relation_oid = row->attrelid;
	fact->number = row->attnum;
	storage_name_copy(&fact->name, &row->attname);
	fact->type_oid = row->atttypid;
	fact->length = row->attlen;
	fact->typmod = row->atttypmod;
	fact->dimensions = row->attndims;
	fact->by_value = row->attbyval;
	fact->alignment = row->attalign;
	fact->storage = row->attstorage;
	fact->compression = row->attcompression;
	fact->not_null_declared = row->attnotnull;
	fact->has_default = row->atthasdef;
	fact->has_missing = row->atthasmissing;
	fact->identity = row->attidentity;
	fact->generated = row->attgenerated;
	fact->dropped = row->attisdropped;
	fact->local = row->attislocal;
	fact->inheritance_count = row->attinhcount;
	fact->collation_oid = row->attcollation;
}

static void
storage_copy_type(DarmokHeapTypeFact *fact, const FormData_pg_type *row)
{
	memset(fact, 0, sizeof(*fact));
	fact->oid = row->oid;
	fact->schema_oid = row->typnamespace;
	storage_name_copy(&fact->name, &row->typname);
	fact->length = row->typlen;
	fact->by_value = row->typbyval;
	fact->kind = row->typtype;
	fact->category = row->typcategory;
	fact->preferred = row->typispreferred;
	fact->defined = row->typisdefined;
	fact->delimiter = row->typdelim;
	fact->relation_oid = row->typrelid;
	fact->subscript_oid = row->typsubscript;
	fact->element_oid = row->typelem;
	fact->array_oid = row->typarray;
	fact->input_oid = row->typinput;
	fact->output_oid = row->typoutput;
	fact->receive_oid = row->typreceive;
	fact->send_oid = row->typsend;
	fact->typmod_input_oid = row->typmodin;
	fact->typmod_output_oid = row->typmodout;
	fact->analyze_oid = row->typanalyze;
	fact->alignment = row->typalign;
	fact->storage = row->typstorage;
	fact->not_null_declared = row->typnotnull;
	fact->base_type_oid = row->typbasetype;
	fact->typmod = row->typtypmod;
	fact->dimensions = row->typndims;
	fact->collation_oid = row->typcollation;
}

static void
storage_copy_missing(StorageObservation *observation, StorageAttribute *attribute,
					 HeapTuple tuple)
{
	DarmokCatalogCarrier *carrier = &attribute->missing_carrier;

	darmok_catalog_carrier_copy(&observation->image_budget, carrier, tuple,
		RelationGetDescr(observation->heaps[3]), Anum_pg_attribute_attmissingval, false);
	if (attribute->fact.has_missing != carrier->present ||
		(carrier->present && (attribute->fact.dropped || !OidIsValid(attribute->fact.type_oid))))
		elog(ERROR, "native storage missing declaration and physical carrier disagree");
	observation->cost.missing_carrier_bytes += carrier->bytes;
	if (carrier->present)
	{
		if (observation->missing_count == PG_INT32_MAX)
			elog(ERROR, "native storage missing value count exceeds native limits");
		observation->missing_count++;
	}
}

static void
storage_normalize_missing(StorageObservation *observation)
{
	int position = 0;

	/* Only B's invocation-owned carriers, outside raw/S, with its registered
	 * source snapshot still alive. Default declarations remain independent. */
	MemoryContextSwitchTo(observation->context);
	if ((Size) observation->missing_count > MaxAllocSize / sizeof(DarmokHeapMissingFact))
		elog(ERROR, "native storage missing fact array exceeds native limits");
	if (observation->missing_count > 0)
		observation->missing = darmok_catalog_image_alloc(&observation->image_budget,
			sizeof(DarmokHeapMissingFact) * (Size) observation->missing_count, true);
	for (int i = 0; i < observation->attribute_count; i++)
	{
		const DarmokHeapAttributeFact *fact = &observation->attributes[i];
		uint64 key = ((uint64) fact->relation_oid << 32) | (uint16) fact->number;
		StorageAttribute *attribute;
		DarmokHeapPayloadImage value;
		ArrayType *image;
		DarmokHeapMissingFact *missing;
		Size expanded;

		if (!fact->has_missing)
			continue;
		attribute = hash_search(observation->attribute_rows, &key, HASH_FIND, NULL);
		if (attribute == NULL || !attribute->missing_carrier.present ||
			position >= observation->missing_count)
			elog(ERROR, "native storage lost a copied missing carrier");
		darmok_catalog_carrier_image(&observation->image_budget, &attribute->missing_carrier, &value);
		image = (ArrayType *) value.image;
		expanded = value.image_bytes;
		if (expanded < ARR_OVERHEAD_NONULLS(1) || ARR_NDIM(image) != 1 ||
			ARR_ELEMTYPE(image) != fact->type_oid || ARR_DIMS(image)[0] != 1 || ARR_LBOUND(image)[0] != 1)
			elog(ERROR, "native storage missing image has an inconsistent singleton array envelope");
		if (ARR_HASNULL(image))
		{
			if (expanded < ARR_OVERHEAD_WITHNULLS(1, 1) ||
				image->dataoffset != ARR_OVERHEAD_WITHNULLS(1, 1) ||
				(ARR_NULLBITMAP(image)[0] & 1) == 0)
				elog(ERROR, "native storage missing image has a NULL element or invalid bitmap offset");
		}
		if ((Size) ARR_DATA_OFFSET(image) > expanded)
			elog(ERROR, "native storage missing image has an out-of-range data offset");
		missing = &observation->missing[position++];
		missing->relation_oid = fact->relation_oid;
		missing->number = fact->number;
		missing->type_oid = fact->type_oid;
		missing->carrier_kind = value.carrier_kind;
		missing->stored_bytes = value.stored_bytes;
		missing->image_bytes = expanded;
		missing->image = value.image;
		if (expanded > STORAGE_PHASE_BYTES - observation->missing_image_bytes)
			elog(ERROR, "native storage missing image accounting exceeds phase limits");
		observation->missing_image_bytes += expanded;
		storage_budget(observation);
	}
	if (position != observation->missing_count)
		elog(ERROR, "native storage copied missing count disagrees with selected columns");
	storage_budget(observation);
}

static bool
storage_toast_profile_oid(Oid oid)
{
	return oid == payload_toast_oids[0] || oid == payload_toast_oids[1];
}

static bool
storage_descriptor_profile_oid(Oid oid)
{
	return storage_toast_profile_oid(oid) || oid == critical_index_oids[0] ||
		oid == critical_index_oids[1];
}

static void
storage_read_fixed(StorageState *state, StorageObservation *observation)
{
	HeapTuple tuple;
	uint64 rows = 0;

	/* Catalog snapshots never select the first transaction data snapshot.
	 * Descriptor preparation has ended: fixed fields and raw carrier copy.
	 * Copying performs bounded native allocation; budget checks are periodic. */
	InvalidateCatalogSnapshot();
	observation->snapshot = RegisterSnapshot(GetNonHistoricCatalogSnapshot(RelationRelationId));
	for (int i = 0; i < STORAGE_HEAPS; i++)
		observation->scans[i] = heap_beginscan(observation->heaps[i], observation->snapshot,
											0, NULL, NULL, SO_TYPE_SEQSCAN | SO_ALLOW_PAGEMODE);
	while ((tuple = heap_getnext(observation->scans[0], ForwardScanDirection)) != NULL)
	{
		Form_pg_namespace row = (Form_pg_namespace) GETSTRUCT(tuple);
		StorageNamespace *fact;
		StorageNamespaceName *named;
		NameData key;
		bool found;

		observation->cost.namespace_rows++;
		storage_name_copy(&key, &row->nspname);
		fact = hash_search(observation->namespaces, &row->oid, HASH_ENTER, &found);
		if (found)
			elog(ERROR, "duplicate namespace OID in native storage observation");
		fact->name = key;
		named = hash_search(observation->namespace_names, &key, HASH_ENTER, &found);
		if (found)
			elog(ERROR, "duplicate namespace name in native storage observation");
		named->oid = row->oid;
		if (++rows % 1024 == 0)
			storage_budget(observation);
	}
	while ((tuple = heap_getnext(observation->scans[1], ForwardScanDirection)) != NULL)
	{
		Form_pg_class row = (Form_pg_class) GETSTRUCT(tuple);
		StorageClass *fact;
		StorageNamedRelation *named;
		StorageNameKey key = {0};
		bool found;

		observation->cost.relation_rows++;
		key.schema_oid = row->relnamespace;
		storage_name_copy(&key.name, &row->relname);
		named = hash_search(observation->relation_names, &key, HASH_ENTER, &found);
		if (found)
			elog(ERROR, "duplicate relation name in native storage observation");
		named->oid = row->oid;
		fact = hash_search(observation->classes, &row->oid, HASH_ENTER, &found);
		if (found)
			elog(ERROR, "duplicate relation OID in native storage observation");
		memset(fact, 0, sizeof(*fact));
		fact->oid = row->oid;
		fact->fact.oid = row->oid;
		fact->fact.schema_oid = row->relnamespace;
		fact->fact.name = key.name;
		fact->fact.kind = row->relkind;
		fact->fact.persistence = row->relpersistence;
		fact->fact.access_method_oid = row->relam;
		fact->fact.toast_oid = row->reltoastrelid;
		fact->fact.tablespace_oid = row->reltablespace;
		fact->fact.stored_file_number = row->relfilenode;
		fact->fact.row_type_oid = row->reltype;
		fact->fact.declared_attribute_count = row->relnatts;
		fact->fact.shared = row->relisshared;
		fact->fact.is_partition = row->relispartition;
		fact->fact.has_indexes = row->relhasindex;
		fact->fact.has_subclasses = row->relhassubclass;
		fact->rewrite_oid = row->relrewrite;
		fact->checks = row->relchecks;
		fact->rules = row->relhasrules;
		fact->triggers = row->relhastriggers;
		fact->row_security = row->relrowsecurity;
		if (storage_descriptor_profile_oid(row->oid))
		{
			bool is_null = false;

			(void) darmok_catalog_present_attr(tuple, RelationGetDescr(observation->heaps[1]),
				Anum_pg_class_reloptions, &is_null);
			fact->options_null = is_null;
		}
		if (++rows % 1024 == 0)
			storage_budget(observation);
	}
	{
		StorageClass *attribute_catalog = hash_search(observation->classes,
													   &fact_heap_oids[3], HASH_FIND, NULL);

		if (attribute_catalog == NULL || attribute_catalog->fact.kind != RELKIND_RELATION ||
			attribute_catalog->fact.access_method_oid != HEAP_TABLE_AM_OID ||
			attribute_catalog->fact.is_partition ||
			OidIsValid(attribute_catalog->fact.toast_oid) ||
			attribute_catalog->fact.declared_attribute_count != Natts_pg_attribute)
			ereport(ERROR,
					(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
					 errmsg("native storage missing values require builtin no-TOAST pg_attribute")));
	}
	while ((tuple = heap_getnext(observation->scans[2], ForwardScanDirection)) != NULL)
	{
		Form_pg_index row = (Form_pg_index) GETSTRUCT(tuple);
		StorageIndex *fact;
		bool found;

		observation->cost.index_rows++;
		fact = hash_search(observation->indexes, &row->indexrelid, HASH_ENTER, &found);
		if (found)
			elog(ERROR, "duplicate index OID in native storage observation");
		fact->heap_oid = row->indrelid;
		fact->live = row->indislive;
		fact->ready = row->indisready;
		fact->valid = row->indisvalid;
		fact->check_xmin = row->indcheckxmin;
		if (++rows % 1024 == 0)
			storage_budget(observation);
	}
	/* Selection is pure fixed-byte/OID hashing within this SAME registered
	 * snapshot/raw observation. Full graph/type validation follows outside raw. */
	for (int i = 0; i < state->count; i++)
	{
		Oid oid = storage_literal_oid(observation, &state->inputs[i], i);
		StorageRoot *root;
		bool found;

		root = hash_search(observation->selected_roots, &oid, HASH_ENTER, &found);
		if (!found)
			root->attribute_offset = 0;
	}
	while ((tuple = heap_getnext(observation->scans[3], ForwardScanDirection)) != NULL)
	{
		Form_pg_attribute row = (Form_pg_attribute) GETSTRUCT(tuple);

		observation->cost.attribute_rows++;
		if (row->attnum > 0 && storage_toast_profile_oid(row->attrelid))
		{
			uint64 key = ((uint64) row->attrelid << 32) | (uint16) row->attnum;
			StorageProfileAttribute *profile;
			bool found;
			bool is_null = false;

			profile = hash_search(observation->profile_attributes, &key, HASH_ENTER, &found);
			if (found)
				elog(ERROR, "duplicate native catalog TOAST profile ordinal");
			storage_copy_attribute(&profile->fact, row);
			(void) darmok_catalog_present_attr(tuple, RelationGetDescr(observation->heaps[3]),
				Anum_pg_attribute_attmissingval, &is_null);
			profile->missing_null = is_null;
		}
		if (row->attnum > 0 &&
			hash_search(observation->selected_roots, &row->attrelid, HASH_FIND, NULL) != NULL)
		{
			uint64 key = ((uint64) row->attrelid << 32) | (uint16) row->attnum;
			StorageAttribute *attribute;
			bool found;

			attribute = hash_search(observation->attribute_rows, &key, HASH_ENTER, &found);
			if (found)
				elog(ERROR, "duplicate positive slot in native storage observation");
			storage_copy_attribute(&attribute->fact, row);
			storage_copy_missing(observation, attribute, tuple);
			if (!row->attisdropped && OidIsValid(row->atttypid))
			{
				StorageType *type = hash_search(observation->type_rows, &row->atttypid,
											  HASH_ENTER, &found);

				if (!found)
				{
					type->copied = false;
					memset(&type->fact, 0, sizeof(type->fact));
					memset(&type->binary_default, 0, sizeof(type->binary_default));
					memset(&type->text_default, 0, sizeof(type->text_default));
				}
			}
		}
		if (++rows % 1024 == 0)
			storage_budget(observation);
	}
	while ((tuple = heap_getnext(observation->scans[4], ForwardScanDirection)) != NULL)
	{
		Form_pg_type row = (Form_pg_type) GETSTRUCT(tuple);
		StorageType *type;

		observation->cost.type_rows++;
		type = hash_search(observation->type_rows, &row->oid, HASH_FIND, NULL);
		if (type != NULL)
		{
			if (type->copied)
				elog(ERROR, "duplicate live type OID in native storage observation");
			storage_copy_type(&type->fact, row);
			darmok_catalog_carrier_copy(&observation->image_budget, &type->binary_default,
				tuple, RelationGetDescr(observation->heaps[4]), Anum_pg_type_typdefaultbin, true);
			darmok_catalog_carrier_copy(&observation->image_budget, &type->text_default,
				tuple, RelationGetDescr(observation->heaps[4]), Anum_pg_type_typdefault, true);
			if (type->binary_default.present && !type->text_default.present)
				elog(ERROR, "native type binary default has no text default");
			observation->cost.payload_carrier_bytes += type->binary_default.bytes + type->text_default.bytes;
			type->copied = true;
		}
		if (++rows % 1024 == 0)
			storage_budget(observation);
	}
	while ((tuple = heap_getnext(observation->scans[5], ForwardScanDirection)) != NULL)
	{
		Form_pg_attrdef row = (Form_pg_attrdef) GETSTRUCT(tuple);

		observation->cost.attrdef_rows++;
		if (storage_toast_profile_oid(row->adrelid))
			elog(ERROR, "native catalog TOAST profile has a contradictory expression row");
		if (hash_search(observation->selected_roots, &row->adrelid, HASH_FIND, NULL) != NULL)
		{
			uint64 key;
			StorageAttribute *attribute;
			StorageExpression *expression;
			bool found;

			/* Inspect actual relation/ordinal even when its declaration says no
			 * default. Do not filter contradictory/out-of-range rows away. */
			if (row->adnum <= 0 || !OidIsValid(row->oid))
				elog(ERROR, "native column expression has an invalid source identity");
			key = ((uint64) row->adrelid << 32) | (uint16) row->adnum;
			attribute = hash_search(observation->attribute_rows, &key, HASH_FIND, NULL);
			if (attribute == NULL || attribute->fact.dropped || !attribute->fact.has_default)
				elog(ERROR, "native column expression contradicts its positive column declaration");
			(void) hash_search(observation->expression_ids, &row->oid, HASH_ENTER, &found);
			if (found)
				elog(ERROR, "duplicate native column expression source OID");
			expression = hash_search(observation->expression_rows, &key, HASH_ENTER, &found);
			if (found)
				elog(ERROR, "duplicate native column expression ordinal");
			expression->oid = row->oid;
			darmok_catalog_carrier_copy(&observation->image_budget, &expression->carrier,
				tuple, RelationGetDescr(observation->heaps[5]), Anum_pg_attrdef_adbin, true);
			if (!expression->carrier.present)
				elog(ERROR, "native column expression has a NULL binary carrier");
			observation->cost.payload_carrier_bytes += expression->carrier.bytes;
		}
		if (++rows % 1024 == 0)
			storage_budget(observation);
	}
	storage_budget(observation);
}

static void
storage_end_scans(StorageObservation *observation)
{
	for (int i = 0; i < STORAGE_HEAPS; i++)
		if (observation->scans[i] != NULL)
		{
			heap_endscan(observation->scans[i]);
			observation->scans[i] = NULL;
		}
}

static void
storage_close_reader(StorageObservation *observation)
{
	storage_end_scans(observation);
	if (observation->snapshot != NULL)
	{
		UnregisterSnapshot(observation->snapshot);
		observation->snapshot = NULL;
		InvalidateCatalogSnapshot();
	}
	for (int i = 0; i < STORAGE_TOAST_HEAPS; i++)
		if (observation->toast_heaps[i] != NULL)
		{
			table_close(observation->toast_heaps[i], NoLock);
			observation->toast_heaps[i] = NULL;
		}
	for (int i = 0; i < STORAGE_CRITICAL_INDEXES; i++)
		if (observation->critical_indexes[i] != NULL)
		{
			relation_close(observation->critical_indexes[i], NoLock);
			observation->critical_indexes[i] = NULL;
		}
	for (int i = 0; i < STORAGE_HEAPS; i++)
		if (observation->heaps[i] != NULL)
		{
			table_close(observation->heaps[i], AccessShareLock);
			observation->heaps[i] = NULL;
		}
}

static bool
storage_stamp_equal(const DarmokCatalogStamp *a, const DarmokCatalogStamp *b)
{
	return darmok_catalog_stamp_equal(a, b) &&
		a->database_oid == b->database_oid && a->backend_id == b->backend_id &&
		memcmp(a->cluster_id, b->cluster_id, sizeof(a->cluster_id)) == 0;
}

static bool
storage_observe_stamp(DarmokCatalogStamp *stamp)
{
	volatile bool acquired = false;

	darmok_catalog_reader_start();
	PG_TRY();
	{
		acquired = darmok_catalog_fence_try_acquire(stamp);
	}
	PG_FINALLY();
	{
		darmok_catalog_reader_finish();
	}
	PG_END_TRY();
	return acquired;
}

static bool
storage_capture(StorageState *state, StorageObservation *observation, const DarmokCatalogStamp *expected)
{
	volatile bool captured = false;

	darmok_catalog_reader_start();
	PG_TRY();
	{
		if (darmok_catalog_fence_try_acquire(&observation->stamp) &&
			storage_stamp_equal(expected, &observation->stamp))
		{
			storage_read_fixed(state, observation);
			captured = true;
		}
	}
	PG_FINALLY();
	{
		/* No descriptor cleanup here. The final capture still owns S. */
		darmok_catalog_reader_finish();
	}
	PG_END_TRY();
	return captured;
}

static StorageClass *
storage_class(StorageObservation *observation, Oid oid)
{
	StorageClass *fact = hash_search(observation->classes, &oid, HASH_FIND, NULL);

	if (fact == NULL)
		ereport(ERROR,
				(errcode(ERRCODE_DATA_CORRUPTED),
				 errmsg("native storage relation OID %u is missing", oid)));
	return fact;
}

static void
storage_validate_fact(StorageState *state, StorageObservation *observation,
					  DarmokHeapStorageFact *fact)
{
	StorageNamespace *schema = hash_search(observation->namespaces, &fact->schema_oid,
										  HASH_FIND, NULL);

	if (schema == NULL || !OidIsValid(fact->oid) ||
		fact->shared != IsSharedRelation(fact->oid))
		ereport(ERROR,
				(errcode(ERRCODE_DATA_CORRUPTED),
				 errmsg("native storage schema or shared relation identity is inconsistent")));
	fact->schema_name = schema->name;
	if (fact->persistence != RELPERSISTENCE_PERMANENT &&
		fact->persistence != RELPERSISTENCE_UNLOGGED &&
		fact->persistence != RELPERSISTENCE_TEMP)
		ereport(ERROR,
				(errcode(ERRCODE_DATA_CORRUPTED),
				 errmsg("invalid native storage persistence")));
	fact->file_proc_number = INVALID_PROC_NUMBER;
	if (fact->persistence == RELPERSISTENCE_TEMP)
	{
		Oid expected = fact->kind == RELKIND_TOASTVALUE ||
			(fact->kind == RELKIND_INDEX &&
			 storage_class(observation, fact->parent_oid)->fact.kind == RELKIND_TOASTVALUE)
			? state->temp_toast_namespace : state->temp_namespace;

		if (!OidIsValid(expected) || fact->schema_oid != expected || fact->shared ||
			!isTempOrTempToastNamespace(fact->schema_oid))
			ereport(ERROR,
					(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
					 errmsg("native storage does not admit foreign temporary relations")));
		fact->file_proc_number = state->proc_number;
	}
	else if (isTempOrTempToastNamespace(fact->schema_oid))
		ereport(ERROR,
				(errcode(ERRCODE_DATA_CORRUPTED),
				 errmsg("native storage persistence disagrees with the actual temporary namespace")));
	fact->file_tablespace_oid = OidIsValid(fact->tablespace_oid)
		? fact->tablespace_oid : state->database_tablespace;
	fact->file_database_oid = fact->file_tablespace_oid == GLOBALTABLESPACE_OID
		? InvalidOid : state->database_oid;
	if (fact->shared != !OidIsValid(fact->file_database_oid))
		ereport(ERROR,
				(errcode(ERRCODE_DATA_CORRUPTED),
				 errmsg("native storage tablespace disagrees with the native relation tag")));
	fact->file_number = fact->stored_file_number;
}

static DarmokHeapStorageFact *
storage_append(StorageState *state, StorageObservation *observation,
			   StorageClass *source, Oid parent, uint8 mask, Oid root_oid, bool catalog)
{
	StorageNode *node;
	DarmokHeapStorageFact *fact;
	bool found;

	node = hash_search(observation->nodes, &source->oid, HASH_ENTER, &found);
	if (OidIsValid(parent) &&
		(parent == source->oid || hash_search(observation->nodes, &parent, HASH_FIND, NULL) == NULL))
		elog(ERROR, "native storage graph has a cyclic or non-parent-first edge");
	if (!found && observation->fact_count >= DARMOK_RELATION_REQUEST_LIMIT)
		ereport(ERROR,
				(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
				 errmsg("native storage graph exceeds 4096 relations")));
	if (!found)
	{
		node->position = observation->fact_count++;
		fact = &observation->facts[node->position];
		*fact = source->fact;
		fact->parent_oid = parent;
		fact->mode_mask = 0;
		storage_validate_fact(state, observation, fact);
	}
	fact = &observation->facts[node->position];
	if (fact->parent_oid != parent)
		elog(ERROR, "native storage shared node has contradictory parent identity");
	fact->mode_mask |= mask;
	if (observation->use_count >= STORAGE_USES)
		elog(ERROR, "native storage use provenance exceeds its combined graph bound");
	observation->uses[observation->use_count++] = (DarmokHeapStorageUse) {
		.root_oid = root_oid, .relation_oid = source->oid, .mode_mask = mask, .catalog = catalog
	};
	return fact;
}

static int
storage_index_order(const ListCell *a, const ListCell *b)
{
	const StorageIndex *left = lfirst(a);
	const StorageIndex *right = lfirst(b);

	return (left->oid > right->oid) - (left->oid < right->oid);
}

static void
storage_append_indexes(StorageState *state, StorageObservation *observation,
					   StorageClass *heap, uint8 mask, Oid root_oid, bool catalog)
{
	ListCell *cell;
	bool valid = false;

	list_sort(heap->indexes, storage_index_order);
	foreach(cell, heap->indexes)
	{
		StorageIndex *index = lfirst(cell);
		StorageClass *source = storage_class(observation, index->oid);
		DarmokHeapStorageFact *fact;

		if (!index->live || index->heap_oid != heap->oid ||
			source->fact.kind != RELKIND_INDEX || source->fact.is_partition ||
			OidIsValid(source->fact.toast_oid) ||
			source->fact.schema_oid != heap->fact.schema_oid ||
			source->fact.persistence != heap->fact.persistence ||
			source->fact.shared != heap->fact.shared)
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("inconsistent native storage index edge")));
		if (source->fact.access_method_oid != BTREE_AM_OID)
			ereport(ERROR,
					(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
					 errmsg("native storage admits only builtin btree indexes")));
		fact = storage_append(state, observation, source, heap->oid, mask, root_oid, catalog);
		fact->index_live = index->live;
		fact->index_ready = index->ready;
		fact->index_valid = index->valid;
		fact->index_check_xmin = index->check_xmin;
		valid |= index->valid;
	}
	if (heap->fact.kind == RELKIND_TOASTVALUE && !valid)
		ereport(ERROR,
				(errcode(ERRCODE_DATA_CORRUPTED),
				 errmsg("native storage TOAST heap has no live valid index")));
}

static int
storage_oid_order(const void *a, const void *b)
{
	Oid left = *(const Oid *) a;
	Oid right = *(const Oid *) b;

	return (left > right) - (left < right);
}

static int
storage_attribute_order(const void *a, const void *b)
{
	const DarmokHeapAttributeFact *left = a;
	const DarmokHeapAttributeFact *right = b;

	if (left->relation_oid != right->relation_oid)
		return (left->relation_oid > right->relation_oid) -
			(left->relation_oid < right->relation_oid);
	return (left->number > right->number) - (left->number < right->number);
}

static int
storage_type_order(const void *a, const void *b)
{
	const DarmokHeapTypeFact *left = a;
	const DarmokHeapTypeFact *right = b;

	return (left->oid > right->oid) - (left->oid < right->oid);
}

static int
storage_payload_order(const void *a, const void *b)
{
	const StoragePayloadSource *left = a;
	const StoragePayloadSource *right = b;

	if (left->catalog_oid != right->catalog_oid)
		return (left->catalog_oid > right->catalog_oid) - (left->catalog_oid < right->catalog_oid);
	if (left->row_oid != right->row_oid)
		return (left->row_oid > right->row_oid) - (left->row_oid < right->row_oid);
	return (left->field_number > right->field_number) - (left->field_number < right->field_number);
}

static void
storage_validate_profiles(StorageObservation *observation)
{
	static const Oid chunk_types[3] = {OIDOID, INT4OID, BYTEAOID};
	static const int16 chunk_lengths[3] = {sizeof(Oid), sizeof(int32), -1};
	static const char *const chunk_names[3] = {"chunk_id", "chunk_seq", "chunk_data"};

	if (storage_class(observation, AttrDefaultRelationId)->fact.toast_oid != payload_toast_oids[0] ||
		storage_class(observation, TypeRelationId)->fact.toast_oid != payload_toast_oids[1])
		ereport(ERROR,
				(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
				 errmsg("native catalog payloads require their compiled bootstrap TOAST targets")));
	if (hash_get_num_entries(observation->profile_attributes) != 3 * STORAGE_TOAST_HEAPS)
		elog(ERROR, "native catalog TOAST profile has missing or extra positive attributes");
	for (int i = 0; i < STORAGE_TOAST_HEAPS + STORAGE_CRITICAL_INDEXES; i++)
	{
		bool toast = i < STORAGE_TOAST_HEAPS;
		Oid oid = toast ? payload_toast_oids[i] : critical_index_oids[i - STORAGE_TOAST_HEAPS];
		StorageClass *source = storage_class(observation, oid);
		DarmokHeapStorageFact *fact = &source->fact;

		if (!IsCatalogRelationOid(oid) || fact->kind != (toast ? RELKIND_TOASTVALUE : RELKIND_INDEX) ||
			fact->access_method_oid != (toast ? HEAP_TABLE_AM_OID : BTREE_AM_OID) ||
			fact->schema_oid != (toast ? PG_TOAST_NAMESPACE : PG_CATALOG_NAMESPACE) ||
			fact->persistence != RELPERSISTENCE_PERMANENT || fact->shared ||
			fact->is_partition || OidIsValid(fact->toast_oid) || OidIsValid(source->rewrite_oid) ||
			source->checks != 0 || source->rules || source->triggers || source->row_security ||
			!source->options_null || fact->declared_attribute_count !=
			(toast ? 3 : (oid == ClassOidIndexId ? 1 : 2)))
			ereport(ERROR,
					(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
					 errmsg("native catalog descriptor is outside its builtin bootstrap profile")));
		if (!toast)
		{
			StorageIndex *index = hash_search(observation->indexes, &oid, HASH_FIND, NULL);
			Oid parent = oid == ClassOidIndexId ? RelationRelationId : AttributeRelationId;

			if (index == NULL || index->heap_oid != parent || !index->live || !index->ready ||
				!index->valid || index->check_xmin)
				elog(ERROR, "native critical catalog index has an inconsistent initialized profile");
			continue;
		}
		for (int number = 1; number <= 3; number++)
		{
			uint64 key = ((uint64) oid << 32) | number;
			StorageProfileAttribute *profile = hash_search(observation->profile_attributes, &key, HASH_FIND, NULL);
			DarmokHeapAttributeFact *attribute;

			if (profile == NULL)
				elog(ERROR, "native catalog TOAST profile has a missing compiled ordinal");
			attribute = &profile->fact;
			if (attribute->relation_oid != oid || attribute->number != number ||
				strcmp(NameStr(attribute->name), chunk_names[number - 1]) != 0 ||
				attribute->type_oid != chunk_types[number - 1] ||
				attribute->length != chunk_lengths[number - 1] ||
				attribute->by_value != (number != 3) || attribute->alignment != TYPALIGN_INT ||
				attribute->storage != TYPSTORAGE_PLAIN || attribute->compression != 0 ||
				attribute->typmod != -1 || attribute->dimensions != 0 ||
				OidIsValid(attribute->collation_oid) || attribute->dropped || attribute->has_missing ||
				attribute->has_default || attribute->generated != 0 || attribute->identity != 0 ||
				!profile->missing_null || !attribute->local || attribute->inheritance_count != 0)
				ereport(ERROR,
						(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
						 errmsg("native catalog TOAST attributes are outside their bootstrap profile")));
		}
	}
}

static void
storage_build_payloads(StorageObservation *observation)
{
	long expressions = hash_get_num_entries(observation->expression_rows);
	Size count;
	StoragePayloadSource *sources;
	HASH_SEQ_STATUS iter;
	StorageExpression *expression;
	Size position = 0;

	if (expressions < 0 || expressions > PG_INT32_MAX || observation->type_count < 0 ||
		observation->type_count > (PG_INT32_MAX - expressions) / 2)
		elog(ERROR, "native catalog payload source count exceeds native limits");
	count = (Size) observation->type_count * 2 + (Size) expressions;
	if (count > MaxAllocSize / sizeof(StoragePayloadSource) ||
		count > MaxAllocSize / sizeof(DarmokHeapCatalogPayloadFact) ||
		count > MaxAllocSize / sizeof(DarmokCatalogPayloadRequest))
		elog(ERROR, "native catalog payload source count exceeds native limits");
	observation->payload_count = count;
	sources = darmok_catalog_image_alloc(&observation->image_budget,
		count * sizeof(StoragePayloadSource), true);
	for (int i = 0; i < observation->type_count; i++)
	{
		StorageType *type = hash_search(observation->type_rows, &observation->types[i].oid, HASH_FIND, NULL);

		if (type == NULL)
			elog(ERROR, "native catalog lost a copied type default source");
		for (int field = 0; field < 2; field++)
		{
			StoragePayloadSource *source = &sources[position++];

			source->catalog_oid = TypeRelationId;
			source->row_oid = type->oid;
			source->field_number = field == 0 ? Anum_pg_type_typdefaultbin : Anum_pg_type_typdefault;
			source->carrier = field == 0 ? &type->binary_default : &type->text_default;
		}
	}
	hash_seq_init(&iter, observation->expression_rows);
	while ((expression = hash_seq_search(&iter)) != NULL)
	{
		StoragePayloadSource *source = &sources[position++];

		source->catalog_oid = AttrDefaultRelationId;
		source->row_oid = expression->oid;
		source->relation_oid = expression->key >> 32;
		source->number = expression->key & 0xffff;
		source->field_number = Anum_pg_attrdef_adbin;
		source->carrier = &expression->carrier;
	}
	if (position != count)
		elog(ERROR, "native catalog payload source count changed during copied validation");
	if (count > 1)
		qsort(sources, count, sizeof(StoragePayloadSource), storage_payload_order);
	observation->payloads = darmok_catalog_image_alloc(&observation->image_budget,
		count * sizeof(DarmokHeapCatalogPayloadFact), true);
	observation->payload_requests = darmok_catalog_image_alloc(&observation->image_budget,
		count * sizeof(DarmokCatalogPayloadRequest), true);
	for (Size i = 0; i < count; i++)
	{
		const StoragePayloadSource *source = &sources[i];
		DarmokHeapCatalogPayloadFact *fact = &observation->payloads[i];
		Oid toast_oid = darmok_catalog_carrier_toast(source->carrier);

		if (OidIsValid(toast_oid) &&
			toast_oid != storage_class(observation, source->catalog_oid)->fact.toast_oid)
			ereport(ERROR,
					(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
					 errmsg("native catalog external pointer does not target its declared TOAST heap")));
		fact->catalog_oid = source->catalog_oid;
		fact->row_oid = source->row_oid;
		fact->relation_oid = source->relation_oid;
		fact->number = source->number;
		fact->field_number = source->field_number;
		observation->payload_requests[i].carrier = source->carrier;
		observation->payload_requests[i].image = &fact->value;
	}
	pfree(sources);
}

static void
storage_build_columns(StorageState *state, StorageObservation *observation,
					  const Oid *oids, int oid_count)
{
	long attributes = hash_get_num_entries(observation->attribute_rows);
	long types = hash_get_num_entries(observation->type_rows);
	HASH_SEQ_STATUS iter;
	StorageAttribute *attribute;
	StorageType *type;
	int position = 0;

	if (attributes < 0 || types < 0 || types > attributes ||
		observation->missing_count < 0 || observation->missing_count > attributes ||
		attributes > (long) state->count * MaxHeapAttributeNumber ||
		(Size) attributes > MaxAllocSize / sizeof(DarmokHeapAttributeFact) ||
		(Size) types > MaxAllocSize / sizeof(DarmokHeapTypeFact))
		ereport(ERROR,
				(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
				 errmsg("native storage selected column/type counts exceed native limits")));
	observation->attribute_count = (int) attributes;
	observation->type_count = (int) types;
	if (attributes > 0)
		observation->attributes = darmok_catalog_image_alloc(&observation->image_budget,
			sizeof(DarmokHeapAttributeFact) * (Size) attributes, false);
	storage_budget(observation);
	if (types > 0)
		observation->types = darmok_catalog_image_alloc(&observation->image_budget,
			sizeof(DarmokHeapTypeFact) * (Size) types, false);
	observation->root_facts = darmok_catalog_image_alloc(&observation->image_budget,
		sizeof(DarmokHeapStorageRootFact) * state->count, true);
	storage_budget(observation);
	hash_seq_init(&iter, observation->attribute_rows);
	while ((attribute = hash_seq_search(&iter)) != NULL)
		observation->attributes[position++] = attribute->fact;
	if (position != observation->attribute_count)
		elog(ERROR, "native storage positive slot count changed during copied validation");
	if (attributes > 1)
		qsort(observation->attributes, (Size) attributes, sizeof(DarmokHeapAttributeFact), storage_attribute_order);
	position = 0;
	hash_seq_init(&iter, observation->type_rows);
	while ((type = hash_seq_search(&iter)) != NULL)
	{
		StorageNamespace *schema = hash_search(observation->namespaces, &type->fact.schema_oid,
											  HASH_FIND, NULL);

		if (!type->copied || !type->fact.defined || !OidIsValid(type->oid) ||
			type->fact.oid != type->oid || schema == NULL || !OidIsValid(type->fact.schema_oid))
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("native storage live column type is missing, undefined or inconsistent")));
		type->fact.schema_name = schema->name;
		observation->types[position++] = type->fact;
	}
	if (position != observation->type_count)
		elog(ERROR, "native storage live type count changed during copied validation");
	if (types > 1)
		qsort(observation->types, (Size) types, sizeof(DarmokHeapTypeFact), storage_type_order);
	position = 0;
	for (int i = 0; i < oid_count; i++)
	{
		StorageClass *heap = storage_class(observation, oids[i]);
		StorageRoot *root = hash_search(observation->selected_roots, &oids[i], HASH_FIND, NULL);
		int count = heap->fact.declared_attribute_count;

		if (root == NULL || count < 0 || count > MaxHeapAttributeNumber)
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("native storage declared column count is inconsistent")));
		root->attribute_offset = position;
		for (int number = 1; number <= count; number++)
		{
			DarmokHeapAttributeFact *fact;

			if (position >= observation->attribute_count ||
				observation->attributes[position].relation_oid != oids[i] ||
				observation->attributes[position].number != number)
				ereport(ERROR,
						(errcode(ERRCODE_DATA_CORRUPTED),
						 errmsg("native storage positive column ordinals do not match relnatts")));
			fact = &observation->attributes[position++];
			{
				uint64 key = ((uint64) fact->relation_oid << 32) | (uint16) fact->number;
				StorageExpression *expression = hash_search(observation->expression_rows, &key, HASH_FIND, NULL);
				bool generation_valid = fact->generated == 0 || fact->generated == ATTRIBUTE_GENERATED_STORED;

#if PG_VERSION_NUM >= 180000
				generation_valid |= fact->generated == ATTRIBUTE_GENERATED_VIRTUAL;
#endif
				if (fact->has_default != (expression != NULL) || !generation_valid ||
					(fact->generated != 0 && (!fact->has_default || fact->identity != 0)) ||
					(fact->dropped && (fact->has_default || fact->has_missing ||
					 fact->generated != 0 || fact->identity != 0)))
					elog(ERROR, "native column default/generation declaration is inconsistent");
			}
			if (fact->dropped)
			{
				if (OidIsValid(fact->type_oid))
					ereport(ERROR,
							(errcode(ERRCODE_DATA_CORRUPTED),
							 errmsg("native storage dropped column has a live type link")));
			}
			else
			{
				type = hash_search(observation->type_rows, &fact->type_oid, HASH_FIND, NULL);
				if (!OidIsValid(fact->type_oid) || type == NULL || !type->copied ||
					fact->length != type->fact.length ||
					fact->by_value != type->fact.by_value ||
					fact->alignment != type->fact.alignment)
					ereport(ERROR,
							(errcode(ERRCODE_DATA_CORRUPTED),
							 errmsg("native storage live column/type physical layout is inconsistent")));
				/* Storage/compression, typmod, dimensions, collation and NOT NULL
				 * are independent column declarations, not type-layout redundancy. */
			}
		}
		if (position < observation->attribute_count &&
			observation->attributes[position].relation_oid == oids[i])
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("native storage has excess positive column ordinals")));
	}
	if (position != observation->attribute_count)
		elog(ERROR, "native storage has an unselected positive column");
	for (int i = 0; i < state->count; i++)
	{
		Oid oid = observation->roots[i].relation_oid;
		StorageClass *heap = storage_class(observation, oid);
		StorageRoot *root = hash_search(observation->selected_roots, &oid, HASH_FIND, NULL);
		DarmokHeapStorageRootFact *fact = &observation->root_facts[i];

		if (root == NULL)
			elog(ERROR, "native storage lost a selected root binding");
		fact->oid = oid;
		fact->row_type_oid = heap->fact.row_type_oid;
		fact->declared_attribute_count = heap->fact.declared_attribute_count;
		fact->attribute_offset = root->attribute_offset;
	}
	storage_budget(observation);
}

static void
storage_append_root_graph(StorageState *state, StorageObservation *observation,
						  Oid oid, uint8 mask, bool catalog)
{
	StorageClass *heap = storage_class(observation, oid);

	storage_append(state, observation, heap, InvalidOid, mask, oid, catalog);
	storage_append_indexes(state, observation, heap, mask, oid, catalog);
	if (OidIsValid(heap->fact.toast_oid))
	{
		StorageClass *toast = storage_class(observation, heap->fact.toast_oid);
		uint8 toast_mask = 1 << AccessShareLock;

		if (toast->fact.kind != RELKIND_TOASTVALUE || toast->fact.is_partition ||
			toast->fact.access_method_oid != HEAP_TABLE_AM_OID ||
			OidIsValid(toast->fact.toast_oid) ||
			toast->fact.persistence != heap->fact.persistence || toast->fact.shared != heap->fact.shared)
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("inconsistent native storage TOAST edge")));
		if (mask & (1 << RowExclusiveLock))
			toast_mask |= 1 << RowExclusiveLock;
		storage_append(state, observation, toast, heap->oid, toast_mask, oid, catalog);
		storage_append_indexes(state, observation, toast, toast_mask, oid, catalog);
	}
}

static void
storage_build_graph(StorageState *state, StorageObservation *observation)
{
	HTAB *modes = storage_hash(observation, "storage root modes", sizeof(Oid), sizeof(StorageMode));
	HASH_SEQ_STATUS iter;
	StorageIndex *index;
	Oid *oids = darmok_catalog_image_alloc(&observation->image_budget,
		sizeof(Oid) * (state->count + STORAGE_HEAPS), false);
	Oid *application_oids = darmok_catalog_image_alloc(&observation->image_budget,
		sizeof(Oid) * state->count, false);
	int oid_count = 0;
	int application_count = 0;

	observation->roots = darmok_catalog_image_alloc(&observation->image_budget,
		sizeof(DarmokRelationRequest) * state->count, true);
	observation->facts = darmok_catalog_image_alloc(&observation->image_budget,
		sizeof(DarmokHeapStorageFact) * DARMOK_RELATION_REQUEST_LIMIT, true);
	observation->references = darmok_catalog_image_alloc(&observation->image_budget,
		sizeof(DarmokRelationRequest) * DARMOK_RELATION_REQUEST_LIMIT, true);
	observation->uses = darmok_catalog_image_alloc(&observation->image_budget,
		sizeof(DarmokHeapStorageUse) * STORAGE_USES, true);
	observation->nodes = storage_hash(observation, "storage graph OIDs", sizeof(Oid), sizeof(StorageNode));
	hash_seq_init(&iter, observation->indexes);
	while ((index = hash_seq_search(&iter)) != NULL)
		if (index->live)
		{
			StorageClass *heap = storage_class(observation, index->heap_oid);

			/* Never trust the conservative relhasindex hint. */
			heap->indexes = lappend(heap->indexes, index);
		}
	for (int i = 0; i < state->count; i++)
	{
		StorageInput *input = &state->inputs[i];
		Oid oid = storage_literal_oid(observation, input, i);
		StorageClass *heap = storage_class(observation, oid);
		StorageMode *mode;
		bool found;

		if (heap->fact.kind != RELKIND_RELATION || heap->fact.is_partition ||
			heap->fact.access_method_oid != HEAP_TABLE_AM_OID)
			ereport(ERROR,
					(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
					 errmsg("native storage admits only ordinary nonpartitioned builtin heaps")));
		observation->roots[i].relation_oid = oid;
		observation->roots[i].lock_mode = input->mode;
		mode = hash_search(modes, &oid, HASH_ENTER, &found);
		if (!found)
		{
			mode->mask = 0;
			mode->catalog = false;
			oids[oid_count++] = oid;
			application_oids[application_count++] = oid;
		}
		mode->mask |= 1 << input->mode;
	}
	for (int i = 0; i < STORAGE_HEAPS; i++)
	{
		StorageClass *heap = storage_class(observation, fact_heap_oids[i]);
		StorageMode *mode;
		bool found;

		if (heap->fact.kind != RELKIND_RELATION || heap->fact.is_partition ||
			heap->fact.access_method_oid != HEAP_TABLE_AM_OID || heap->fact.shared ||
			heap->fact.persistence != RELPERSISTENCE_PERMANENT ||
			heap->fact.schema_oid != PG_CATALOG_NAMESPACE || OidIsValid(heap->rewrite_oid))
			elog(ERROR, "native fact catalog is outside its ordinary builtin storage profile");
		mode = hash_search(modes, &heap->oid, HASH_ENTER, &found);
		if (!found)
		{
			mode->mask = 0;
			oids[oid_count++] = heap->oid;
		}
		mode->catalog = true;
	}
	qsort(oids, oid_count, sizeof(Oid), storage_oid_order);
	for (int i = 0; i < oid_count; i++)
	{
		StorageMode *mode = hash_search(modes, &oids[i], HASH_FIND, NULL);

		if (mode->mask != 0)
			storage_append_root_graph(state, observation, oids[i], mode->mask, false);
		if (mode->catalog)
			storage_append_root_graph(state, observation, oids[i], 1 << AccessShareLock, true);
	}
	/* Mode union is complete before exact-reference emission: one physical
	 * increment per OID/mode, with independent per-root use provenance. */
	for (int i = 0; i < observation->fact_count; i++)
		for (LOCKMODE mode = AccessShareLock; mode <= RowExclusiveLock; mode++)
			if (observation->facts[i].mode_mask & (1 << mode))
			{
				DarmokRelationRequest *reference;

				if (observation->reference_count >= DARMOK_RELATION_REQUEST_LIMIT)
					elog(ERROR, "native storage graph exceeds 4096 exact references");
				reference = &observation->references[observation->reference_count++];
				reference->relation_oid = observation->facts[i].oid;
				reference->lock_mode = mode;
			}
	qsort(application_oids, application_count, sizeof(Oid), storage_oid_order);
	storage_build_columns(state, observation, application_oids, application_count);
	storage_validate_profiles(observation);
	storage_build_payloads(observation);
	pfree(application_oids);
	pfree(oids);
	storage_budget(observation);
}

static void
storage_resolve_maps(StorageObservation *observation)
{
	darmok_native_refresh_start();
	PG_TRY();
	{
		for (int i = 0; i < observation->fact_count; i++)
		{
			DarmokHeapStorageFact *fact = &observation->facts[i];

			if (!OidIsValid(fact->stored_file_number))
			{
				RelFileNumber number = RelationMapOidToFilenumber(fact->oid, fact->shared);

				if (number == InvalidRelFileNumber)
					ereport(ERROR,
							(errcode(ERRCODE_DATA_CORRUPTED),
							 errmsg("native storage mapped file number is missing")));
				fact->file_number = number;
			}
		}
	}
	PG_FINALLY();
	{
		darmok_native_refresh_finish();
	}
	PG_END_TRY();
}

static bool
storage_definition_equal(const DarmokHeapStorageFact *a, const DarmokHeapStorageFact *b)
{
	return a->oid == b->oid && a->schema_oid == b->schema_oid &&
		memcmp(&a->schema_name, &b->schema_name, sizeof(NameData)) == 0 &&
		memcmp(&a->name, &b->name, sizeof(NameData)) == 0 &&
		a->access_method_oid == b->access_method_oid &&
		a->toast_oid == b->toast_oid && a->parent_oid == b->parent_oid &&
		a->tablespace_oid == b->tablespace_oid &&
		a->stored_file_number == b->stored_file_number &&
		a->row_type_oid == b->row_type_oid &&
		a->declared_attribute_count == b->declared_attribute_count &&
		a->file_tablespace_oid == b->file_tablespace_oid &&
		a->file_database_oid == b->file_database_oid &&
		a->file_proc_number == b->file_proc_number &&
		a->kind == b->kind && a->persistence == b->persistence &&
		a->mode_mask == b->mode_mask && a->shared == b->shared &&
		a->is_partition == b->is_partition &&
		a->index_live == b->index_live && a->index_ready == b->index_ready &&
		a->index_valid == b->index_valid && a->index_check_xmin == b->index_check_xmin;
	/* relhasindex/relhassubclass are conservative hints, not definitions.
	 * Real mapped file numbers were resolved outside S under native refs.
	 * Native relfilenumber replacement requires exclusive relation ownership. */
}

static bool
storage_attribute_equal(const DarmokHeapAttributeFact *a, const DarmokHeapAttributeFact *b)
{
	return a->relation_oid == b->relation_oid && a->number == b->number &&
		memcmp(&a->name, &b->name, sizeof(NameData)) == 0 &&
		a->type_oid == b->type_oid && a->length == b->length &&
		a->typmod == b->typmod && a->dimensions == b->dimensions &&
		a->by_value == b->by_value && a->alignment == b->alignment &&
		a->storage == b->storage && a->compression == b->compression &&
		a->not_null_declared == b->not_null_declared &&
		a->has_default == b->has_default && a->has_missing == b->has_missing &&
		a->identity == b->identity && a->generated == b->generated &&
		a->dropped == b->dropped && a->local == b->local &&
		a->inheritance_count == b->inheritance_count && a->collation_oid == b->collation_oid;
}

static bool
storage_type_equal(const DarmokHeapTypeFact *a, const DarmokHeapTypeFact *b)
{
	return a->oid == b->oid && a->schema_oid == b->schema_oid &&
		memcmp(&a->schema_name, &b->schema_name, sizeof(NameData)) == 0 &&
		memcmp(&a->name, &b->name, sizeof(NameData)) == 0 &&
		a->length == b->length && a->by_value == b->by_value &&
		a->kind == b->kind && a->category == b->category &&
		a->preferred == b->preferred && a->defined == b->defined &&
		a->delimiter == b->delimiter && a->relation_oid == b->relation_oid &&
		a->subscript_oid == b->subscript_oid && a->element_oid == b->element_oid &&
		a->array_oid == b->array_oid && a->input_oid == b->input_oid &&
		a->output_oid == b->output_oid && a->receive_oid == b->receive_oid &&
		a->send_oid == b->send_oid && a->typmod_input_oid == b->typmod_input_oid &&
		a->typmod_output_oid == b->typmod_output_oid && a->analyze_oid == b->analyze_oid &&
		a->alignment == b->alignment && a->storage == b->storage &&
		a->not_null_declared == b->not_null_declared && a->base_type_oid == b->base_type_oid &&
		a->typmod == b->typmod && a->dimensions == b->dimensions &&
		a->collation_oid == b->collation_oid;
}

static bool
storage_carrier_equal(const DarmokCatalogCarrier *a, const DarmokCatalogCarrier *b)
{
	return a->present == b->present && a->bytes == b->bytes &&
		(a->bytes == 0 || (a->data != NULL && b->data != NULL && memcmp(a->data, b->data, a->bytes) == 0));
}

static void
storage_compare(StorageState *state, StorageObservation *a, StorageObservation *b)
{
	if (a->fact_count != b->fact_count || a->reference_count != b->reference_count ||
		a->attribute_count != b->attribute_count || a->type_count != b->type_count ||
		a->missing_count != b->missing_count || a->payload_count != b->payload_count ||
		a->use_count != b->use_count ||
		a->cost.missing_carrier_bytes != b->cost.missing_carrier_bytes ||
		a->cost.payload_carrier_bytes != b->cost.payload_carrier_bytes ||
		!storage_stamp_equal(&a->stamp, &b->stamp))
		ereport(ERROR,
				(errcode(ERRCODE_DATA_CORRUPTED),
				 errmsg("native storage graph changed without a publication identity change")));
	for (int i = 0; i < state->count; i++)
		if (a->roots[i].relation_oid != b->roots[i].relation_oid ||
			a->roots[i].lock_mode != b->roots[i].lock_mode ||
			a->root_facts[i].oid != b->root_facts[i].oid ||
			a->root_facts[i].row_type_oid != b->root_facts[i].row_type_oid ||
			a->root_facts[i].declared_attribute_count != b->root_facts[i].declared_attribute_count ||
			a->root_facts[i].attribute_offset != b->root_facts[i].attribute_offset)
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("native storage literal binding changed without publication")));
	for (int i = 0; i < a->fact_count; i++)
	{
		if (!storage_definition_equal(&a->facts[i], &b->facts[i]))
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("native storage definition changed without publication")));
		b->facts[i].file_number = a->facts[i].file_number;
	}
	for (int i = 0; i < a->reference_count; i++)
		if (a->references[i].relation_oid != b->references[i].relation_oid ||
			a->references[i].lock_mode != b->references[i].lock_mode)
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("native storage exact references changed without publication")));
	for (int i = 0; i < a->attribute_count; i++)
	{
		if (!storage_attribute_equal(&a->attributes[i], &b->attributes[i]))
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("native storage column definition changed without publication")));
		if (a->attributes[i].has_missing)
		{
			uint64 key = ((uint64) a->attributes[i].relation_oid << 32) |
				(uint16) a->attributes[i].number;
			StorageAttribute *left = hash_search(a->attribute_rows, &key, HASH_FIND, NULL);
			StorageAttribute *right = hash_search(b->attribute_rows, &key, HASH_FIND, NULL);

			if (left == NULL || right == NULL ||
				!storage_carrier_equal(&left->missing_carrier, &right->missing_carrier))
				ereport(ERROR,
						(errcode(ERRCODE_DATA_CORRUPTED),
						 errmsg("native storage missing carrier changed without publication")));
		}
	}
	for (int i = 0; i < a->type_count; i++)
		if (!storage_type_equal(&a->types[i], &b->types[i]))
			ereport(ERROR,
					(errcode(ERRCODE_DATA_CORRUPTED),
					 errmsg("native storage type definition changed without publication")));
	for (int i = 0; i < a->payload_count; i++)
	{
		DarmokHeapCatalogPayloadFact *left = &a->payloads[i];
		DarmokHeapCatalogPayloadFact *right = &b->payloads[i];

		if (left->catalog_oid != right->catalog_oid || left->row_oid != right->row_oid ||
			left->relation_oid != right->relation_oid || left->number != right->number ||
			left->field_number != right->field_number ||
			!storage_carrier_equal(a->payload_requests[i].carrier, b->payload_requests[i].carrier))
			elog(ERROR, "native catalog payload source changed without publication");
	}
	for (int i = 0; i < a->use_count; i++)
		if (a->uses[i].root_oid != b->uses[i].root_oid ||
			a->uses[i].relation_oid != b->uses[i].relation_oid ||
			a->uses[i].mode_mask != b->uses[i].mode_mask || a->uses[i].catalog != b->uses[i].catalog)
			elog(ERROR, "native catalog/application storage use changed without publication");
	for (int i = 0; i < STORAGE_TOAST_HEAPS; i++)
		for (int number = 1; number <= 3; number++)
		{
			uint64 key = ((uint64) payload_toast_oids[i] << 32) | number;
			StorageProfileAttribute *left = hash_search(a->profile_attributes, &key, HASH_FIND, NULL);
			StorageProfileAttribute *right = hash_search(b->profile_attributes, &key, HASH_FIND, NULL);

			if (left == NULL || right == NULL || left->missing_null != right->missing_null ||
				!storage_attribute_equal(&left->fact, &right->fact))
				elog(ERROR, "native catalog TOAST descriptor profile changed without publication");
		}
}

static void
storage_fetch_context(void *context)
{
	StorageState *state = context;

	storage_context_check(state);
	if (state->source.snapshot == NULL || darmok_statement_guard_owned(&state->semantic) ||
		!darmok_relation_attempt_owned(&state->physical))
		elog(ERROR, "native catalog fetch lost its source snapshot or physical boundary");
	storage_budget(&state->source);
}

static void
storage_fetch_payloads(StorageState *state)
{
	StorageObservation *observation = &state->source;
	bool selected[STORAGE_TOAST_HEAPS] = {false};
	Relation heaps[STORAGE_TOAST_HEAPS] = {NULL};
	int heap_count = 0;

	storage_fetch_context(state);
	for (int i = 0; i < observation->payload_count; i++)
	{
		Oid oid = darmok_catalog_carrier_toast(observation->payload_requests[i].carrier);

		if (!OidIsValid(oid))
			continue;
		if (!storage_toast_profile_oid(oid))
			elog(ERROR, "native catalog fetch has an unadmitted pointer target");
		for (int j = 0; j < STORAGE_TOAST_HEAPS; j++)
			selected[j] |= oid == payload_toast_oids[j];
	}
	/* B's fresh class/positive-slot/NULL-option profile was established and
	 * compared before any selected TOAST descriptor path. Native startup and
	 * continuous builtin support history remain explicit source preconditions. */
	storage_validate_profiles(observation);
	if (selected[0] || selected[1])
	{
		darmok_native_refresh_start();
		PG_TRY();
		{
			AcceptInvalidationMessages();
			if (!criticalRelcachesBuilt)
				elog(ERROR, "native critical relcache prerequisite changed before catalog payload fetch");
			for (int i = 0; i < STORAGE_CRITICAL_INDEXES; i++)
			{
				Relation index;

				observation->critical_indexes[i] = relation_open(critical_index_oids[i], NoLock);
				index = observation->critical_indexes[i];
				if (!index->rd_isnailed || !index->rd_isvalid || index->rd_indam == NULL ||
					index->rd_rel->relam != BTREE_AM_OID || index->rd_options != NULL)
					elog(ERROR, "native critical index postcheck contradicts its initialized builtin profile");
			}
			for (int i = 0; i < STORAGE_TOAST_HEAPS; i++)
				if (selected[i])
				{
					Relation heap;
					TupleDesc descriptor;

					/* No new conflicting relation tag is acquired while B is alive.
					 * Nested native catalog/index opens use already-owned exact AS. */
					observation->toast_heaps[i] = table_open(payload_toast_oids[i], NoLock);
					heap = observation->toast_heaps[i];
					descriptor = RelationGetDescr(heap);
					if (heap->rd_tableam != GetHeapamTableAmRoutine() ||
						heap->rd_rel->relkind != RELKIND_TOASTVALUE ||
						OidIsValid(heap->rd_rel->reltoastrelid) || OidIsValid(heap->rd_rel->relrewrite) ||
						heap->rd_options != NULL || descriptor->natts != 3)
						elog(ERROR, "native catalog TOAST descriptor postcheck contradicts its admitted profile");
					for (int number = 1; number <= 3; number++)
					{
						uint64 key = ((uint64) payload_toast_oids[i] << 32) | number;
						StorageProfileAttribute *profile = hash_search(observation->profile_attributes, &key, HASH_FIND, NULL);
						DarmokHeapAttributeFact actual;

						storage_copy_attribute(&actual, TupleDescAttr(descriptor, number - 1));
						if (profile == NULL || !storage_attribute_equal(&actual, &profile->fact))
							elog(ERROR, "native catalog TOAST descriptor attributes contradict their admitted profile");
					}
					heaps[heap_count++] = heap;
				}
		}
		PG_FINALLY();
		{
			darmok_native_refresh_finish();
		}
		PG_END_TRY();
	}
	storage_fetch_context(state);
	storage_normalize_missing(observation);
	darmok_catalog_payload_images(&observation->image_budget, observation->payload_requests,
		observation->payload_count, heaps, heap_count, &observation->payload_cost,
		storage_fetch_context, state);
	storage_fetch_context(state);
}

static void
storage_end_metadata(StorageState *state)
{
	darmok_catalog_reader_finish();
	if (darmok_statement_guard_owned(&state->semantic))
		darmok_statement_guard_release(&state->semantic);
	storage_close_reader(&state->initial);
	storage_close_reader(&state->source);
	storage_close_reader(&state->final);
}

static void
storage_unwind(StorageState *state)
{
	storage_end_metadata(state);
	if (darmok_relation_attempt_owned(&state->physical))
		darmok_relation_attempt_release(&state->physical);
}

static void
storage_reset(StorageState *state)
{
	MemoryContextSwitchTo(state->invocation);
	if (state->initial.context != NULL)
		MemoryContextDelete(state->initial.context);
	if (state->source.context != NULL)
		MemoryContextDelete(state->source.context);
	if (state->final.context != NULL)
		MemoryContextDelete(state->final.context);
	memset(&state->initial, 0, sizeof(state->initial));
	memset(&state->source, 0, sizeof(state->source));
	memset(&state->final, 0, sizeof(state->final));
}

static bool
storage_attempt(StorageState *state, const DarmokCatalogStamp *before, int attempt,
				DarmokHeapStorageConsumer consumer, void *consumer_state, bool retain)
{
	DarmokHeapStorageView view = {0};
	StorageObservation *a = &state->initial;
	StorageObservation *b = &state->source;
	StorageObservation *c = &state->final;

	storage_observation_init(state, a);
	storage_prepare(a);
	storage_context_check(state);
	if (!storage_capture(state, a, before))
		return false;
	storage_close_reader(a); /* No A snapshot/descriptors through physical waits. */
	storage_context_check(state);
	storage_build_graph(state, a);
	darmok_relation_attempt_acquire(&state->physical, a->references, a->reference_count);
	storage_observation_init(state, b);
	storage_prepare(b);
	storage_resolve_maps(a);
	storage_context_check(state);
	if (!storage_capture(state, b, &a->stamp))
		return false;
	storage_end_scans(b); /* Retain the registered source snapshot only. */
	storage_build_graph(state, b);
	storage_compare(state, a, b);
	storage_context_check(state);
	storage_fetch_payloads(state);
	storage_close_reader(b); /* Close horizon and increments before any later wait. */
	storage_context_check(state);
	storage_observation_init(state, c);
	storage_prepare(c);
	storage_context_check(state);
	if (!darmok_statement_guard_acquire(&state->semantic))
		return false;
	if (!storage_capture(state, c, &a->stamp))
		return false;
	/* Raw/gate have ended; only copied C facts are processed under S.
	 * No descriptor opening, external fetching or decoding occurs here. */
	storage_build_graph(state, c);
	storage_compare(state, a, c);
	storage_compare(state, b, c);
	storage_context_check(state);
	for (int i = 0; i < c->payload_count; i++)
		c->payloads[i].value = b->payloads[i].value;
	view.roots = c->roots;
	view.root_count = state->count;
	view.root_facts = c->root_facts;
	view.attributes = c->attributes;
	view.attribute_count = c->attribute_count;
	view.types = c->types;
	view.type_count = c->type_count;
	view.missing = b->missing;
	view.missing_count = b->missing_count;
	view.attribute_array_bytes = sizeof(DarmokHeapAttributeFact) * (Size) view.attribute_count;
	view.type_array_bytes = sizeof(DarmokHeapTypeFact) * (Size) view.type_count;
	view.missing_array_bytes = sizeof(DarmokHeapMissingFact) * (Size) view.missing_count;
	view.missing_image_bytes = b->missing_image_bytes;
	view.payloads = c->payloads;
	view.payload_count = c->payload_count;
	view.payload_array_bytes = sizeof(DarmokHeapCatalogPayloadFact) * (Size) c->payload_count;
	view.payload_image_bytes = b->payload_cost.image_bytes;
	view.toast_heaps = b->payload_cost.toast_heaps;
	view.toast_rows = b->payload_cost.toast_rows;
	view.selected_chunks = b->payload_cost.selected_chunks;
	view.payload_stored_bytes = b->payload_cost.stored_bytes;
	a->cost.allocated_bytes = MemoryContextMemAllocated(a->context, true);
	b->cost.allocated_bytes = MemoryContextMemAllocated(b->context, true);
	c->cost.allocated_bytes = MemoryContextMemAllocated(c->context, true);
	a->cost.requested_copy_bytes = a->image_budget.requested_bytes;
	b->cost.requested_copy_bytes = b->image_budget.requested_bytes;
	c->cost.requested_copy_bytes = c->image_budget.requested_bytes;
	view.initial_cost = a->cost;
	view.source_cost = b->cost;
	view.final_cost = c->cost;
	view.facts = c->facts;
	view.fact_count = c->fact_count;
	view.uses = c->uses;
	view.use_count = c->use_count;
	view.references = c->references;
	view.reference_count = c->reference_count;
	view.generation = c->stamp.generation;
	view.local_generation = c->stamp.local_generation;
	view.attempts = attempt + 1;
	view.physical_owned = darmok_relation_attempt_owned(&state->physical);
	view.metadata_owned = darmok_statement_guard_owned(&state->semantic);
	view.first_snapshot_set = FirstSnapshotSet;
	if (!view.physical_owned || !view.metadata_owned)
		elog(ERROR, "native storage lost its owned metadata or physical attempt");
	consumer(&view, consumer_state);
	storage_context_check(state);
	storage_end_metadata(state);
	storage_context_check(state);
	if (retain)
		darmok_relation_attempt_retain(&state->physical);
	else
		darmok_relation_attempt_release(&state->physical);
	return true;
}

static void
storage_copy_inputs(StorageState *state, const DarmokHeapStorageRoot *roots)
{
	Size bytes = 0;

	state->inputs = palloc0(sizeof(StorageInput) * state->count);
	for (int i = 0; i < state->count; i++)
	{
		const char *names[2] = {roots[i].schema, roots[i].name};
		Size lengths[2] = {roots[i].schema_bytes, roots[i].name_bytes};
		NameData *parts[2] = {&state->inputs[i].schema, &state->inputs[i].name};

		if (roots[i].lock_mode != AccessShareLock &&
			roots[i].lock_mode != RowShareLock && roots[i].lock_mode != RowExclusiveLock)
			ereport(ERROR,
					(errcode(ERRCODE_INVALID_PARAMETER_VALUE),
					 errmsg("invalid native storage root mode")));
		state->inputs[i].mode = roots[i].lock_mode;
		for (int j = 0; j < 2; j++)
		{
			Size length;

			if (names[j] == NULL)
				ereport(ERROR,
						(errcode(ERRCODE_INVALID_PARAMETER_VALUE),
						 errmsg("native storage requires literal counted names")));
			length = lengths[j];
			if (length == 0 || length >= NAMEDATALEN || memchr(names[j], '\0', length) != NULL)
				ereport(ERROR,
						(errcode(ERRCODE_INVALID_PARAMETER_VALUE),
						 errmsg("native storage name must contain 1 to %d UTF8 bytes", NAMEDATALEN - 1)));
			pg_verify_mbstr(PG_UTF8, names[j], length, false);
			bytes += length + 1;
			if (bytes > STORAGE_NAME_BYTES)
				ereport(ERROR,
						(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
						 errmsg("native storage names exceed 1 MiB")));
			memcpy(NameStr(*parts[j]), names[j], length);
		}
	}
}

void
darmok_heap_storage_metadata(const DarmokHeapStorageRoot *roots, int count,
							 DarmokHeapStorageConsumer consumer, void *consumer_state, bool retain)
{
	StorageState *state;
	volatile bool completed = false;

	if (storage_active)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("native heap storage cannot reenter its invocation")));
	darmok_native_invocation_check();
	state = palloc0(sizeof(StorageState));
	state->parent = CurrentMemoryContext;
	state->owner = CurrentResourceOwner;
	state->transaction_owner = CurTransactionResourceOwner;
	state->subid = GetCurrentSubTransactionId();
	state->database_oid = MyDatabaseId;
	state->database_tablespace = MyDatabaseTableSpace;
	state->proc_number = MyProcNumber;
	state->first_snapshot_set = FirstSnapshotSet;
	state->count = count;
	GetTempNamespaceState(&state->temp_namespace, &state->temp_toast_namespace);
	storage_active = true;
	PG_TRY();
	{
		if (roots == NULL || consumer == NULL || count <= 0 || count > DARMOK_RELATION_REQUEST_LIMIT)
			ereport(ERROR,
					(errcode(ERRCODE_INVALID_PARAMETER_VALUE),
					 errmsg("native storage requires 1 to 4096 roots and a pure C consumer")));
		if (GetDatabaseEncoding() != PG_UTF8)
			ereport(ERROR,
					(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
					 errmsg("native storage requires a UTF8 database")));
		state->invocation = AllocSetContextCreate(state->parent, "darmok heap storage invocation",
												ALLOCSET_DEFAULT_SIZES);
		MemoryContextSwitchTo(state->invocation);
		storage_copy_inputs(state, roots);
		storage_context_check(state);
		for (int attempt = 0; attempt < STORAGE_ATTEMPTS; attempt++)
		{
			DarmokCatalogStamp before;

			if (storage_observe_stamp(&before) &&
				storage_attempt(state, &before, attempt, consumer, consumer_state, retain))
			{
				completed = true;
				break;
			}
			/* False is only a completed pre-effect intent/generation result.
			 * Never CV-sleep with this or earlier borrowed native counts. */
			storage_unwind(state);
			storage_context_check(state);
			storage_reset(state);
		}
		if (!completed)
			ereport(ERROR,
					(errcode(ERRCODE_T_R_SERIALIZATION_FAILURE),
					 errmsg("native storage admission changed through 16 attempts")));
	}
	PG_FINALLY();
	{
		PG_TRY(_cleanup);
		{
			storage_unwind(state);
		}
		PG_FINALLY(_cleanup);
		{
			if (!completed)
				darmok_native_invocation_require_abort(state->subid);
			storage_active = false;
			MemoryContextSwitchTo(state->parent);
			if (state->invocation != NULL)
				MemoryContextDelete(state->invocation);
			pfree(state);
		}
		PG_END_TRY(_cleanup);
	}
	PG_END_TRY();
}
