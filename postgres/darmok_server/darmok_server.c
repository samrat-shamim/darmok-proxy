/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#include "postgres.h"

#include "access/xact.h"
#include "access/xlog.h"
#include "access/parallel.h"
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
#include "utils/guc.h"
#include "utils/inval.h"
#include "utils/memutils.h"
#include "utils/resowner.h"
#include "utils/snapmgr.h"
#include "utils/wait_event.h"

#include "catalog_read.h"

#if PG_VERSION_NUM < 170000 || PG_VERSION_NUM >= 190000
#error "darmok_server requires PostgreSQL 17 or 18"
#endif

PG_MODULE_MAGIC;

PGDLLEXPORT void _PG_init(void);

/* OID zero cannot name an extension. The default lock method is deliberately
 * distinct from user advisory locks. Database zero makes this cluster-wide. */
#define DARMOK_LOCK_SUBID 0x444d
#define DARMOK_PUBLICATION_SUBID 0x444e
#define DARMOK_CLUSTER_ID_BYTES 16

typedef struct DarmokShared
{
	unsigned char cluster_id[DARMOK_CLUSTER_ID_BYTES];
	pg_atomic_uint64 generation;
	pg_atomic_uint64 next_backend_id;
	pg_atomic_uint32 pending_shared_drops;
	ConditionVariable shared_drops_finished;
} DarmokShared;

static DarmokShared *shared = NULL;
static uint64 backend_id = 0;
static uint64 local_generation = 1;
static bool broken = false;
/* These flags belong only to one utility invocation, never a frontend handle. */
static bool reader_active = false;
static bool reader_gate_held = false;
static bool reader_fence_held = false;
static ResourceOwner reader_owner = NULL;
static bool publication_gate_held = false;
static SubTransactionId publication_subid = InvalidSubTransactionId;
static ResourceOwner publication_owner = NULL;
static bool completion_fence_held = false;
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

static LOCKTAG
publication_tag(void)
{
	LOCKTAG tag;

	SET_LOCKTAG_OBJECT(tag, InvalidOid, ExtensionRelationId, InvalidOid,
					   DARMOK_PUBLICATION_SUBID);
	return tag;
}

static void
require_ready(void)
{
	if (shared == NULL || broken)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("darmok_server catalog state is unavailable")));
	if (RecoveryInProgress())
		ereport(ERROR,
				(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
				 errmsg("darmok_server catalog discovery requires a primary server")));
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

	/* Publication uses the transaction owner; an internal read uses its
	 * current utility owner and releases the fence before returning. */
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

/* Ordinary publishers retain reader exclusion, not the global fence, during
 * native ON COMMIT work. A prepared transaction must be able to finish while
 * that work waits for one of its catalog locks. */
static void
take_publication_gate(void)
{
	if (!publication_gate_held)
	{
		LOCKTAG tag = publication_tag();

		transaction_lock_acquire(&tag, RowExclusiveLock,
								 CurTransactionResourceOwner);
		publication_gate_held = true;
		publication_subid = GetCurrentSubTransactionId();
		publication_owner = CurTransactionResourceOwner;
	}
}

static void
drain_catalog_readers(void)
{
	LOCKTAG tag = catalog_tag();
	ResourceOwner owner = CurTransactionResourceOwner;
	volatile bool acquired = false;

	PG_TRY();
	{
		transaction_lock_acquire(&tag, ExclusiveLock, owner);
		acquired = true;
		(void) advance_shared(&shared->generation);
	}
	PG_FINALLY();
	{
		if (acquired)
			transaction_unlock(&tag, ExclusiveLock, owner);
	}
	PG_END_TRY();
}

static void
note_metadata_attempt(void)
{
	require_ready();
	if (reader_active)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("metadata cannot change during catalog discovery")));
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
	if (reader_active || completion_fence_held)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("unsafe catalog publication boundary")));
	if (preparing_transaction)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("nontransactional catalog publication cannot be prepared")));
	if (!publication_gate_held)
	{
		take_publication_gate();
		drain_catalog_readers();
	}
}

