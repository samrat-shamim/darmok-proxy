/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#include "postgres.h"

#include "access/xact.h"
#include "access/xlog.h"
#include "catalog/objectaccess.h"
#include "catalog/pg_database_d.h"
#include "catalog/pg_extension_d.h"
#include "catalog/pg_tablespace_d.h"
#include "funcapi.h"
#include "miscadmin.h"
#include "nodes/parsenodes.h"
#include "port/atomics.h"
#include "storage/condition_variable.h"
#include "storage/ipc.h"
#include "storage/lock.h"
#include "storage/lwlock.h"
#include "storage/shmem.h"
#include "storage/sinval.h"
#include "tcop/utility.h"
#include "utils/builtins.h"
#include "utils/inval.h"
#include "utils/memutils.h"
#include "utils/resowner.h"
#include "utils/snapmgr.h"
#include "utils/wait_event.h"

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
	pg_atomic_uint32 pending_shared_drops;
	ConditionVariable shared_drops_finished;
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
static bool pre_commit_started = false;
static bool shared_drop_pending = false;
static bool shared_drop_cleanup_registered = false;
static SubTransactionId shared_drop_subid = InvalidSubTransactionId;
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
transaction_lock_acquire(const LOCKTAG *tag, LOCKMODE mode,
						 ResourceOwner owner)
{
	ResourceOwner saved = CurrentResourceOwner;

	/* SQL-function portals have their own resource owners. Put a lease on the
	 * current transaction owner so a later command can release the same lock. */
	CurrentResourceOwner = owner;
	PG_TRY();
	{
		(void) LockAcquire(tag, mode, false, false);
	}
	PG_FINALLY();
	{
		CurrentResourceOwner = saved;
	}
	PG_END_TRY();
}

static void
transaction_unlock(const LOCKTAG *tag, LOCKMODE mode, ResourceOwner owner)
{
	ResourceOwner saved = CurrentResourceOwner;

	CurrentResourceOwner = owner;
	PG_TRY();
	{
		if (!LockRelease(tag, mode, false))
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
clear_shared_drop_intent(int code, Datum arg)
{
	(void) code;
	(void) arg;
	if (shared_drop_pending)
	{
		shared_drop_pending = false;
		shared_drop_subid = InvalidSubTransactionId;
		Assert(pg_atomic_read_u32(&shared->pending_shared_drops) > 0);
		pg_atomic_fetch_sub_u32(&shared->pending_shared_drops, 1);
		ConditionVariableBroadcast(&shared->shared_drops_finished);
	}
}

static void
take_writer_transaction_lock(void)
{
	if (!writer_transaction_lock)
	{
		LOCKTAG tag = catalog_tag();

		transaction_lock_acquire(&tag, ExclusiveLock,
								 CurTransactionResourceOwner);
		writer_transaction_lock = true;
		writer_subid = GetCurrentSubTransactionId();
		writer_owner = CurTransactionResourceOwner;
	}
}

static void
note_metadata_attempt(void)
{
	require_ready();
	if (lease.active)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("end the catalog lease before changing metadata")));
	last_metadata_subid = GetCurrentSubTransactionId();
	advance_local();
	require_ready();
}

static bool
has_catalog_publication(void)
{
	SharedInvalidationMessage *messages = NULL;
	bool init_file;
	int count = xactGetCommittedInvalidationMessages(&messages, &init_file);

	/* This native top-level snapshot does not consume the messages. Native
	 * subabort has already removed its messages, so a rolled-back child DDL
	 * does not require an ordinary transaction publication fence. */
	if (messages != NULL)
		pfree(messages);
	return count > 0 || init_file;
}

static void
publish_catalog_change(void)
{
	require_ready();
	if (lease.active)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("end the catalog lease before publishing metadata")));
	if (preparing_transaction)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("nontransactional catalog publication cannot be prepared")));
	if (!writer_transaction_lock)
	{
		take_writer_transaction_lock();
		(void) advance_shared(&shared->generation);
	}
}

