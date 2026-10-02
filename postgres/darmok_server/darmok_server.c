/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#include "postgres.h"

#include "access/xact.h"
#include "access/twophase.h"
#include "access/xlog.h"
#include "catalog/objectaccess.h"
#include "catalog/pg_extension_d.h"
#include "funcapi.h"
#include "miscadmin.h"
#include "nodes/parsenodes.h"
#include "port/atomics.h"
#include "storage/ipc.h"
#include "storage/lock.h"
#include "storage/lwlock.h"
#include "storage/shmem.h"
#include "tcop/utility.h"
#include "utils/inval.h"
#include "utils/memutils.h"
#include "utils/resowner.h"
#include "utils/snapmgr.h"

#if PG_VERSION_NUM < 170000 || PG_VERSION_NUM >= 190000
#error "darmok_server requires PostgreSQL 17 or 18"
#endif

PG_MODULE_MAGIC;

PGDLLEXPORT void _PG_init(void);
PG_FUNCTION_INFO_V1(darmok_begin_catalog_lease);
PG_FUNCTION_INFO_V1(darmok_check_catalog_lease);
PG_FUNCTION_INFO_V1(darmok_end_catalog_lease);

/* OID zero cannot name an extension. The default lock method is deliberately
 * distinct from user advisory locks. Database zero makes this cluster-wide. */
#define DARMOK_LOCK_SUBID 0x444d
#define DARMOK_CLUSTER_ID_BYTES 16

typedef struct DarmokShared
{
	unsigned char cluster_id[DARMOK_CLUSTER_ID_BYTES];
	pg_atomic_uint64 generation;
	pg_atomic_uint64 next_backend_id;
} DarmokShared;

typedef struct DarmokLease
{
	bool active;
	uint64 id;
	uint64 generation;
	uint64 local_generation;
	SubTransactionId subid;
	ResourceOwner owner;
} DarmokLease;

static DarmokShared *shared = NULL;
static uint64 backend_id = 0;
static uint64 next_lease_id = 0;
static uint64 local_generation = 1;
static bool broken = false;
static DarmokLease lease = {0};
static bool writer_transaction_lock = false;
static SubTransactionId writer_subid = InvalidSubTransactionId;
static ResourceOwner writer_owner = NULL;
static int writer_depth = 0;
static bool preparing_transaction = false;
static SubTransactionId last_metadata_subid = InvalidSubTransactionId;

static shmem_request_hook_type previous_shmem_request = NULL;
static shmem_startup_hook_type previous_shmem_startup = NULL;
static ProcessUtility_hook_type previous_utility = NULL;
static object_access_hook_type previous_object_access = NULL;

static LOCKTAG
catalog_tag(void)
{
	LOCKTAG tag;

	SET_LOCKTAG_OBJECT(tag, InvalidOid, ExtensionRelationId, InvalidOid,
					   DARMOK_LOCK_SUBID);
	return tag;
}

static void
require_ready(void)
{
	if (shared == NULL || broken)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("darmok_server catalog lease state is unavailable")));
	if (RecoveryInProgress())
		ereport(ERROR,
				(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
				 errmsg("darmok_server catalog leases require a primary server")));
}

/* A counter is never allowed to wrap and reuse a cache identity. This helper
 * runs before effects, not from a post-commit callback. */
static uint64
advance_shared(pg_atomic_uint64 *counter)
{
	uint64 value = pg_atomic_read_u64(counter);

	for (;;)
	{
		if (value >= PG_INT64_MAX)
			ereport(ERROR,
					(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
					 errmsg("darmok_server catalog identity counter exhausted")));
		if (pg_atomic_compare_exchange_u64(counter, &value, value + 1))
			return value + 1;
	}
}

/* Transaction callbacks must not throw after a transaction has committed. */
static void
advance_local(void)
{
	if (local_generation >= PG_INT64_MAX)
		broken = true;
	else
		local_generation++;
}

