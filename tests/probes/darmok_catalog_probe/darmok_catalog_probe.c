/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
/* Test control only. This module is never part of the product artifact. */
#include "postgres.h"

#include "access/xact.h"
#include "access/relation.h"
#include "catalog/pg_am_d.h"
#include "catalog/pg_extension_d.h"
#include "catalog/catalog.h"
#include "catalog/objectaccess.h"
#include "catalog/pg_database_d.h"
#include "fmgr.h"
#include "funcapi.h"
#include "miscadmin.h"
#include "lib/stringinfo.h"
#include "storage/lock.h"
#include "storage/sinval.h"
#include "tcop/utility.h"
#include "utils/builtins.h"
#include "utils/guc.h"
#include "utils/inval.h"
#include "utils/memutils.h"
#include "utils/resowner.h"
#include "utils/rel.h"
#include "utils/snapmgr.h"

#include "statement_guard.h"
#include "relation_guard.h"
#include "heap_storage_probe.h"
#include "module_footprint.h"
#include "builtin_dispatch.h"

PG_MODULE_MAGIC;
PGDLLEXPORT void _PG_init(void);

static ProcessUtility_hook_type previous_utility = NULL;
static object_access_hook_type previous_object_access = NULL;
static bool shared_drop_barrier = false;
static ResourceOwner owner = NULL;
static SubTransactionId subid = InvalidSubTransactionId;
static char *command = NULL;
static char *guard_status = NULL;
static char *relation_status = NULL;
static char *module_footprint_status = NULL;
static char *builtin_dispatch_status = NULL;
static DarmokRelationAttempt relation_token = {0};
static DarmokRelationAttempt stale_relation_token = {0};
static SubTransactionId relation_subid = InvalidSubTransactionId;
static const char *relation_outcome = "idle";
static char *clear_results = NULL;
static bool observing_acquire = false;
static bool caller_owner_restored = true;
static bool callback_owner_changed = false;
static bool callback_fence_seen = false;
static uint64 acquire_invalidations = 0;
static uint64 acquire_callbacks = 0;
static uint64 watched_callbacks = 0;
static Oid warmed_oid = InvalidOid;
static int warmed_attributes = 0;
static bool warm_before_snapshot = false;
static bool warm_after_snapshot = false;
static DarmokRelationRequest *borrowed_requests = NULL;
static int borrowed_count = 0;
static ResourceOwner borrowed_owner = NULL;
static SubTransactionId borrowed_subid = InvalidSubTransactionId;
/* This held token intentionally spans SET/SHOW only in the separate native
 * test image. Product callers release within one C invocation's PG_FINALLY. */
static DarmokStatementGuard guard_token = {0};
static DarmokStatementGuard stale_token = {0};
static SubTransactionId guard_subid = InvalidSubTransactionId;
static bool before_snapshot = false;
static bool after_snapshot = false;
static const char *guard_outcome = "idle";
static bool omit_prepare_coverage = false;
static SubTransactionId omit_subid = InvalidSubTransactionId;

typedef bool (*GuardAcquire) (volatile DarmokStatementGuard *);
typedef void (*GuardRelease) (volatile DarmokStatementGuard *);
typedef bool (*GuardOwned) (const volatile DarmokStatementGuard *);
typedef void (*GuardCheckPrepared) (void);

static GuardAcquire guard_acquire = NULL;
static GuardRelease guard_release = NULL;
static GuardOwned guard_owned = NULL;
static GuardCheckPrepared guard_check_prepared = NULL;
typedef void (*RelationAcquire) (volatile DarmokRelationAttempt *, const DarmokRelationRequest *, int);
typedef void (*RelationComplete) (volatile DarmokRelationAttempt *);
typedef bool (*RelationOwned) (const volatile DarmokRelationAttempt *);
static RelationAcquire relation_acquire = NULL;
static RelationComplete relation_release = NULL;
static RelationComplete relation_retain = NULL;
static RelationOwned relation_owned = NULL;

typedef DarmokModuleFootprint *(*ModuleCapture) (void);
typedef const char *(*ModuleImage) (const DarmokModuleFootprint *, Size *);
typedef uint32 (*ModuleCount) (const DarmokModuleFootprint *);
typedef Size (*ModuleRequested) (const DarmokModuleFootprint *);
typedef void (*ModuleRelease) (DarmokModuleFootprint *);
static ModuleCapture module_capture = NULL;
static ModuleImage module_image = NULL;
static ModuleCount module_count = NULL;
static ModuleRequested module_requested = NULL;
static ModuleRelease module_release = NULL;

typedef void (*BuiltinCapture) (DarmokBuiltinDispatch *);
static BuiltinCapture builtin_capture = NULL;

static void
object_access(ObjectAccessType access, Oid class_id, Oid object_id,
			  int sub_id, void *arg)
{
	if (previous_object_access)
		previous_object_access(access, class_id, object_id, sub_id, arg);
	if (shared_drop_barrier && access == OAT_DROP && class_id == DatabaseRelationId)
	{
		LOCKTAG tag;

		/* Test synchronization only. The product registers and drains the real
		 * shared-drop intent before this delegate returns, in either preload
		 * order. Keep the second native DROP alive without a module fence until
		 * the fixture releases its ordinary transaction-level advisory holder. */
		SET_LOCKTAG_ADVISORY(tag, MyDatabaseId, 17485, 21316, 2);
		(void) LockAcquire(&tag, ShareLock, false, false);
	}
}

