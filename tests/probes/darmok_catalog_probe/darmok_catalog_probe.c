/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
/* Test control only. This module is never part of the product artifact. */
#include "postgres.h"

#include "access/xact.h"
#include "catalog/pg_extension_d.h"
#include "miscadmin.h"
#include "storage/lock.h"
#include "tcop/utility.h"
#include "utils/guc.h"
#include "utils/resowner.h"

PG_MODULE_MAGIC;
PGDLLEXPORT void _PG_init(void);

static ProcessUtility_hook_type previous_utility = NULL;
static ResourceOwner owner = NULL;
static SubTransactionId subid = InvalidSubTransactionId;
static char *command = NULL;

static LOCKTAG
probe_tag(void)
{
	LOCKTAG tag;

	SET_LOCKTAG_OBJECT(tag, InvalidOid, ExtensionRelationId, InvalidOid, 0x444d);
	return tag;
}

static void
probe_command(const char *value)
{
	LOCKTAG tag = probe_tag();
	ResourceOwner saved = CurrentResourceOwner;

	if (!IsTransactionBlock())
		ereport(ERROR, (errcode(ERRCODE_NO_ACTIVE_SQL_TRANSACTION),
						errmsg("native test probe requires an explicit transaction")));
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
	if (event == XACT_EVENT_COMMIT || event == XACT_EVENT_ABORT || event == XACT_EVENT_PREPARE)
	{
		owner = NULL;
		subid = InvalidSubTransactionId;
	}
}

static void
subtransaction_event(SubXactEvent event, SubTransactionId child,
					 SubTransactionId parent, void *arg)
{
	(void) arg;
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
	MarkGUCPrefixReserved("darmok_catalog_probe");
	previous_utility = ProcessUtility_hook;
	ProcessUtility_hook = process_utility;
	RegisterXactCallback(transaction_event, NULL);
	RegisterSubXactCallback(subtransaction_event, NULL);
}