static void
transaction_lock(LOCKMODE mode, bool acquire, ResourceOwner owner)
{
	LOCKTAG tag = catalog_tag();
	ResourceOwner saved = CurrentResourceOwner;

	/* SQL-function portals have their own resource owners. Put a lease on the
	 * current transaction owner so a later command can release the same lock. */
	CurrentResourceOwner = owner;
	PG_TRY();
	{
		if (acquire)
			(void) LockAcquire(&tag, mode, false, false);
		else if (!LockRelease(&tag, mode, false))
		{
			broken = true;
			ereport(ERROR,
					(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
					 errmsg("darmok_server lost its catalog transaction lock")));
		}
	}
	PG_FINALLY();
	{
		CurrentResourceOwner = saved;
	}
	PG_END_TRY();
}

static void
take_writer_transaction_lock(void)
{
	if (!writer_transaction_lock)
	{
		transaction_lock(ExclusiveLock, true, CurTransactionResourceOwner);
		writer_transaction_lock = true;
		writer_subid = GetCurrentSubTransactionId();
		writer_owner = CurTransactionResourceOwner;
	}
}

static void
before_catalog_change(void)
{
	require_ready();
	if (lease.active)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("end the catalog lease before changing metadata")));
	last_metadata_subid = GetCurrentSubTransactionId();
	/* PREPARE does not publish catalog changes. Its later completion is a
	 * separately fenced utility. Never transfer our synthetic lock to 2PC. */
	if (preparing_transaction)
	{
		advance_local();
		require_ready();
		return;
	}
	take_writer_transaction_lock();
	(void) advance_shared(&shared->generation);
	advance_local();
	require_ready();
}

static void
request_shared_memory(void)
{
	if (previous_shmem_request)
		previous_shmem_request();
	RequestAddinShmemSpace(sizeof(DarmokShared));
}

static void
start_shared_memory(void)
{
	bool found;

	if (previous_shmem_startup)
		previous_shmem_startup();
	LWLockAcquire(AddinShmemInitLock, LW_EXCLUSIVE);
	shared = ShmemInitStruct("darmok_server catalog state 1", sizeof(DarmokShared),
							 &found);
	if (!found)
	{
		if (!pg_strong_random(shared->cluster_id, DARMOK_CLUSTER_ID_BYTES))
			elog(FATAL, "could not create darmok_server cluster incarnation");
		pg_atomic_init_u64(&shared->generation, 1);
		pg_atomic_init_u64(&shared->next_backend_id, 0);
	}
	LWLockRelease(AddinShmemInitLock);
}

static void
transaction_event(XactEvent event, void *arg)
{
	(void) arg;
	switch (event)
	{
		case XACT_EVENT_PRE_COMMIT:
			/* A concurrently built index has several transactions. Fence every
			 * publication, not the waits between phases: holding a session fence
			 * there would deadlock readers with WaitForOlderSnapshots(). */
			if (writer_depth > 0)
				before_catalog_change();
			break;
		case XACT_EVENT_PRE_PREPARE:
			if (lease.active)
				ereport(ERROR,
						(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
						 errmsg("end the catalog lease before preparing a transaction")));
			preparing_transaction = true;
			if (writer_transaction_lock)
			{
				transaction_lock(ExclusiveLock, false, writer_owner);
				writer_transaction_lock = false;
				writer_owner = NULL;
			}
			break;
		case XACT_EVENT_COMMIT:
		case XACT_EVENT_ABORT:
		case XACT_EVENT_PREPARE:
		case XACT_EVENT_PARALLEL_COMMIT:
		case XACT_EVENT_PARALLEL_ABORT:
			/* Committed facts can stay cached. Aborted/prepared private facts
			 * cannot; metadata-free transaction boundaries need no cache miss. */
			if ((event == XACT_EVENT_ABORT || event == XACT_EVENT_PREPARE ||
				 event == XACT_EVENT_PARALLEL_ABORT) &&
				last_metadata_subid != InvalidSubTransactionId)
				advance_local();
			lease.active = false;
			lease.owner = NULL;
			writer_transaction_lock = false;
			writer_owner = NULL;
			writer_subid = InvalidSubTransactionId;
			preparing_transaction = false;
			last_metadata_subid = InvalidSubTransactionId;
			break;
		default:
			break;
	}
}