static void
show_builtin_dispatch(DestReceiver *dest, QueryCompletion *completion)
{
	DarmokBuiltinDispatch observation;
	bool snapshot = FirstSnapshotSet;
	ResourceOwner current_owner = CurrentResourceOwner;
	ResourceOwner transaction_owner = CurTransactionResourceOwner;
	StringInfo text;
	TupOutputState *output;

	if (builtin_capture == NULL)
		builtin_capture = (BuiltinCapture) load_external_function("$libdir/darmok_server",
																  "darmok_builtin_dispatch_capture", true, NULL);
	builtin_capture(&observation);
	text = makeStringInfo();
	appendStringInfo(text, "{\"builtin_count\":%d,\"last_builtin_oid\":%u,\"rows\":[",
					 observation.builtin_count, observation.last_builtin_oid);
	for (int i = 0; i < DARMOK_BUILTIN_DISPATCH_ROWS; i++)
	{
		const DarmokBuiltinDispatchRow *row = &observation.rows[i];

		/* Captured names are the two checked ASCII literals, not arbitrary
		 * catalog text. The product returns no native function/name pointers. */
		appendStringInfo(text, "%s{\"oid\":%u,\"index\":%u,\"nargs\":%d,\"strict\":%s,\"retset\":%s,\"name\":\"%s\",\"linked_symbol\":%s}",
						 i == 0 ? "" : ",", row->oid, row->index, row->nargs,
						 row->strict ? "true" : "false", row->retset ? "true" : "false", row->name,
						 row->linked_symbol ? "true" : "false");
	}
	appendStringInfo(text, "],\"before_snapshot\":%s,\"after_snapshot\":%s,\"owners_unchanged\":%s}",
					 snapshot ? "true" : "false", FirstSnapshotSet ? "true" : "false",
					 current_owner == CurrentResourceOwner && transaction_owner == CurTransactionResourceOwner
					 ? "true" : "false");
	output = begin_tup_output_tupdesc(dest,
									  GetPGVariableResultDesc("darmok_catalog_probe.builtin_dispatch"),
									  &TTSOpsVirtual);
	do_text_output_oneline(output, text->data);
	end_tup_output(output);
	pfree(text->data);
	pfree(text);
	if (FirstSnapshotSet != snapshot)
		elog(ERROR, "native builtin probe SHOW changed first data snapshot");
	SetQueryCompletion(completion, CMDTAG_SHOW, 0);
}

static void
show_modules(DestReceiver *dest, QueryCompletion *completion)
{
	static const char hex[] = "0123456789abcdef";
	StringInfo text = makeStringInfo();
	DarmokModuleFootprint *footprint;
	bool snapshot = FirstSnapshotSet;
	ResourceOwner current_owner = CurrentResourceOwner;
	ResourceOwner transaction_owner = CurTransactionResourceOwner;
	TupOutputState *output;

	if (module_capture == NULL)
	{
		ModuleCapture capture;
		ModuleImage image;
		ModuleCount count;
		ModuleRequested requested;
		ModuleRelease release;

		capture = (ModuleCapture) load_external_function("$libdir/darmok_server",
															 "darmok_module_footprint_capture", true, NULL);
		image = (ModuleImage) load_external_function("$libdir/darmok_server",
														 "darmok_module_footprint_image", true, NULL);
		count = (ModuleCount) load_external_function("$libdir/darmok_server",
														 "darmok_module_footprint_count", true, NULL);
		requested = (ModuleRequested) load_external_function("$libdir/darmok_server",
															 "darmok_module_footprint_requested_bytes", true, NULL);
		release = (ModuleRelease) load_external_function("$libdir/darmok_server",
															 "darmok_module_footprint_release", true, NULL);
		module_image = image;
		module_count = count;
		module_requested = requested;
		module_release = release;
		module_capture = capture;
	}
	footprint = module_capture();
	PG_TRY();
	{
		Size bytes;
		const unsigned char *image = (const unsigned char *) module_image(footprint, &bytes);

		appendStringInfo(text, "{\"paths\":%u,\"bytes\":%zu,\"requested_bytes\":%zu,\"image_hex\":\"",
						 module_count(footprint), bytes, module_requested(footprint));
		for (Size i = 0; i < bytes; i++)
		{
			appendStringInfoChar(text, hex[image[i] >> 4]);
			appendStringInfoChar(text, hex[image[i] & 15]);
		}
		appendStringInfo(text, "\",\"before_snapshot\":%s,\"after_snapshot\":%s,\"owners_unchanged\":%s}",
						 snapshot ? "true" : "false", FirstSnapshotSet ? "true" : "false",
						 current_owner == CurrentResourceOwner && transaction_owner == CurTransactionResourceOwner
						 ? "true" : "false");
	}
	PG_FINALLY();
	{
		module_release(footprint);
	}
	PG_END_TRY();
	output = begin_tup_output_tupdesc(dest,
									  GetPGVariableResultDesc("darmok_catalog_probe.module_footprint"),
									  &TTSOpsVirtual);
	do_text_output_oneline(output, text->data);
	end_tup_output(output);
	pfree(text->data);
	pfree(text);
	if (FirstSnapshotSet != snapshot)
		elog(ERROR, "native module probe SHOW changed first data snapshot");
	SetQueryCompletion(completion, CMDTAG_SHOW, 0);
}