static void
begin_shared_drop_publication(void)
{
	LOCKTAG tag = catalog_tag();

	require_ready();
	if (lease.active || preparing_transaction || pre_commit_started ||
		proc_exit_inprogress || writer_transaction_lock)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("unsafe shared-drop catalog publication boundary")));
	if (!shared_drop_pending)
	{
		if (!shared_drop_cleanup_registered)
		{
			before_shmem_exit(clear_shared_drop_intent, 0);
			shared_drop_cleanup_registered = true;
		}
		shared_drop_subid = GetCurrentSubTransactionId();
		pg_atomic_fetch_add_u32(&shared->pending_shared_drops, 1);
		shared_drop_pending = true;
		/* Exclude new readers before draining existing leases. Do not retain
		 * this lock while native DROP waits for backend retirement or storage
		 * barriers: exiting temp publishers must still acquire it. The intent
		 * owns reader exclusion until transaction end; pre-commit takes the
		 * ordinary publication lock independently. One intent per transaction
		 * preserves its earliest subtransaction owner across nested drops. */
		transaction_lock_acquire(&tag, ExclusiveLock,
								 CurTransactionResourceOwner);
		(void) advance_shared(&shared->generation);
		transaction_unlock(&tag, ExclusiveLock, CurTransactionResourceOwner);
	}
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
		pg_atomic_init_u32(&shared->pending_shared_drops, 0);
		ConditionVariableInit(&shared->shared_drops_finished);
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
			pre_commit_started = true;
			if (shared_drop_pending || has_catalog_publication())
			{
				if (last_metadata_subid == InvalidSubTransactionId)
					note_metadata_attempt();
				publish_catalog_change();
			}
			break;
		case XACT_EVENT_PRE_PREPARE:
			if (shared_drop_pending)
				ereport(ERROR,
						(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
						 errmsg("shared-drop catalog publication cannot be prepared")));
			if (lease.active)
				ereport(ERROR,
						(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
						 errmsg("end the catalog lease before preparing a transaction")));
			preparing_transaction = true;
			/* Private metadata is not published by PREPARE. Never transfer our
			 * global publication fence into native two-phase lock records. */
			if (writer_transaction_lock)
			{
				LOCKTAG tag = catalog_tag();

				transaction_unlock(&tag, ExclusiveLock, writer_owner);
				writer_transaction_lock = false;
				writer_owner = NULL;
				writer_subid = InvalidSubTransactionId;
			}
			break;
		case XACT_EVENT_COMMIT:
		case XACT_EVENT_ABORT:
		case XACT_EVENT_PREPARE:
		case XACT_EVENT_PARALLEL_COMMIT:
		case XACT_EVENT_PARALLEL_ABORT:
			clear_shared_drop_intent(0, 0);
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
			pre_commit_started = false;
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
		if (shared_drop_pending && shared_drop_subid == subid)
			shared_drop_subid = parent;
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
		if (shared_drop_pending && shared_drop_subid == subid)
			clear_shared_drop_intent(0, 0);
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
			return false;
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
			/* Conservatively mark metadata attempts; native transactional
			 * invalidations decide publication. This is not SQL admission. */
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
	bool finishing = IsA(pstmt->utilityStmt, TransactionStmt) &&
		(((TransactionStmt *) pstmt->utilityStmt)->kind == TRANS_STMT_COMMIT_PREPARED ||
		 ((TransactionStmt *) pstmt->utilityStmt)->kind == TRANS_STMT_ROLLBACK_PREPARED);

	if (preparing && lease.active)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("end the catalog lease before preparing a transaction")));

	if (writer)
	{
		note_metadata_attempt();
		writer_depth++;
	}
	PG_TRY();
	{
		if (finishing)
		{
			/* Classifying a prepared target through SQL can wait on catalog
			 * locks retained by that same target. Fence every completion instead;
			 * native core alone resolves the exact GID and checks its state. */
			note_metadata_attempt();
			publish_catalog_change();
		}
		if (previous_utility)
			previous_utility(pstmt, query, read_only_tree, context, params,
							 query_env, dest, completion);
		else
			standard_ProcessUtility(pstmt, query, read_only_tree, context, params,
								query_env, dest, completion);
		if (writer)
		{
			/* An internally committing utility can return in a new transaction.
			 * Its final changes are fenced by the outer native pre-commit, with
			 * a new generation, rather than acquiring a fence across phase waits. */
			if (IsTransactionState() &&
				last_metadata_subid == InvalidSubTransactionId)
				note_metadata_attempt();
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
	if (access == OAT_POST_CREATE || access == OAT_POST_ALTER || access == OAT_DROP)
	{
		if (writer_depth == 0)
			note_metadata_attempt();
		if (access == OAT_DROP &&
				 (class_id == DatabaseRelationId || class_id == TableSpaceRelationId))
			/* The invalid database marker and directory removal precede commit.
			 * Exclude readers here without fencing native cleanup publishers. */
			begin_shared_drop_publication();
		else if (pre_commit_started && !preparing_transaction)
			publish_catalog_change();
	}
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
	if (!IsTransactionBlock())
		ereport(ERROR,
				(errcode(ERRCODE_NO_ACTIVE_SQL_TRANSACTION),
				 errmsg("a catalog lease requires a transaction block")));
	if (lease.active)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("this backend already owns a catalog lease")));
	if (writer_depth > 0 || preparing_transaction || pre_commit_started ||
		shared_drop_pending)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("a catalog lease cannot start during a metadata utility or transaction finalization")));
	if (next_lease_id >= PG_INT64_MAX)
		ereport(ERROR,
				(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
				 errmsg("darmok_server lease identity counter exhausted")));
	if (backend_id == 0)
		backend_id = advance_shared(&shared->next_backend_id);
	if (get_call_result_type(fcinfo, NULL, &desc) != TYPEFUNC_COMPOSITE)
		elog(ERROR, "catalog lease requires its declared composite result");
	desc = BlessTupleDesc(desc);
	{
		LOCKTAG tag = catalog_tag();

		/* Shared drops exclude new readers through their native lifecycle,
		 * without holding this fence across backend retirement/storage waits.
		 * Recheck after Share acquisition to close the admission race. */
		for (;;)
		{
			PG_TRY();
			{
				while (pg_atomic_read_u32(&shared->pending_shared_drops) != 0)
					ConditionVariableSleep(&shared->shared_drops_finished,
									   PG_WAIT_EXTENSION);
			}
			PG_FINALLY();
			{
				ConditionVariableCancelSleep();
			}
			PG_END_TRY();
			transaction_lock_acquire(&tag, ShareLock,
									 CurTransactionResourceOwner);
			if (pg_atomic_read_u32(&shared->pending_shared_drops) == 0)
				break;
			transaction_unlock(&tag, ShareLock, CurTransactionResourceOwner);
		}
	}
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
	{
		LOCKTAG tag = catalog_tag();

		transaction_unlock(&tag, ShareLock, lease.owner);
	}
	lease.active = false;
	lease.owner = NULL;
	PG_RETURN_VOID();
}