static void
begin_shared_drop_publication(void)
{
	require_ready();
	if (reader_active || preparing_transaction || pre_commit_started ||
		proc_exit_inprogress || publication_gate_held || completion_fence_held)
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
		/* Exclude new readers before draining existing internal fences. Do not retain
		 * this lock while native DROP waits for backend retirement or storage
		 * barriers: exiting temp publishers must still acquire it. The intent
		 * owns reader exclusion until transaction end; pre-commit takes the
		 * ordinary publication lock independently. One intent per transaction
		 * preserves its earliest subtransaction owner across nested drops. */
		drain_catalog_readers();
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
			if (completion_fence_held)
				ereport(ERROR,
						(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
						 errmsg("native cleanup cannot retain a prepared completion fence")));
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
			if (reader_active || completion_fence_held)
				ereport(ERROR,
						(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
						 errmsg("unsafe catalog coordination boundary for prepare")));
			preparing_transaction = true;
			/* Private metadata is not published by PREPARE. Never transfer our
			 * publication gate into native two-phase lock records. */
			if (publication_gate_held)
			{
				LOCKTAG tag = publication_tag();

				transaction_unlock(&tag, RowExclusiveLock, publication_owner);
				publication_gate_held = false;
				publication_owner = NULL;
				publication_subid = InvalidSubTransactionId;
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
			reader_active = false;
			reader_gate_held = false;
			reader_fence_held = false;
			reader_owner = NULL;
			/* Native lock cleanup releases the publication gate AFTER catalog
			 * invalidations. Do not release its actual reference here. */
			publication_gate_held = false;
			publication_owner = NULL;
			publication_subid = InvalidSubTransactionId;
			completion_fence_held = false;
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
		if (publication_gate_held && publication_subid == subid)
		{
			publication_subid = parent;
			publication_owner = ResourceOwnerGetParent(publication_owner);
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
		if (publication_gate_held && publication_subid == subid)
		{
			publication_gate_held = false;
			publication_owner = NULL;
			publication_subid = InvalidSubTransactionId;
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
	ResourceOwner completion_owner = CurTransactionResourceOwner;
	volatile bool completion_acquired = false;

	if (IsA(pstmt->utilityStmt, VariableShowStmt) &&
		strcmp(((VariableShowStmt *) pstmt->utilityStmt)->name,
			   DARMOK_CATALOG_REQUEST) == 0)
	{
		darmok_catalog_show(context, dest, completion);
		return;
	}

	if (preparing && reader_active)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("a transaction cannot prepare during catalog discovery")));

	if (writer)
	{
		note_metadata_attempt();
		writer_depth++;
	}
	PG_TRY();
	{
		if (finishing)
		{
			LOCKTAG tag = catalog_tag();

			if (reader_active || publication_gate_held || completion_fence_held ||
				preparing_transaction || pre_commit_started || shared_drop_pending)
				ereport(ERROR,
						(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
						 errmsg("unsafe prepared completion catalog boundary")));
			/* Classifying a prepared target through SQL can wait on catalog
			 * locks retained by that same target. Fence every completion instead;
			 * native core alone resolves the exact GID and checks its state. */
			note_metadata_attempt();
			transaction_lock_acquire(&tag, ExclusiveLock, completion_owner);
			completion_acquired = true;
			completion_fence_held = true;
			(void) advance_shared(&shared->generation);
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
		if (completion_acquired)
		{
			LOCKTAG tag = catalog_tag();

			/* Native FinishPreparedTransaction sends saved invalidations and
			 * releases the target before returning. The caller may have prior
			 * Parse/Bind temp history: its own late cleanup must run WITHOUT
			 * this fence, using ordinary gate-first publication if necessary. */
			completion_fence_held = false;
			transaction_unlock(&tag, ExclusiveLock, completion_owner);
		}
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
	darmok_catalog_define_guc();
	MarkGUCPrefixReserved("darmok_server");
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

void
darmok_catalog_reader_start(void)
{
	require_ready();
	if (reader_active || writer_depth > 0 || publication_gate_held ||
		completion_fence_held ||
		preparing_transaction || pre_commit_started || shared_drop_pending ||
		HistoricSnapshotActive() || IsParallelWorker() || IsInParallelMode() ||
		ParallelContextActive())
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("unsafe native catalog discovery boundary")));
	if (backend_id == 0)
		backend_id = advance_shared(&shared->next_backend_id);
	reader_owner = CurrentResourceOwner;
	reader_active = true;
}

void
darmok_catalog_fence_acquire(DarmokCatalogStamp *stamp)
{
	LOCKTAG tag = catalog_tag();
	LOCKTAG gate = publication_tag();

	Assert(reader_active && !reader_gate_held && !reader_fence_held);
	require_ready();
	/* Shared drops close admission through their native lifecycle. No catalog
	 * work occurs while waiting or while this raw observation fence is held. */
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
		/* Gate-first acquisition excludes unfinished ordinary publications.
		 * Prepared completion uses only global Exclusive, so it can release
		 * native locks needed by a publisher's late ON COMMIT cleanup. */
		transaction_lock_acquire(&gate, ShareLock, reader_owner);
		reader_gate_held = true;
		transaction_lock_acquire(&tag, ShareLock, reader_owner);
		reader_fence_held = true;
		if (pg_atomic_read_u32(&shared->pending_shared_drops) == 0)
			break;
		darmok_catalog_fence_release();
	}
	memcpy(stamp->cluster_id, shared->cluster_id, DARMOK_CLUSTER_ID_BYTES);
	stamp->database_oid = MyDatabaseId;
	stamp->backend_id = backend_id;
	stamp->generation = pg_atomic_read_u64(&shared->generation);
	stamp->local_generation = local_generation;
}

void
darmok_catalog_fence_release(void)
{
	PG_TRY();
	{
		if (reader_fence_held)
		{
			LOCKTAG tag = catalog_tag();

			reader_fence_held = false;
			transaction_unlock(&tag, ShareLock, reader_owner);
		}
	}
	PG_FINALLY();
	{
		if (reader_gate_held)
		{
			LOCKTAG gate = publication_tag();

			reader_gate_held = false;
			transaction_unlock(&gate, ShareLock, reader_owner);
		}
	}
	PG_END_TRY();
}

bool
darmok_catalog_stamp_equal(const DarmokCatalogStamp *left,
						   const DarmokCatalogStamp *right)
{
	return left->generation == right->generation &&
		left->local_generation == right->local_generation;
}

void
darmok_catalog_reader_finish(void)
{
	/* Call before any scan/snapshot/relation cleanup, serialization or receiver
	 * work, also from the invocation's PG_FINALLY on ordinary native ERROR. */
	darmok_catalog_fence_release();
	reader_active = false;
	reader_owner = NULL;
}