static void
resolve_relation(void)
{
	if (relation_acquire == NULL)
	{
		RelationAcquire acquire;
		RelationComplete release;
		RelationComplete retain;
		RelationOwned owned;

		acquire = (RelationAcquire) load_external_function("$libdir/darmok_server",
															 "darmok_relation_attempt_acquire", true, NULL);
		release = (RelationComplete) load_external_function("$libdir/darmok_server",
															   "darmok_relation_attempt_release", true, NULL);
		retain = (RelationComplete) load_external_function("$libdir/darmok_server",
															  "darmok_relation_attempt_retain", true, NULL);
		owned = (RelationOwned) load_external_function("$libdir/darmok_server",
															 "darmok_relation_attempt_owned", true, NULL);
		relation_release = release;
		relation_retain = retain;
		relation_owned = owned;
		relation_acquire = acquire;
	}
}

static DarmokRelationRequest *
parse_relation_requests(const char *text, int *count)
{
	DarmokRelationRequest *requests;
	const char *cursor = text;
	int n = 1;

	for (const char *p = text; *p != '\0'; p++)
		if (*p == ',')
			n++;
	if (n > DARMOK_RELATION_REQUEST_LIMIT + 1)
		elog(ERROR, "native relation probe request is too large");
	requests = palloc(sizeof(DarmokRelationRequest) * n);
	for (int i = 0; i < n; i++)
	{
		char *end;
		unsigned long oid;
		unsigned long mode;

		if (*cursor < '0' || *cursor > '9')
			elog(ERROR, "native relation probe requires decimal OID/mode pairs");
		oid = strtoul(cursor, &end, 10);
		if (end == cursor || *end != '/' || oid > PG_UINT32_MAX)
			elog(ERROR, "invalid native relation probe OID");
		cursor = end + 1;
		if (*cursor < '0' || *cursor > '9')
			elog(ERROR, "invalid native relation probe mode");
		mode = strtoul(cursor, &end, 10);
		if (end == cursor || mode > INT_MAX ||
			(i == n - 1 ? *end != '\0' : *end != ','))
			elog(ERROR, "invalid native relation probe mode or separator");
		requests[i].relation_oid = (Oid) oid;
		requests[i].lock_mode = (LOCKMODE) mode;
		cursor = end + 1;
	}
	*count = n;
	return requests;
}

static bool
probe_has_coordination(void)
{
	for (int sub = 0x444d; sub <= 0x444f; sub++)
	{
		LOCKTAG tag;

		SET_LOCKTAG_OBJECT(tag, InvalidOid, ExtensionRelationId, InvalidOid, sub);
		for (LOCKMODE mode = AccessShareLock; mode <= AccessExclusiveLock; mode++)
			if (LockHeldByMe(&tag, mode, false))
				return true;
	}
	return false;
}

static void
observe_relcache(Datum arg, Oid relation_oid)
{
	(void) arg;
	/* Passive only: no SQL, private API call, lock acquisition or ERROR. */
	if (observing_acquire)
	{
		acquire_callbacks++;
		if (relation_oid == warmed_oid)
			watched_callbacks++;
		callback_owner_changed |= CurrentResourceOwner != CurTransactionResourceOwner;
		callback_fence_seen |= probe_has_coordination();
	}
}

static void
observe_relation_acquire(volatile DarmokRelationAttempt *token,
						 const DarmokRelationRequest *requests, int count)
{
	ResourceOwner saved = CurrentResourceOwner;
	uint64 before_invalidations = SharedInvalidMessageCounter;

	before_snapshot = FirstSnapshotSet;
	acquire_callbacks = 0;
	watched_callbacks = 0;
	callback_owner_changed = false;
	callback_fence_seen = false;
	observing_acquire = true;
	PG_TRY();
	{
		relation_acquire(token, requests, count);
	}
	PG_FINALLY();
	{
		observing_acquire = false;
		after_snapshot = FirstSnapshotSet;
		acquire_invalidations = SharedInvalidMessageCounter - before_invalidations;
		caller_owner_restored = CurrentResourceOwner == saved;
	}
	PG_END_TRY();
}