static void
subtransaction_event(SubXactEvent event, SubTransactionId subid,
					 SubTransactionId parent, void *arg)
{
	(void) arg;
	if (event == SUBXACT_EVENT_COMMIT_SUB)
	{
		if (last_metadata_subid == subid)
			last_metadata_subid = parent;
		if (lease.active && lease.subid == subid)
		{
			lease.subid = parent;
			lease.owner = ResourceOwnerGetParent(lease.owner);
		}
		if (writer_transaction_lock && writer_subid == subid)
		{
			writer_subid = parent;
			writer_owner = ResourceOwnerGetParent(writer_owner);
		}
	}
	else if (event == SUBXACT_EVENT_ABORT_SUB)
	{
		if (last_metadata_subid == subid)
		{
			last_metadata_subid = parent;
			advance_local();
		}
		if (lease.active && lease.subid == subid)
		{
			lease.active = false;
			lease.owner = NULL;
		}
		if (writer_transaction_lock && writer_subid == subid)
		{
			writer_transaction_lock = false;
			writer_owner = NULL;
		}
	}
}

static bool
catalog_utility(Node *node)
{
	switch (nodeTag(node))
	{
		case T_TransactionStmt:
		{
			TransactionStmt *stmt = (TransactionStmt *) node;

			return stmt->kind == TRANS_STMT_COMMIT_PREPARED ||
				stmt->kind == TRANS_STMT_ROLLBACK_PREPARED;
		}
		case T_VariableSetStmt:
		case T_VariableShowStmt:
		case T_ConstraintsSetStmt:
		case T_FetchStmt:
		case T_ClosePortalStmt:
		case T_PrepareStmt:
		case T_ExecuteStmt:
		case T_DeallocateStmt:
		case T_ExplainStmt:
		case T_LockStmt:
		case T_ListenStmt:
		case T_NotifyStmt:
		case T_UnlistenStmt:
		case T_CopyStmt:
		case T_DiscardStmt:
			return false;
		default:
			/* This is a conservative lock policy, not SQL semantic admission.
			 * New/extension utility variants receive the stronger fence. */
			return true;
	}
}

static void
process_utility(PlannedStmt *pstmt, const char *query, bool read_only_tree,
				ProcessUtilityContext context, ParamListInfo params,
				QueryEnvironment *query_env, DestReceiver *dest,
				QueryCompletion *completion)
{
	bool writer = catalog_utility(pstmt->utilityStmt);
	bool preparing = IsA(pstmt->utilityStmt, TransactionStmt) &&
		((TransactionStmt *) pstmt->utilityStmt)->kind == TRANS_STMT_PREPARE;
	if (preparing && lease.active)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("end the catalog lease before preparing a transaction")));

	if (writer)
	{
		before_catalog_change();
		writer_depth++;
	}
	PG_TRY();
	{
		if (previous_utility)
			previous_utility(pstmt, query, read_only_tree, context, params,
							 query_env, dest, completion);
		else
			standard_ProcessUtility(pstmt, query, read_only_tree, context, params,
								query_env, dest, completion);
		if (writer)
		{
			/* The final transaction keeps its fence through native invalidation
			 * publication and transaction-lock cleanup. */
			if (IsTransactionState())
				take_writer_transaction_lock();
		}
	}
	PG_FINALLY();
	{
		if (writer)
			writer_depth--;
	}
	PG_END_TRY();
	if (writer)
		require_ready();
}

static void
object_access(ObjectAccessType access, Oid class_id, Oid object_id,
			  int sub_id, void *arg)
{
	if ((access == OAT_POST_CREATE || access == OAT_POST_ALTER ||
		 access == OAT_DROP) && writer_depth == 0)
		before_catalog_change();
	if (previous_object_access)
		previous_object_access(access, class_id, object_id, sub_id, arg);
}

void
_PG_init(void)
{
	if (!process_shared_preload_libraries_in_progress)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("darmok_server must be in shared_preload_libraries at server startup")));
	previous_shmem_request = shmem_request_hook;
	shmem_request_hook = request_shared_memory;
	previous_shmem_startup = shmem_startup_hook;
	shmem_startup_hook = start_shared_memory;
	previous_utility = ProcessUtility_hook;
	ProcessUtility_hook = process_utility;
	previous_object_access = object_access_hook;
	object_access_hook = object_access;
	RegisterXactCallback(transaction_event, NULL);
	RegisterSubXactCallback(subtransaction_event, NULL);
}

