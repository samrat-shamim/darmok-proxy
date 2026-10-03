/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
/* Test-only source-admitted pure copying consumer. Never in product images. */
#include "postgres.h"
#include "fmgr.h"
#include "funcapi.h"
#include "lib/stringinfo.h"
#include "utils/builtins.h"
#include "utils/fmgrprotos.h"
#include "utils/guc.h"
#include "utils/json.h"
#include "utils/jsonb.h"
#include "utils/memutils.h"
#include "utils/numeric.h"
#include "utils/snapmgr.h"

#include "heap_storage.h"
#include "heap_storage_probe.h"

typedef void (*StorageRun)(const DarmokHeapStorageRoot *, int,
						  DarmokHeapStorageConsumer, void *, bool);
typedef struct StorageCopy
{
	MemoryContext context;
	char *body;
	int calls;
} StorageCopy;

static StorageRun storage_run = NULL;
static MemoryContext result_context = NULL;
static char *result_json = NULL;
static char *status_setting = NULL;

static void
copy_name_json(StringInfo output, const NameData *name)
{
	/* Pinned builtin byte escaping only: no SQL type/output/provider dispatch. */
	escape_json(output, NameStr(*name));
}

static void
copy_storage(const DarmokHeapStorageView *view, void *opaque)
{
	StorageCopy *copy = opaque;
	MemoryContext saved = MemoryContextSwitchTo(copy->context);

	PG_TRY();
	{
		StringInfo output = makeStringInfo();

		copy->calls++;
		appendStringInfo(output, "{\"attempts\":%d,\"physical_owned\":%s,\"metadata_owned\":%s,"
						 "\"first_snapshot_set\":%s,\"generation\":" UINT64_FORMAT
						 ",\"local_generation\":" UINT64_FORMAT ",\"roots\":[",
						 view->attempts, view->physical_owned ? "true" : "false",
						 view->metadata_owned ? "true" : "false",
						 view->first_snapshot_set ? "true" : "false",
						 view->generation, view->local_generation);
		for (int i = 0; i < view->root_count; i++)
			appendStringInfo(output, "%s[%u,%d]", i == 0 ? "" : ",",
							 view->roots[i].relation_oid, view->roots[i].lock_mode);
		appendStringInfoString(output, "],\"references\":[");
		for (int i = 0; i < view->reference_count; i++)
			appendStringInfo(output, "%s[%u,%d]", i == 0 ? "" : ",",
							 view->references[i].relation_oid, view->references[i].lock_mode);
		appendStringInfoString(output, "],\"facts\":[");
		for (int i = 0; i < view->fact_count; i++)
		{
			const DarmokHeapStorageFact *fact = &view->facts[i];

			appendStringInfo(output, "%s{\"oid\":%u,\"schema_oid\":%u,\"schema\":",
							 i == 0 ? "" : ",", fact->oid, fact->schema_oid);
			copy_name_json(output, &fact->schema_name);
			appendStringInfoString(output, ",\"name\":");
			copy_name_json(output, &fact->name);
			appendStringInfo(output, ",\"kind\":%u,\"persistence\":%u,\"am\":%u,"
							 "\"toast_oid\":%u,\"parent_oid\":%u,\"mode_mask\":%u,"
							 "\"shared\":%s,\"is_partition\":%s,\"has_indexes\":%s,"
							 "\"has_subclasses\":%s,\"live\":%s,\"ready\":%s,\"valid\":%s,"
							 "\"check_xmin\":%s,\"tablespace_oid\":%u,\"stored_file_number\":%u,"
							 "\"file_tablespace_oid\":%u,\"file_database_oid\":%u,"
							 "\"file_number\":%u,\"file_proc_number\":%d}",
							 (unsigned char) fact->kind, (unsigned char) fact->persistence,
							 fact->access_method_oid, fact->toast_oid, fact->parent_oid,
							 fact->mode_mask, fact->shared ? "true" : "false",
							 fact->is_partition ? "true" : "false",
							 fact->has_indexes ? "true" : "false",
							 fact->has_subclasses ? "true" : "false",
							 fact->index_live ? "true" : "false",
							 fact->index_ready ? "true" : "false",
							 fact->index_valid ? "true" : "false",
							 fact->index_check_xmin ? "true" : "false",
							 fact->tablespace_oid, fact->stored_file_number,
							 fact->file_tablespace_oid, fact->file_database_oid,
							 fact->file_number, fact->file_proc_number);
		}
		appendStringInfoString(output, "]}");
		copy->body = output->data;
	}
	PG_FINALLY();
	{
		MemoryContextSwitchTo(saved);
	}
	PG_END_TRY();
}