static void
sample_relation_clear(const DarmokRelationRequest *requests, int count)
{
	ResourceOwner saved = CurrentResourceOwner;
	StringInfoData result;

	if (count <= 0 || count > DARMOK_RELATION_REQUEST_LIMIT)
		elog(ERROR, "native clear sampler requires a bounded exact request list");
	initStringInfo(&result);
	appendStringInfoChar(&result, '[');
	CurrentResourceOwner = CurTransactionResourceOwner;
	PG_TRY();
	{
		for (int i = 0; i < count; i++)
		{
			LOCKTAG tag;
			LockAcquireResult observed;

			if (!OidIsValid(requests[i].relation_oid) ||
				requests[i].lock_mode < AccessShareLock ||
				requests[i].lock_mode > RowExclusiveLock)
				elog(ERROR, "native clear sampler requires an exact weak mode");
			SET_LOCKTAG_RELATION(tag,
								IsSharedRelation(requests[i].relation_oid) ? InvalidOid : MyDatabaseId,
								requests[i].relation_oid);
			/* Exact mode must already be held; no sampling wait/dispatch/mark. */
			if (!LockHeldByMe(&tag, requests[i].lock_mode, false))
				elog(ERROR, "native clear sampler requires an already-held exact mode");
#if PG_VERSION_NUM >= 180000
			observed = LockAcquireExtended(&tag, requests[i].lock_mode,
										 false, false, true, NULL, false);
#else
			observed = LockAcquireExtended(&tag, requests[i].lock_mode,
										 false, false, true, NULL);
#endif
			if (!LockRelease(&tag, requests[i].lock_mode, false))
				elog(ERROR, "native clear sampler lost its extra increment");
			if (observed != LOCKACQUIRE_ALREADY_HELD &&
				observed != LOCKACQUIRE_ALREADY_CLEAR)
				elog(ERROR, "unexpected native clear sampler acquisition result");
			appendStringInfo(&result, "%s\"%s\"", i == 0 ? "" : ",",
							 observed == LOCKACQUIRE_ALREADY_CLEAR ? "already_clear" : "already_held");
		}
	}
	PG_FINALLY();
	{
		CurrentResourceOwner = saved;
	}
	PG_END_TRY();
	appendStringInfoChar(&result, ']');
	if (clear_results != NULL)
		pfree(clear_results);
	clear_results = MemoryContextStrdup(TopMemoryContext, result.data);
	pfree(result.data);
}

static void
warm_relation(const DarmokRelationRequest *requests, int count)
{
	Relation relation;

	if (count != 1 || requests[0].lock_mode != AccessShareLock || probe_has_coordination())
		elog(ERROR, "native warm probe requires one AccessShare request outside exclusion");
	warm_before_snapshot = FirstSnapshotSet;
	relation = relation_open(requests[0].relation_oid, AccessShareLock);
	PG_TRY();
	{
		if (relation->rd_rel->relkind != RELKIND_RELATION ||
			relation->rd_rel->relam != HEAP_TABLE_AM_OID)
			elog(ERROR, "native warm probe requires an ordinary builtin heap");
		warmed_oid = RelationGetRelid(relation);
		warmed_attributes = RelationGetDescr(relation)->natts;
	}
	PG_FINALLY();
	{
		/* No descriptor or warm increment survives to block prepared AX. */
		relation_close(relation, AccessShareLock);
	}
	PG_END_TRY();
	warm_after_snapshot = FirstSnapshotSet;
}