static uint64
require_lease(FunctionCallInfo fcinfo)
{
	LOCKTAG tag = catalog_tag();

	require_ready();
	if (PG_ARGISNULL(0) || PG_ARGISNULL(1) || !lease.active ||
		PG_GETARG_INT64(0) <= 0 || PG_GETARG_INT64(1) <= 0 ||
		(uint64) PG_GETARG_INT64(0) != lease.id ||
		(uint64) PG_GETARG_INT64(1) != backend_id ||
		!LockHeldByMe(&tag, ShareLock, false))
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("catalog lease is absent or does not match this backend")));
	return lease.id;
}

Datum
darmok_begin_catalog_lease(PG_FUNCTION_ARGS)
{
	TupleDesc desc;
	Datum values[6];
	bool nulls[6] = {false, false, false, false, false, false};
	bytea *cluster_id;

	require_ready();
	/* A prepared transaction can retain a relation lock indefinitely. Waiting
	 * for it while owning our read fence would also block its fenced catalog
	 * completion. This release requires native 2PC disabled, not a weaker
	 * generation check or an unbounded wait through that dependency. */
	if (max_prepared_xacts != 0)
		ereport(ERROR,
				(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
				 errmsg("catalog leases require max_prepared_transactions=0")));
	if (!IsTransactionBlock())
		ereport(ERROR,
				(errcode(ERRCODE_NO_ACTIVE_SQL_TRANSACTION),
				 errmsg("a catalog lease requires an explicit transaction")));
	if (lease.active)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("this backend already owns a catalog lease")));
	if (writer_depth > 0 || preparing_transaction)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("a catalog lease cannot start inside a metadata utility")));
	if (next_lease_id >= PG_INT64_MAX)
		ereport(ERROR,
				(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
				 errmsg("darmok_server lease identity counter exhausted")));
	if (backend_id == 0)
		backend_id = advance_shared(&shared->next_backend_id);
	if (get_call_result_type(fcinfo, NULL, &desc) != TYPEFUNC_COMPOSITE)
		elog(ERROR, "catalog lease requires its declared composite result");
	desc = BlessTupleDesc(desc);
	transaction_lock(ShareLock, true, CurTransactionResourceOwner);
	lease.active = true;
	lease.id = ++next_lease_id;
	lease.subid = GetCurrentSubTransactionId();
	lease.owner = CurTransactionResourceOwner;
	lease.generation = pg_atomic_read_u64(&shared->generation);
	lease.local_generation = local_generation;
	/* A waiter must consume committed invalidations after obtaining the fence.
	 * This does not replace a frontend data-snapshot policy. */
	AcceptInvalidationMessages();
	InvalidateCatalogSnapshot();
	cluster_id = (bytea *) palloc(VARHDRSZ + DARMOK_CLUSTER_ID_BYTES);
	SET_VARSIZE(cluster_id, VARHDRSZ + DARMOK_CLUSTER_ID_BYTES);
	memcpy(VARDATA(cluster_id), shared->cluster_id, DARMOK_CLUSTER_ID_BYTES);
	values[0] = Int64GetDatum((int64) lease.id);
	values[1] = PointerGetDatum(cluster_id);
	values[2] = ObjectIdGetDatum(MyDatabaseId);
	values[3] = Int64GetDatum((int64) backend_id);
	values[4] = Int64GetDatum((int64) lease.generation);
	values[5] = Int64GetDatum((int64) lease.local_generation);
	PG_RETURN_DATUM(HeapTupleGetDatum(heap_form_tuple(desc, values, nulls)));
}

Datum
darmok_check_catalog_lease(PG_FUNCTION_ARGS)
{
	(void) require_lease(fcinfo);
	if (lease.generation != pg_atomic_read_u64(&shared->generation) ||
		lease.local_generation != local_generation)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("catalog lease generation changed")));
	PG_RETURN_VOID();
}

Datum
darmok_end_catalog_lease(PG_FUNCTION_ARGS)
{
	(void) require_lease(fcinfo);
	transaction_lock(ShareLock, false, lease.owner);
	lease.active = false;
	lease.owner = NULL;
	PG_RETURN_VOID();
}
