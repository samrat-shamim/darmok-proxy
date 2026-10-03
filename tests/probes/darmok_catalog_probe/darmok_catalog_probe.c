/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
/* Test control only. This module is never part of the product artifact. */
#include "postgres.h"

#include "access/xact.h"
#include "catalog/pg_extension_d.h"
#include "fmgr.h"
#include "funcapi.h"
#include "miscadmin.h"
#include "storage/lock.h"
#include "tcop/utility.h"
#include "utils/builtins.h"
#include "utils/guc.h"
#include "utils/resowner.h"
#include "utils/snapmgr.h"

#include "statement_guard.h"

PG_MODULE_MAGIC;
PGDLLEXPORT void _PG_init(void);

static ProcessUtility_hook_type previous_utility = NULL;
static ResourceOwner owner = NULL;
static SubTransactionId subid = InvalidSubTransactionId;
static char *command = NULL;
static char *guard_status = NULL;
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
	if (strncmp(value, "guard_", 6) == 0)
	{
		statement_command(value);
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
		strcmp(((VariableShowStmt *) pstmt->utilityStmt)->name,
			   "darmok_catalog_probe.guard_status") == 0)
	{
		show_guard(dest, completion);
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
	MarkGUCPrefixReserved("darmok_catalog_probe");
	previous_utility = ProcessUtility_hook;
	ProcessUtility_hook = process_utility;
	RegisterXactCallback(transaction_event, NULL);
	RegisterSubXactCallback(subtransaction_event, NULL);
}