static void
relation_command(const char *value)
{
	const char *colon = strchr(value, ':');
	DarmokRelationRequest *requests = NULL;
	int count = 0;

	resolve_relation();
	if (colon != NULL)
		requests = parse_relation_requests(colon + 1, &count);
	if (strcmp(value, "relation_release") == 0)
	{
		relation_release(&relation_token);
		relation_subid = InvalidSubTransactionId;
		relation_outcome = "released";
	}
	else if (strcmp(value, "relation_retain") == 0)
	{
		relation_retain(&relation_token);
		relation_subid = InvalidSubTransactionId;
		relation_outcome = "retained";
	}
	else if (strcmp(value, "relation_copy") == 0)
	{
		if (!relation_owned(&relation_token))
			elog(ERROR, "cannot copy unowned relation token");
		stale_relation_token.identity = relation_token.identity;
	}
	else if (strcmp(value, "relation_stale_release") == 0)
		relation_release(&stale_relation_token);
	else if (strcmp(value, "relation_stale_retain") == 0)
		relation_retain(&stale_relation_token);
	else if (strcmp(value, "relation_empty") == 0)
		relation_acquire(&relation_token, NULL, 0);
	else if (strncmp(value, "relation_clear:", 15) == 0)
		sample_relation_clear(requests, count);
	else if (strncmp(value, "relation_warm:", 14) == 0)
		warm_relation(requests, count);
	else if (strncmp(value, "relation_hold:", 14) == 0)
	{
		observe_relation_acquire(&relation_token, requests, count);
		relation_subid = GetCurrentSubTransactionId();
		relation_outcome = "acquired";
	}
	else if (strncmp(value, "relation_scoped:", 16) == 0 ||
			 strncmp(value, "relation_scoped_error:", 22) == 0 ||
			 strncmp(value, "relation_mutated:", 17) == 0 ||
			 strncmp(value, "relation_scoped_retain:", 23) == 0)
	{
		volatile DarmokRelationAttempt scoped = {0};

		PG_TRY();
		{
			observe_relation_acquire(&scoped, requests, count);
			relation_outcome = "scoped";
			if (strncmp(value, "relation_mutated:", 17) == 0)
				memset(requests, 0, sizeof(DarmokRelationRequest) * count);
			if (strncmp(value, "relation_scoped_error:", 22) == 0)
				elog(ERROR, "ordinary native scoped relation fixture error");
			if (strncmp(value, "relation_scoped_retain:", 23) == 0)
			{
				relation_retain(&scoped);
				relation_outcome = "retained";
			}
		}
		PG_FINALLY();
		{
			if (relation_owned(&scoped))
				relation_release(&scoped);
		}
		PG_END_TRY();
	}
	else if (strncmp(value, "relation_borrow:", 16) == 0)
	{
		ResourceOwner saved = CurrentResourceOwner;

		if (borrowed_requests != NULL)
			elog(ERROR, "native relation probe already owns borrowed references");
		borrowed_requests = MemoryContextAlloc(TopTransactionContext,
												  sizeof(DarmokRelationRequest) * count);
		memcpy(borrowed_requests, requests, sizeof(DarmokRelationRequest) * count);
		borrowed_count = count;
		borrowed_owner = CurTransactionResourceOwner;
		borrowed_subid = GetCurrentSubTransactionId();
		CurrentResourceOwner = borrowed_owner;
		PG_TRY();
		{
			for (int i = 0; i < count; i++)
			{
				LOCKTAG tag;

				SET_LOCKTAG_RELATION(tag,
									IsSharedRelation(requests[i].relation_oid) ? InvalidOid : MyDatabaseId,
									requests[i].relation_oid);
				(void) LockAcquire(&tag, requests[i].lock_mode, false, false);
			}
		}
		PG_FINALLY();
		{
			CurrentResourceOwner = saved;
		}
		PG_END_TRY();
	}
	else if (strcmp(value, "relation_unborrow") == 0)
	{
		ResourceOwner saved = CurrentResourceOwner;

		if (borrowed_requests == NULL)
			elog(ERROR, "native relation probe has no borrowed references");
		CurrentResourceOwner = borrowed_owner;
		PG_TRY();
		{
			for (int i = borrowed_count - 1; i >= 0; i--)
			{
				LOCKTAG tag;

				SET_LOCKTAG_RELATION(tag,
									IsSharedRelation(borrowed_requests[i].relation_oid) ? InvalidOid : MyDatabaseId,
									borrowed_requests[i].relation_oid);
				if (!LockRelease(&tag, borrowed_requests[i].lock_mode, false))
					elog(ERROR, "native relation probe lost borrowed reference");
			}
		}
		PG_FINALLY();
		{
			CurrentResourceOwner = saved;
		}
		PG_END_TRY();
		pfree(borrowed_requests);
		borrowed_requests = NULL;
		borrowed_owner = NULL;
		borrowed_subid = InvalidSubTransactionId;
		borrowed_count = 0;
	}
	else
		elog(ERROR, "unknown native relation probe command");
	if (requests != NULL)
		pfree(requests);
}

static void
show_relation(DestReceiver *dest, QueryCompletion *completion)
{
	TupOutputState *output;
	char *text;
	bool snapshot = FirstSnapshotSet;

	resolve_relation();
	text = psprintf("{\"owned\":%s,\"first_snapshot\":%s,\"before_snapshot\":%s,\"after_snapshot\":%s,\"outcome\":\"%s\","
					"\"clear_results\":%s,\"caller_owner_restored\":%s,\"callback_owner_changed\":%s,\"callback_fence_seen\":%s,"
					"\"acquire_invalidations\":" UINT64_FORMAT ",\"acquire_callbacks\":" UINT64_FORMAT ",\"watched_callbacks\":" UINT64_FORMAT ","
					"\"warmed_oid\":%u,\"warmed_attributes\":%d,\"warm_before_snapshot\":%s,\"warm_after_snapshot\":%s}",
					relation_owned(&relation_token) ? "true" : "false",
					snapshot ? "true" : "false", before_snapshot ? "true" : "false",
					after_snapshot ? "true" : "false", relation_outcome,
					clear_results == NULL ? "[]" : clear_results,
					caller_owner_restored ? "true" : "false", callback_owner_changed ? "true" : "false",
					callback_fence_seen ? "true" : "false", acquire_invalidations, acquire_callbacks, watched_callbacks,
					warmed_oid, warmed_attributes, warm_before_snapshot ? "true" : "false", warm_after_snapshot ? "true" : "false");
	output = begin_tup_output_tupdesc(dest,
									  GetPGVariableResultDesc("darmok_catalog_probe.relation_status"),
									  &TTSOpsVirtual);
	do_text_output_oneline(output, text);
	end_tup_output(output);
	pfree(text);
	if (FirstSnapshotSet != snapshot)
		elog(ERROR, "native relation probe SHOW changed first data snapshot");
	SetQueryCompletion(completion, CMDTAG_SHOW, 0);
}