static DarmokHeapStorageRoot *
parse_storage(const char *input, int *count)
{
	Jsonb *json = DatumGetJsonbP(DirectFunctionCall1(jsonb_in, CStringGetDatum(input)));
	JsonbIterator *iter;
	JsonbValue value;
	DarmokHeapStorageRoot *roots;

	if (!JB_ROOT_IS_ARRAY(json) || JB_ROOT_IS_SCALAR(json) ||
		JB_ROOT_COUNT(json) > DARMOK_RELATION_REQUEST_LIMIT)
		elog(ERROR, "native storage probe requires a bounded root array");
	*count = JB_ROOT_COUNT(json);
	roots = palloc0(sizeof(DarmokHeapStorageRoot) * *count);
	iter = JsonbIteratorInit(&json->root);
	if (JsonbIteratorNext(&iter, &value, true) != WJB_BEGIN_ARRAY)
		elog(ERROR, "invalid storage probe root array");
	for (int i = 0; i < *count; i++)
	{
		JsonbIterator *pair;

		if (JsonbIteratorNext(&iter, &value, true) != WJB_ELEM ||
			value.type != jbvBinary || !JsonContainerIsArray(value.val.binary.data) ||
			JsonContainerIsScalar(value.val.binary.data) || JsonContainerSize(value.val.binary.data) != 3)
			elog(ERROR, "native storage probe requires schema/name/mode triples");
		pair = JsonbIteratorInit(value.val.binary.data);
		if (JsonbIteratorNext(&pair, &value, true) != WJB_BEGIN_ARRAY ||
			JsonbIteratorNext(&pair, &value, true) != WJB_ELEM || value.type != jbvString)
			elog(ERROR, "invalid storage probe schema");
		roots[i].schema = pnstrdup(value.val.string.val, value.val.string.len);
		roots[i].schema_bytes = value.val.string.len;
		if (JsonbIteratorNext(&pair, &value, true) != WJB_ELEM || value.type != jbvString)
			elog(ERROR, "invalid storage probe name");
		roots[i].name = pnstrdup(value.val.string.val, value.val.string.len);
		roots[i].name_bytes = value.val.string.len;
		if (JsonbIteratorNext(&pair, &value, true) != WJB_ELEM || value.type != jbvNumeric)
			elog(ERROR, "invalid storage probe mode");
		roots[i].lock_mode = DatumGetInt32(DirectFunctionCall1(numeric_int4,
															NumericGetDatum(value.val.numeric)));
		if (!DatumGetBool(DirectFunctionCall2(numeric_eq,
												NumericGetDatum(value.val.numeric),
												DirectFunctionCall1(int4_numeric,
																	Int32GetDatum(roots[i].lock_mode)))))
			elog(ERROR, "native storage probe mode must be an exact integer");
		if (JsonbIteratorNext(&pair, &value, true) != WJB_END_ARRAY)
			elog(ERROR, "invalid storage probe triple end");
	}
	if (JsonbIteratorNext(&iter, &value, true) != WJB_END_ARRAY)
		elog(ERROR, "invalid storage probe array end");
	return roots;
}

void
darmok_heap_storage_probe_command(const char *value)
{
	StorageCopy copy = {0};
	DarmokHeapStorageRoot *roots;
	const char *input;
	bool retain;
	bool before = FirstSnapshotSet;
	int count;

	if (strncmp(value, "storage_release:", 16) == 0)
	{
		input = value + 16;
		retain = false;
	}
	else if (strncmp(value, "storage_retain:", 15) == 0)
	{
		input = value + 15;
		retain = true;
	}
	else
		elog(ERROR, "unknown storage probe command");
	if (result_context != NULL)
		MemoryContextDelete(result_context);
	result_json = NULL;
	result_context = AllocSetContextCreate(TopMemoryContext, "storage probe copied facts",
										  ALLOCSET_DEFAULT_SIZES);
	copy.context = result_context;
	roots = parse_storage(input, &count);
	if (storage_run == NULL)
		storage_run = (StorageRun) load_external_function("$libdir/darmok_server",
														 "darmok_heap_storage_metadata", true, NULL);
	storage_run(roots, count, copy_storage, &copy, retain);
	if (copy.calls != 1 || copy.body == NULL)
		elog(ERROR, "storage probe did not receive exactly one completed fact view");
	{
		MemoryContext saved = MemoryContextSwitchTo(result_context);

		result_json = psprintf("{\"calls\":%d,\"before_snapshot\":%s,\"after_snapshot\":%s,"
							   "\"retained\":%s,\"metadata\":%s}",
							   copy.calls, before ? "true" : "false",
							   FirstSnapshotSet ? "true" : "false",
							   retain ? "true" : "false", copy.body);
		MemoryContextSwitchTo(saved);
	}
}

bool
darmok_heap_storage_probe_show(const char *name, DestReceiver *dest, QueryCompletion *completion)
{
	TupOutputState *output;

	if (strcmp(name, "darmok_catalog_probe.storage_status") != 0)
		return false;
	if (result_json == NULL)
		elog(ERROR, "native storage probe has no completed copied view");
	output = begin_tup_output_tupdesc(dest, GetPGVariableResultDesc(name), &TTSOpsVirtual);
	do_text_output_oneline(output, result_json);
	end_tup_output(output);
	SetQueryCompletion(completion, CMDTAG_SHOW, 0);
	return true;
}

void
darmok_heap_storage_probe_define_guc(void)
{
	DefineCustomStringVariable("darmok_catalog_probe.storage_status", "Copied native test state only.",
							   NULL, &status_setting, "native test state", PGC_USERSET,
							   GUC_NOT_IN_SAMPLE, NULL, NULL, NULL);
}
