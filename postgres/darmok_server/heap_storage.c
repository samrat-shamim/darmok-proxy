/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#include "postgres.h"

#include "access/heapam.h"
#include "access/table.h"
#include "access/tableam.h"
#include "catalog/catalog.h"
#include "catalog/namespace.h"
#include "catalog/pg_am_d.h"
#include "catalog/pg_class.h"
#include "catalog/pg_index.h"
#include "catalog/pg_namespace.h"
#include "catalog/pg_tablespace_d.h"
#include "mb/pg_wchar.h"
#include "miscadmin.h"
#include "storage/proc.h"
#include "utils/hsearch.h"
#include "utils/inval.h"
#include "utils/memutils.h"
#include "utils/relmapper.h"
#include "utils/resowner.h"
#include "utils/snapmgr.h"

#include "catalog_read.h"
#include "heap_storage.h"
#include "statement_guard.h"

#define STORAGE_ATTEMPTS 16
#define STORAGE_NAME_BYTES ((Size) 1024 * 1024)
#define STORAGE_PHASE_BYTES ((Size) 64 * 1024 * 1024)
#define STORAGE_HEAPS 3

static const Oid fact_heap_oids[STORAGE_HEAPS] = {
	NamespaceRelationId, RelationRelationId, IndexRelationId
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
} StorageMode;

typedef struct StorageNode
{
	Oid oid;
	int position;
} StorageNode;

typedef struct StorageObservation
{
	MemoryContext context;
	Relation heaps[STORAGE_HEAPS];
	TableScanDesc scans[STORAGE_HEAPS];
	Snapshot snapshot;
	HTAB *namespaces;
	HTAB *namespace_names;
	HTAB *classes;
	HTAB *relation_names;
	HTAB *indexes;
	DarmokCatalogStamp stamp;
	DarmokRelationRequest *roots;
	DarmokHeapStorageFact *facts;
	DarmokRelationRequest *references;
	HTAB *nodes;
	int fact_count;
	int reference_count;
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
		for (int i = 0; i < STORAGE_HEAPS; i++)
		{
			observation->heaps[i] = table_open(fact_heap_oids[i], AccessShareLock);
			if (observation->heaps[i]->rd_tableam != GetHeapamTableAmRoutine())
				ereport(ERROR,
						(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
						 errmsg("native storage requires builtin heap catalogs")));
		}
	}
	PG_FINALLY();
	{
		darmok_native_refresh_finish();
	}
	PG_END_TRY();
}