static void
resolve_guard(void)
{
	if (guard_acquire == NULL)
	{
		GuardAcquire acquire;
		GuardRelease release;
		GuardOwned owned;
		GuardCheckPrepared check_prepared;

		/* PostgreSQL's native loader erases the symbol's C prototype. These
		 * casts restore the exact private header ABI; no SQL function exists.
		 * Resolve on use so the ordered coverage probe can preload first. */
		acquire = (GuardAcquire) load_external_function("$libdir/darmok_server",
															 "darmok_statement_guard_acquire", true, NULL);
		release = (GuardRelease) load_external_function("$libdir/darmok_server",
																 "darmok_statement_guard_release", true, NULL);
		owned = (GuardOwned) load_external_function("$libdir/darmok_server",
														   "darmok_statement_guard_owned", true, NULL);
		check_prepared = (GuardCheckPrepared) load_external_function("$libdir/darmok_server",
																		 "darmok_statement_guard_check_prepared", true, NULL);
		/* Publish only a complete symbol set; a failed native lookup remains a
		 * repeatable explicit error, never a partially initialized callback. */
		guard_release = release;
		guard_owned = owned;
		guard_check_prepared = check_prepared;
		guard_acquire = acquire;
	}
}

static LOCKTAG
probe_tag(void)
{
	LOCKTAG tag;

	SET_LOCKTAG_OBJECT(tag, InvalidOid, ExtensionRelationId, InvalidOid, 0x444d);
	return tag;
}

static void
statement_command(const char *value)
{
	resolve_guard();
	if (owner != NULL)
		elog(ERROR, "native guard probe cannot retain the raw test fence");
	if (strcmp(value, "guard_hold") == 0)
	{
		before_snapshot = FirstSnapshotSet;
		if (guard_acquire(&guard_token))
		{
			guard_outcome = "acquired";
			guard_subid = GetCurrentSubTransactionId();
		}
		else
			guard_outcome = "retry";
		after_snapshot = FirstSnapshotSet;
	}
	else if (strcmp(value, "guard_release") == 0)
	{
		guard_release(&guard_token);
		guard_subid = InvalidSubTransactionId;
		guard_outcome = "released";
	}
	else if (strcmp(value, "guard_copy") == 0)
	{
		if (!guard_owned(&guard_token))
			elog(ERROR, "native guard probe cannot copy an unowned token");
		stale_token.identity = guard_token.identity;
	}
	else if (strcmp(value, "guard_stale_release") == 0)
		guard_release(&stale_token);
	else if (strcmp(value, "guard_check_prepared") == 0)
	{
		before_snapshot = FirstSnapshotSet;
		guard_check_prepared();
		after_snapshot = FirstSnapshotSet;
		guard_outcome = "covered";
	}
	else if (strcmp(value, "guard_scoped") == 0 || strcmp(value, "guard_scoped_error") == 0)
	{
		volatile DarmokStatementGuard scoped = {0};

		before_snapshot = FirstSnapshotSet;
		PG_TRY();
		{
			guard_outcome = guard_acquire(&scoped) ? "scoped" : "retry";
			after_snapshot = FirstSnapshotSet;
			if (strcmp(value, "guard_scoped_error") == 0)
				elog(ERROR, "ordinary native scoped guard fixture error");
		}
		PG_FINALLY();
		{
			if (guard_owned(&scoped))
				guard_release(&scoped);
		}
		PG_END_TRY();
	}
	else
		elog(ERROR, "unknown native guard probe command");
}

static void
show_guard(DestReceiver *dest, QueryCompletion *completion)
{
	TupOutputState *output;
	char *text;
	bool snapshot = FirstSnapshotSet;

	resolve_guard();
	text = psprintf("{\"owned\":%s,\"first_snapshot\":%s,\"before_snapshot\":%s,\"after_snapshot\":%s,\"outcome\":\"%s\"}",
					guard_owned(&guard_token) ? "true" : "false",
					snapshot ? "true" : "false", before_snapshot ? "true" : "false",
					after_snapshot ? "true" : "false", guard_outcome);
	output = begin_tup_output_tupdesc(dest,
									  GetPGVariableResultDesc("darmok_catalog_probe.guard_status"),
									  &TTSOpsVirtual);
	do_text_output_oneline(output, text);
	end_tup_output(output);
	pfree(text);
	if (FirstSnapshotSet != snapshot)
		elog(ERROR, "native guard probe SHOW changed first data snapshot");
	SetQueryCompletion(completion, CMDTAG_SHOW, 0);
}

static void
probe_command(const char *value)
{
	LOCKTAG tag = probe_tag();
	ResourceOwner saved = CurrentResourceOwner;

	if (!IsTransactionBlock())
		ereport(ERROR, (errcode(ERRCODE_NO_ACTIVE_SQL_TRANSACTION),
						errmsg("native test probe requires an explicit transaction")));
	if (strncmp(value, "storage_", 8) == 0)
	{
		darmok_heap_storage_probe_command(value);
		return;
	}
	if (strncmp(value, "guard_", 6) == 0)
	{
		statement_command(value);
		return;
	}
	if (strncmp(value, "relation_", 9) == 0)
	{
		relation_command(value);
		return;
	}
	if (strcmp(value, "omit_prepare_coverage") == 0)
	{
		/* Synthetic representation fixture only. The explicitly ordered
		 * preload puts this callback after the product PRE_PREPARE callback. */
		omit_prepare_coverage = true;
		omit_subid = GetCurrentSubTransactionId();
		return;
	}
	if (guard_owned != NULL && guard_owned(&guard_token))
		elog(ERROR, "native raw probe cannot retain a statement guard");
	if (strcmp(value, "hold") == 0)
	{
		if (owner != NULL)
			elog(ERROR, "native test probe already holds Share");
		CurrentResourceOwner = CurTransactionResourceOwner;
		PG_TRY();
		{
			(void) LockAcquire(&tag, ShareLock, false, false);
		}
		PG_FINALLY();
		{
			CurrentResourceOwner = saved;
		}
		PG_END_TRY();
		owner = CurTransactionResourceOwner;
		subid = GetCurrentSubTransactionId();
	}
	else if (strcmp(value, "release") == 0)
	{
		if (owner == NULL)
			elog(ERROR, "native test probe has no Share to release");
		CurrentResourceOwner = owner;
		PG_TRY();
		{
			if (!LockRelease(&tag, ShareLock, false))
				elog(ERROR, "native test probe lost its Share lock");
		}
		PG_FINALLY();
		{
			CurrentResourceOwner = saved;
		}
		PG_END_TRY();
		owner = NULL;
		subid = InvalidSubTransactionId;
	}
	else
		elog(ERROR, "unknown native test probe command");
}

static void
process_utility(PlannedStmt *pstmt, const char *query, bool read_only_tree,
				ProcessUtilityContext context, ParamListInfo params,
				QueryEnvironment *query_env, DestReceiver *dest,
				QueryCompletion *completion)
{
	if (IsA(pstmt->utilityStmt, VariableShowStmt) &&
		darmok_heap_storage_probe_show(((VariableShowStmt *) pstmt->utilityStmt)->name,
									 dest, completion))
		return;
	if (IsA(pstmt->utilityStmt, VariableShowStmt) &&
		strcmp(((VariableShowStmt *) pstmt->utilityStmt)->name,
			   "darmok_catalog_probe.guard_status") == 0)
	{
		show_guard(dest, completion);
		return;
	}
	if (IsA(pstmt->utilityStmt, VariableShowStmt) &&
		strcmp(((VariableShowStmt *) pstmt->utilityStmt)->name,
			   "darmok_catalog_probe.relation_status") == 0)
	{
		show_relation(dest, completion);
		return;
	}
	if (IsA(pstmt->utilityStmt, VariableShowStmt) &&
		strcmp(((VariableShowStmt *) pstmt->utilityStmt)->name,
			   "darmok_catalog_probe.module_footprint") == 0)
	{
		show_modules(dest, completion);
		return;
	}
	if (IsA(pstmt->utilityStmt, VariableShowStmt) &&
		strcmp(((VariableShowStmt *) pstmt->utilityStmt)->name,
			   "darmok_catalog_probe.builtin_dispatch") == 0)
	{
		show_builtin_dispatch(dest, completion);
		return;
	}
	if (owner != NULL && IsA(pstmt->utilityStmt, TransactionStmt) &&
		((TransactionStmt *) pstmt->utilityStmt)->kind == TRANS_STMT_PREPARE)
		elog(ERROR, "native test probe must release Share before PREPARE");
	if (IsA(pstmt->utilityStmt, VariableSetStmt))
	{
		VariableSetStmt *setting = (VariableSetStmt *) pstmt->utilityStmt;

		if (setting->name != NULL && strcmp(setting->name, "darmok_catalog_probe.command") == 0)
		{
			A_Const *argument;

			if (context != PROCESS_UTILITY_TOPLEVEL || setting->kind != VAR_SET_VALUE ||
				list_length(setting->args) != 1 || !IsA(linitial(setting->args), A_Const))
				elog(ERROR, "native test probe requires one top-level literal command");
			argument = linitial_node(A_Const, setting->args);
			if (argument->isnull || !IsA(&argument->val, String))
				elog(ERROR, "native test probe command must be a string");
			probe_command(strVal(&argument->val));
			SetQueryCompletion(completion, CMDTAG_SET, 0);
			return;
		}
	}
	if (previous_utility)
		previous_utility(pstmt, query, read_only_tree, context, params, query_env, dest, completion);
	else
		standard_ProcessUtility(pstmt, query, read_only_tree, context, params, query_env, dest, completion);
}