static void
storage_read_fixed(StorageObservation *observation)
{
	HeapTuple tuple;
	uint64 rows = 0;

	/* Catalog snapshots never select the first transaction data snapshot.
	 * Descriptor preparation has ended: only fixed builtin heap fields here.
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
		fact->fact.shared = row->relisshared;
		fact->fact.is_partition = row->relispartition;
		fact->fact.has_indexes = row->relhasindex;
		fact->fact.has_subclasses = row->relhassubclass;
		if (++rows % 1024 == 0)
			storage_budget(observation);
	}
	while ((tuple = heap_getnext(observation->scans[2], ForwardScanDirection)) != NULL)
	{
		Form_pg_index row = (Form_pg_index) GETSTRUCT(tuple);
		StorageIndex *fact;
		bool found;

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
	storage_budget(observation);
}

static void
storage_close_reader(StorageObservation *observation)
{
	for (int i = 0; i < STORAGE_HEAPS; i++)
		if (observation->scans[i] != NULL)
		{
			heap_endscan(observation->scans[i]);
			observation->scans[i] = NULL;
		}
	if (observation->snapshot != NULL)
	{
		UnregisterSnapshot(observation->snapshot);
		observation->snapshot = NULL;
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
storage_capture(StorageObservation *observation, const DarmokCatalogStamp *expected)
{
	volatile bool captured = false;

	darmok_catalog_reader_start();
	PG_TRY();
	{
		if (darmok_catalog_fence_try_acquire(&observation->stamp) &&
			storage_stamp_equal(expected, &observation->stamp))
		{
			storage_read_fixed(observation);
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
			   StorageClass *source, Oid parent, uint8 mask)
{
	StorageNode *node;
	DarmokHeapStorageFact *fact;
	bool found;

	node = hash_search(observation->nodes, &source->oid, HASH_ENTER, &found);
	if (found)
		ereport(ERROR,
				(errcode(ERRCODE_DATA_CORRUPTED),
				 errmsg("duplicate or cyclic native storage edge")));
	if (observation->fact_count >= DARMOK_RELATION_REQUEST_LIMIT)
		ereport(ERROR,
				(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
				 errmsg("native storage graph exceeds 4096 relations")));
	node->position = observation->fact_count++;
	fact = &observation->facts[node->position];
	*fact = source->fact;
	fact->parent_oid = parent;
	fact->mode_mask = mask;
	storage_validate_fact(state, observation, fact);
	for (LOCKMODE mode = AccessShareLock; mode <= RowExclusiveLock; mode++)
		if (mask & (1 << mode))
		{
			DarmokRelationRequest *request;

			if (observation->reference_count >= DARMOK_RELATION_REQUEST_LIMIT)
				ereport(ERROR,
						(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
						 errmsg("native storage graph exceeds 4096 exact references")));
			request = &observation->references[observation->reference_count++];
			request->relation_oid = fact->oid;
			request->lock_mode = mode;
		}
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
					   StorageClass *heap, uint8 mask)
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
		fact = storage_append(state, observation, source, heap->oid, mask);
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

static void
storage_build_graph(StorageState *state, StorageObservation *observation)
{
	HTAB *modes = storage_hash(observation, "storage root modes", sizeof(Oid), sizeof(StorageMode));
	HASH_SEQ_STATUS iter;
	StorageIndex *index;
	Oid *oids = palloc(sizeof(Oid) * state->count);
	int oid_count = 0;

	observation->roots = palloc0(sizeof(DarmokRelationRequest) * state->count);
	observation->facts = palloc0(sizeof(DarmokHeapStorageFact) * DARMOK_RELATION_REQUEST_LIMIT);
	observation->references = palloc0(sizeof(DarmokRelationRequest) * DARMOK_RELATION_REQUEST_LIMIT);
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
		StorageNamespaceName *schema = hash_search(observation->namespace_names,
												 &input->schema, HASH_FIND, NULL);
		StorageNameKey key = {0};
		StorageNamedRelation *named;
		StorageClass *heap;
		StorageMode *mode;
		bool found;

		if (schema == NULL)
			ereport(ERROR,
					(errcode(ERRCODE_UNDEFINED_TABLE),
					 errmsg("native storage schema at root %d does not exist", i)));
		key.schema_oid = schema->oid;
		key.name = input->name;
		named = hash_search(observation->relation_names, &key, HASH_FIND, NULL);
		if (named == NULL)
			ereport(ERROR,
					(errcode(ERRCODE_UNDEFINED_TABLE),
					 errmsg("native storage relation at root %d does not exist", i)));
		heap = storage_class(observation, named->oid);
		if (heap->fact.kind != RELKIND_RELATION || heap->fact.is_partition ||
			heap->fact.access_method_oid != HEAP_TABLE_AM_OID)
			ereport(ERROR,
					(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
					 errmsg("native storage admits only ordinary nonpartitioned builtin heaps")));
		observation->roots[i].relation_oid = named->oid;
		observation->roots[i].lock_mode = input->mode;
		mode = hash_search(modes, &named->oid, HASH_ENTER, &found);
		if (!found)
		{
			mode->mask = 0;
			oids[oid_count++] = named->oid;
		}
		mode->mask |= 1 << input->mode;
	}
	qsort(oids, oid_count, sizeof(Oid), storage_oid_order);
	for (int i = 0; i < oid_count; i++)
	{
		StorageClass *heap = storage_class(observation, oids[i]);
		StorageMode *mode = hash_search(modes, &oids[i], HASH_FIND, NULL);

		storage_append(state, observation, heap, InvalidOid, mode->mask);
		storage_append_indexes(state, observation, heap, mode->mask);
		if (OidIsValid(heap->fact.toast_oid))
		{
			StorageClass *toast = storage_class(observation, heap->fact.toast_oid);
			uint8 mask = 1 << AccessShareLock;

			if (toast->fact.kind != RELKIND_TOASTVALUE || toast->fact.is_partition ||
				toast->fact.access_method_oid != HEAP_TABLE_AM_OID ||
				OidIsValid(toast->fact.toast_oid) ||
				toast->fact.persistence != heap->fact.persistence ||
				toast->fact.shared != heap->fact.shared)
				ereport(ERROR,
						(errcode(ERRCODE_DATA_CORRUPTED),
						 errmsg("inconsistent native storage TOAST edge")));
			if (mode->mask & (1 << RowExclusiveLock))
				mask |= 1 << RowExclusiveLock;
			storage_append(state, observation, toast, heap->oid, mask);
			storage_append_indexes(state, observation, toast, mask);
		}
	}
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

static void
storage_compare(StorageState *state)
{
	StorageObservation *a = &state->initial;
	StorageObservation *b = &state->final;

	if (a->fact_count != b->fact_count || a->reference_count != b->reference_count)
		ereport(ERROR,
				(errcode(ERRCODE_DATA_CORRUPTED),
				 errmsg("native storage graph changed without a publication identity change")));
	for (int i = 0; i < state->count; i++)
		if (a->roots[i].relation_oid != b->roots[i].relation_oid ||
			a->roots[i].lock_mode != b->roots[i].lock_mode)
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
}

static void
storage_end_metadata(StorageState *state)
{
	darmok_catalog_reader_finish();
	if (darmok_statement_guard_owned(&state->semantic))
		darmok_statement_guard_release(&state->semantic);
	storage_close_reader(&state->initial);
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
	if (state->final.context != NULL)
		MemoryContextDelete(state->final.context);
	memset(&state->initial, 0, sizeof(state->initial));
	memset(&state->final, 0, sizeof(state->final));
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

			if (storage_observe_stamp(&before))
			{
				storage_observation_init(state, &state->initial);
				storage_prepare(&state->initial);
				storage_context_check(state);
				if (storage_capture(&state->initial, &before))
				{
					storage_close_reader(&state->initial);
					storage_context_check(state);
					storage_build_graph(state, &state->initial);
					darmok_relation_attempt_acquire(&state->physical, state->initial.references,
												   state->initial.reference_count);
					storage_observation_init(state, &state->final);
					storage_prepare(&state->final);
					storage_resolve_maps(&state->initial);
					storage_context_check(state);
					if (darmok_statement_guard_acquire(&state->semantic))
					{
						if (storage_capture(&state->final, &state->initial.stamp))
						{
							DarmokHeapStorageView view;

							storage_build_graph(state, &state->final);
							storage_compare(state);
							storage_context_check(state);
							view.roots = state->final.roots;
							view.root_count = state->count;
							view.facts = state->final.facts;
							view.fact_count = state->final.fact_count;
							view.references = state->final.references;
							view.reference_count = state->final.reference_count;
							view.generation = state->final.stamp.generation;
							view.local_generation = state->final.stamp.local_generation;
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
							completed = true;
							break;
						}
					}
				}
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
		PG_TRY();
		{
			storage_unwind(state);
		}
		PG_FINALLY();
		{
			if (!completed)
				darmok_native_invocation_require_abort(state->subid);
			storage_active = false;
			MemoryContextSwitchTo(state->parent);
			if (state->invocation != NULL)
				MemoryContextDelete(state->invocation);
			pfree(state);
		}
		PG_END_TRY();
	}
	PG_END_TRY();
}