static void
transaction_event(XactEvent event, void *arg)
{
	(void) arg;
	if (event == XACT_EVENT_PRE_PREPARE && omit_prepare_coverage)
	{
		LOCKTAG tag;
		ResourceOwner saved = CurrentResourceOwner;

		SET_LOCKTAG_OBJECT(tag, InvalidOid, ExtensionRelationId, InvalidOid, 0x444f);
		CurrentResourceOwner = CurTransactionResourceOwner;
		PG_TRY();
		{
			if (!LockRelease(&tag, AccessShareLock, false))
				elog(ERROR, "coverage fixture requires probe-before-product preload order");
		}
		PG_FINALLY();
		{
			CurrentResourceOwner = saved;
		}
		PG_END_TRY();
	}
	if (event == XACT_EVENT_COMMIT || event == XACT_EVENT_ABORT || event == XACT_EVENT_PREPARE)
	{
		relation_token.identity = 0;
		relation_subid = InvalidSubTransactionId;
		if (borrowed_requests != NULL)
			pfree(borrowed_requests);
		borrowed_requests = NULL;
		borrowed_owner = NULL;
		borrowed_subid = InvalidSubTransactionId;
		borrowed_count = 0;
		owner = NULL;
		subid = InvalidSubTransactionId;
		guard_token.identity = 0;
		guard_subid = InvalidSubTransactionId;
		omit_prepare_coverage = false;
		omit_subid = InvalidSubTransactionId;
	}
}

static void
subtransaction_event(SubXactEvent event, SubTransactionId child,
					 SubTransactionId parent, void *arg)
{
	(void) arg;
	if (relation_subid == child)
	{
		if (event == SUBXACT_EVENT_COMMIT_SUB)
			relation_subid = parent;
		else if (event == SUBXACT_EVENT_ABORT_SUB)
		{
			relation_token.identity = 0;
			relation_subid = InvalidSubTransactionId;
		}
	}
	if (borrowed_requests != NULL && borrowed_subid == child)
	{
		if (event == SUBXACT_EVENT_COMMIT_SUB)
		{
			borrowed_subid = parent;
			borrowed_owner = ResourceOwnerGetParent(borrowed_owner);
		}
		else if (event == SUBXACT_EVENT_ABORT_SUB)
		{
			pfree(borrowed_requests);
			borrowed_requests = NULL;
			borrowed_owner = NULL;
			borrowed_count = 0;
			borrowed_subid = InvalidSubTransactionId;
		}
	}
	if (guard_subid == child)
	{
		if (event == SUBXACT_EVENT_COMMIT_SUB)
			guard_subid = parent;
		else if (event == SUBXACT_EVENT_ABORT_SUB)
		{
			guard_token.identity = 0;
			guard_subid = InvalidSubTransactionId;
		}
	}
	if (omit_prepare_coverage && omit_subid == child)
	{
		if (event == SUBXACT_EVENT_COMMIT_SUB)
			omit_subid = parent;
		else if (event == SUBXACT_EVENT_ABORT_SUB)
		{
			omit_prepare_coverage = false;
			omit_subid = InvalidSubTransactionId;
		}
	}
	if (owner != NULL && subid == child)
	{
		if (event == SUBXACT_EVENT_COMMIT_SUB)
		{
			subid = parent;
			owner = ResourceOwnerGetParent(owner);
		}
		else if (event == SUBXACT_EVENT_ABORT_SUB)
		{
			owner = NULL;
			subid = InvalidSubTransactionId;
		}
	}
}

void
_PG_init(void)
{
	DefineCustomStringVariable("darmok_catalog_probe.command", "Native test control only.",
							   NULL, &command, "idle", PGC_USERSET, GUC_NOT_IN_SAMPLE,
							   NULL, NULL, NULL);
	DefineCustomStringVariable("darmok_catalog_probe.guard_status", "Native test state only.",
							   NULL, &guard_status, "native test state", PGC_USERSET, GUC_NOT_IN_SAMPLE,
							   NULL, NULL, NULL);
	DefineCustomStringVariable("darmok_catalog_probe.relation_status", "Native test state only.",
							   NULL, &relation_status, "native test state", PGC_USERSET, GUC_NOT_IN_SAMPLE,
							   NULL, NULL, NULL);
	DefineCustomStringVariable("darmok_catalog_probe.module_footprint", "Native test observation only.",
							   NULL, &module_footprint_status, "native test observation", PGC_INTERNAL, GUC_NOT_IN_SAMPLE,
							   NULL, NULL, NULL);
	DefineCustomStringVariable("darmok_catalog_probe.builtin_dispatch", "Native test observation only.",
							   NULL, &builtin_dispatch_status, "native test observation", PGC_INTERNAL, GUC_NOT_IN_SAMPLE,
							   NULL, NULL, NULL);
	DefineCustomBoolVariable("darmok_catalog_probe.shared_drop_barrier", "Native test synchronization only.",
							 NULL, &shared_drop_barrier, false, PGC_USERSET, GUC_NOT_IN_SAMPLE,
							 NULL, NULL, NULL);
	darmok_heap_storage_probe_define_guc();
	MarkGUCPrefixReserved("darmok_catalog_probe");
	previous_utility = ProcessUtility_hook;
	ProcessUtility_hook = process_utility;
	previous_object_access = object_access_hook;
	object_access_hook = object_access;
	RegisterXactCallback(transaction_event, NULL);
	RegisterSubXactCallback(subtransaction_event, NULL);
	CacheRegisterRelcacheCallback(observe_relcache, (Datum) 0);
}
