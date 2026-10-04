/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
/* Test-only source-admitted pure copying consumer. Never in product images. */
#include "postgres.h"
#include "access/detoast.h"
#include "access/heapam.h"
#include "access/htup_details.h"
#include "access/table.h"
#include "access/tableam.h"
#include "access/toast_compression.h"
#include "catalog/pg_type.h"
#include "fmgr.h"
#include "funcapi.h"
#include "lib/stringinfo.h"
#include "utils/builtins.h"
#include "utils/array.h"
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

extern PGDLLEXPORT Datum darmok_test_missing_array_image(PG_FUNCTION_ARGS);
extern PGDLLEXPORT Datum darmok_test_heap_natts(PG_FUNCTION_ARGS);
extern PGDLLEXPORT Datum darmok_test_varlena_image(PG_FUNCTION_ARGS);
extern PGDLLEXPORT Datum darmok_test_varlena_carrier(PG_FUNCTION_ARGS);
PG_FUNCTION_INFO_V1(darmok_test_missing_array_image);
PG_FUNCTION_INFO_V1(darmok_test_heap_natts);
PG_FUNCTION_INFO_V1(darmok_test_varlena_image);
PG_FUNCTION_INFO_V1(darmok_test_varlena_carrier);

/* Independent ordinary SQL oracles use PostgreSQL's own detoaster and stored
 * compression introspection. They are never called by the private consumer. */
Datum
darmok_test_varlena_image(PG_FUNCTION_ARGS)
{
	struct varlena *image = PG_GETARG_VARLENA_P(0);
	Size bytes = VARSIZE(image);
	bytea *result;

	if (bytes > MaxAllocSize - VARHDRSZ)
		elog(ERROR, "native varlena image oracle exceeds native allocation limits");
	result = palloc(bytes + VARHDRSZ);
	SET_VARSIZE(result, bytes + VARHDRSZ);
	memcpy(VARDATA(result), image, bytes);
	PG_FREE_IF_COPY(image, 0);
	PG_RETURN_BYTEA_P(result);
}

Datum
darmok_test_varlena_carrier(PG_FUNCTION_ARGS)
{
	struct varlena *value = (struct varlena *) PG_GETARG_POINTER(0);
	ToastCompressionId compression = toast_get_compression_id(value);
	Oid toast_oid = InvalidOid;
	Oid value_oid = InvalidOid;
	Size stored = VARSIZE_ANY(value);
	char kind;
	char method = compression == TOAST_INVALID_COMPRESSION_ID ? 0 :
		(compression == TOAST_PGLZ_COMPRESSION_ID ? 'p' : 'l');
	char *json;

	if (VARATT_IS_EXTERNAL_ONDISK(value))
	{
		struct varatt_external pointer;

		VARATT_EXTERNAL_GET_POINTER(pointer, value);
		toast_oid = pointer.va_toastrelid;
		value_oid = pointer.va_valueid;
		stored = VARATT_EXTERNAL_GET_EXTSIZE(pointer);
		kind = 'e';
	}
	else if (VARATT_IS_EXTERNAL(value))
		elog(ERROR, "native fixture oracle encountered a non-disk external carrier");
	else if (VARATT_IS_SHORT(value))
		kind = 's';
	else if (VARATT_IS_COMPRESSED(value))
		kind = method;
	else
		kind = 'u';
	json = psprintf("{\"present\":true,\"carrier_kind\":\"%c\",\"compression_kind\":%u,"
		"\"toast_oid\":%u,\"value_oid\":%u,\"carrier_bytes\":" UINT64_FORMAT
		",\"stored_bytes\":" UINT64_FORMAT "}", kind, (unsigned char) method,
		toast_oid, value_oid, (uint64) VARSIZE_ANY(value), (uint64) stored);
	PG_RETURN_TEXT_P(cstring_to_text(json));
}

/* Ordinary SQL oracles, outside the private invocation. Core normalizes the
 * actual array; this function does not share the product carrier/envelope code.
 * These symbols and their fixture-created SQL functions exist in test images
 * only. No element provider or application codec is implemented here. */
Datum
darmok_test_missing_array_image(PG_FUNCTION_ARGS)
{
	ArrayType *array = PG_GETARG_ARRAYTYPE_P(0);
	Size bytes = VARSIZE(array);
	bytea *result;

	if (bytes > MaxAllocSize - VARHDRSZ)
		elog(ERROR, "native missing image oracle exceeds native allocation limits");
	result = palloc(bytes + VARHDRSZ);
	SET_VARSIZE(result, bytes + VARHDRSZ);
	memcpy(VARDATA(result), array, bytes);
	PG_FREE_IF_COPY(array, 0);
	PG_RETURN_BYTEA_P(result);
}

Datum
darmok_test_heap_natts(PG_FUNCTION_ARGS)
{
	Relation relation = table_open(PG_GETARG_OID(0), AccessShareLock);
	volatile TableScanDesc scan = NULL;
	volatile Datum result = (Datum) 0;

	PG_TRY();
	{
		Datum values[16];
		int count = 0;
		HeapTuple tuple;

		scan = heap_beginscan(relation, GetActiveSnapshot(), 0, NULL, NULL,
							 SO_TYPE_SEQSCAN | SO_ALLOW_PAGEMODE);
		while ((tuple = heap_getnext(scan, ForwardScanDirection)) != NULL)
		{
			if (count == (int) lengthof(values))
				elog(ERROR, "native physical-ordinal oracle requires at most 16 fixture rows");
			values[count++] = Int32GetDatum(HeapTupleHeaderGetNatts(tuple->t_data));
		}
		result = PointerGetDatum(construct_array(values, count, INT4OID, sizeof(int32),
												true, TYPALIGN_INT));
	}
	PG_FINALLY();
	{
		if (scan != NULL)
			heap_endscan(scan);
		table_close(relation, AccessShareLock);
	}
	PG_END_TRY();
	PG_RETURN_DATUM(result);
}

static void
copy_image_hex(StringInfo output, const char *image, Size bytes)
{
	static const char digits[] = "0123456789abcdef";
	char *dest;

	if (bytes > (Size) (MaxAllocSize - 1) / 2)
		elog(ERROR, "native storage probe image exceeds its copying limit");
	enlargeStringInfo(output, bytes * 2);
	dest = output->data + output->len;
	for (Size i = 0; i < bytes; i++)
	{
		unsigned char value = (unsigned char) image[i];

		*dest++ = digits[value >> 4];
		*dest++ = digits[value & 15];
	}
	output->len += bytes * 2;
	output->data[output->len] = '\0';
}

static void
copy_name_json(StringInfo output, const NameData *name)
{
	/* Pinned builtin byte escaping only: no SQL type/output/provider dispatch. */
	escape_json(output, NameStr(*name));
}

static void
copy_attribute_json(StringInfo output, const DarmokHeapAttributeFact *fact)
{
	appendStringInfo(output, "{\"relation_oid\":%u,\"number\":%d,\"name\":",
					 fact->relation_oid, fact->number);
	copy_name_json(output, &fact->name);
	appendStringInfo(output, ",\"type_oid\":%u,\"length\":%d,\"typmod\":%d,\"dimensions\":%d,"
					 "\"by_value\":%s,\"alignment\":%u,\"storage\":%u,\"compression\":%u,"
					 "\"not_null_declared\":%s,\"has_default\":%s,\"has_missing\":%s,"
					 "\"identity\":%u,\"generated\":%u,\"dropped\":%s,\"local\":%s,"
					 "\"inheritance_count\":%d,\"collation_oid\":%u}",
					 fact->type_oid, fact->length, fact->typmod, fact->dimensions,
					 fact->by_value ? "true" : "false", (unsigned char) fact->alignment,
					 (unsigned char) fact->storage, (unsigned char) fact->compression,
					 fact->not_null_declared ? "true" : "false",
					 fact->has_default ? "true" : "false", fact->has_missing ? "true" : "false",
					 (unsigned char) fact->identity, (unsigned char) fact->generated,
					 fact->dropped ? "true" : "false", fact->local ? "true" : "false",
					 fact->inheritance_count, fact->collation_oid);
}

static void
copy_composite_json(StringInfo output, const DarmokHeapCompositeFact *fact)
{
	appendStringInfo(output, "{\"type_oid\":%u,\"relation_oid\":%u,\"schema_oid\":%u,\"schema\":",
					 fact->type_oid, fact->relation_oid, fact->schema_oid);
	copy_name_json(output, &fact->schema_name);
	appendStringInfoString(output, ",\"name\":");
	copy_name_json(output, &fact->name);
	appendStringInfo(output, ",\"kind\":%u,\"persistence\":%u,\"am\":%u,\"is_partition\":%s,"
					 "\"declared_attribute_count\":%d,\"attribute_offset\":%d}",
					 (unsigned char) fact->kind, (unsigned char) fact->persistence,
					 fact->access_method_oid, fact->is_partition ? "true" : "false",
					 fact->declared_attribute_count, fact->attribute_offset);
}

static void
copy_type_json(StringInfo output, const DarmokHeapTypeFact *fact)
{
	appendStringInfo(output, "{\"oid\":%u,\"schema_oid\":%u,\"schema\":",
					 fact->oid, fact->schema_oid);
	copy_name_json(output, &fact->schema_name);
	appendStringInfoString(output, ",\"name\":");
	copy_name_json(output, &fact->name);
	appendStringInfo(output, ",\"length\":%d,\"by_value\":%s,\"kind\":%u,\"category\":%u,"
					 "\"preferred\":%s,\"defined\":%s,\"delimiter\":%u,\"relation_oid\":%u,"
					 "\"subscript_oid\":%u,\"element_oid\":%u,\"array_oid\":%u,"
					 "\"input_oid\":%u,\"output_oid\":%u,\"receive_oid\":%u,\"send_oid\":%u,"
					 "\"typmod_input_oid\":%u,\"typmod_output_oid\":%u,\"analyze_oid\":%u,"
					 "\"alignment\":%u,\"storage\":%u,\"not_null_declared\":%s,"
					 "\"base_type_oid\":%u,\"typmod\":%d,\"dimensions\":%d,\"collation_oid\":%u}",
					 fact->length, fact->by_value ? "true" : "false",
					 (unsigned char) fact->kind, (unsigned char) fact->category,
					 fact->preferred ? "true" : "false", fact->defined ? "true" : "false",
					 (unsigned char) fact->delimiter, fact->relation_oid,
					 fact->subscript_oid, fact->element_oid, fact->array_oid,
					 fact->input_oid, fact->output_oid, fact->receive_oid, fact->send_oid,
					 fact->typmod_input_oid, fact->typmod_output_oid, fact->analyze_oid,
					 (unsigned char) fact->alignment, (unsigned char) fact->storage,
					 fact->not_null_declared ? "true" : "false", fact->base_type_oid,
					 fact->typmod, fact->dimensions, fact->collation_oid);
}

static void
copy_cost_json(StringInfo output, const DarmokHeapObservationCost *cost)
{
	appendStringInfo(output, "{\"namespace_rows\":" UINT64_FORMAT
					 ",\"relation_rows\":" UINT64_FORMAT ",\"index_rows\":" UINT64_FORMAT
					 ",\"attribute_rows\":" UINT64_FORMAT
					 ",\"attribute_payload_rows\":" UINT64_FORMAT ",\"type_rows\":" UINT64_FORMAT
					 ",\"type_payload_rows\":" UINT64_FORMAT
					 ",\"attrdef_rows\":" UINT64_FORMAT
					 ",\"options_rows\":" UINT64_FORMAT
					 ",\"missing_carrier_bytes\":" UINT64_FORMAT
					 ",\"payload_carrier_bytes\":" UINT64_FORMAT
					 ",\"requested_copy_bytes\":" UINT64_FORMAT
					 ",\"allocated_bytes\":" UINT64_FORMAT "}",
					 cost->namespace_rows, cost->relation_rows, cost->index_rows,
					 cost->attribute_rows, cost->attribute_payload_rows,
					 cost->type_rows, cost->type_payload_rows, cost->attrdef_rows,
					 cost->options_rows,
					 (uint64) cost->missing_carrier_bytes, (uint64) cost->payload_carrier_bytes,
					 (uint64) cost->requested_copy_bytes, (uint64) cost->allocated_bytes);
}

static void
copy_payload_json(StringInfo output, const DarmokHeapCatalogPayloadFact *fact)
{
	const DarmokHeapPayloadImage *value = &fact->value;
	char kind[2] = {value->carrier_kind, 0};

	appendStringInfo(output, "{\"catalog_oid\":%u,\"row_oid\":%u,\"relation_oid\":%u,"
		"\"number\":%d,\"field_number\":%d,\"value\":{\"present\":%s,\"carrier_kind\":",
		fact->catalog_oid, fact->row_oid, fact->relation_oid, fact->number, fact->field_number,
		value->present ? "true" : "false");
	escape_json(output, kind);
	appendStringInfo(output, ",\"compression_kind\":%u,\"toast_oid\":%u,\"value_oid\":%u,"
		"\"carrier_bytes\":" UINT64_FORMAT ",\"stored_bytes\":" UINT64_FORMAT
		",\"image_bytes\":" UINT64_FORMAT ",\"image\":",
		(unsigned char) value->compression_kind, value->toast_oid, value->value_oid,
		(uint64) value->carrier_bytes, (uint64) value->stored_bytes, (uint64) value->image_bytes);
	if (value->present)
	{
		appendStringInfoChar(output, '"');
		copy_image_hex(output, value->image, value->image_bytes);
		appendStringInfoChar(output, '"');
	}
	else
		appendStringInfoString(output, "null");
	appendStringInfoString(output, "}}");
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
		appendStringInfoString(output, "],\"root_facts\":[");
		for (int i = 0; i < view->root_count; i++)
		{
			const DarmokHeapStorageRootFact *fact = &view->root_facts[i];

			appendStringInfo(output, "%s{\"oid\":%u,\"row_type_oid\":%u,"
							 "\"declared_attribute_count\":%d,\"attribute_offset\":%d}",
							 i == 0 ? "" : ",", fact->oid, fact->row_type_oid,
							 fact->declared_attribute_count, fact->attribute_offset);
		}
		appendStringInfoString(output, "],\"composites\":[");
		for (int i = 0; i < view->composite_count; i++)
		{
			if (i > 0)
				appendStringInfoChar(output, ',');
			copy_composite_json(output, &view->composites[i]);
		}
		appendStringInfoString(output, "],\"attributes\":[");
		for (int i = 0; i < view->attribute_count; i++)
		{
			if (i > 0)
				appendStringInfoChar(output, ',');
			copy_attribute_json(output, &view->attributes[i]);
		}
		appendStringInfoString(output, "],\"types\":[");
		for (int i = 0; i < view->type_count; i++)
		{
			if (i > 0)
				appendStringInfoChar(output, ',');
			copy_type_json(output, &view->types[i]);
		}
		appendStringInfo(output, "],\"missing_count\":%d,\"missing\":[", view->missing_count);
		for (int i = 0; i < view->missing_count; i++)
		{
			const DarmokHeapMissingFact *fact = &view->missing[i];

			if (fact->carrier_kind != 's' && fact->carrier_kind != 'u' &&
				fact->carrier_kind != 'p' && fact->carrier_kind != 'l')
				elog(ERROR, "native storage probe received an unknown carrier kind");
			appendStringInfo(output, "%s{\"relation_oid\":%u,\"number\":%d,\"type_oid\":%u,"
							 "\"carrier_kind\":\"%c\",\"stored_bytes\":" UINT64_FORMAT
							 ",\"image_bytes\":" UINT64_FORMAT ",\"image\":\"",
							 i == 0 ? "" : ",", fact->relation_oid, fact->number,
							 fact->type_oid, fact->carrier_kind, (uint64) fact->stored_bytes,
							 (uint64) fact->image_bytes);
			copy_image_hex(output, fact->image, fact->image_bytes);
			appendStringInfoString(output, "\"}");
		}
		appendStringInfo(output, "],\"payload_count\":%d,\"payloads\":[", view->payload_count);
		for (int i = 0; i < view->payload_count; i++)
		{
			if (i > 0)
				appendStringInfoChar(output, ',');
			copy_payload_json(output, &view->payloads[i]);
		}
		appendStringInfoString(output, "],\"uses\":[");
		for (int i = 0; i < view->use_count; i++)
			appendStringInfo(output, "%s{\"root_oid\":%u,\"relation_oid\":%u,\"mode_mask\":%u,\"catalog\":%s}",
				i == 0 ? "" : ",", view->uses[i].root_oid, view->uses[i].relation_oid,
				view->uses[i].mode_mask, view->uses[i].catalog ? "true" : "false");
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
							 "\"file_number\":%u,\"file_proc_number\":%d,"
							 "\"row_type_oid\":%u,\"declared_attribute_count\":%d,"
							 "\"declared_check_count\":%d,\"rules_hint\":%s,\"triggers_hint\":%s}",
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
							 fact->file_number, fact->file_proc_number,
							 fact->row_type_oid, fact->declared_attribute_count,
							 fact->declared_check_count, fact->rules_hint ? "true" : "false",
							 fact->triggers_hint ? "true" : "false");
		}
		appendStringInfo(output, "],\"attribute_array_bytes\":" UINT64_FORMAT
						 ",\"composite_array_bytes\":" UINT64_FORMAT
						 ",\"type_array_bytes\":" UINT64_FORMAT
						 ",\"missing_array_bytes\":" UINT64_FORMAT
						 ",\"missing_image_bytes\":" UINT64_FORMAT
						 ",\"payload_array_bytes\":" UINT64_FORMAT
						 ",\"payload_image_bytes\":" UINT64_FORMAT
						 ",\"payload_stored_bytes\":" UINT64_FORMAT
						 ",\"toast_heaps\":" UINT64_FORMAT ",\"toast_rows\":" UINT64_FORMAT
						 ",\"selected_chunks\":" UINT64_FORMAT ",\"initial_cost\":",
						 (uint64) view->attribute_array_bytes, (uint64) view->composite_array_bytes,
						 (uint64) view->type_array_bytes,
						 (uint64) view->missing_array_bytes, (uint64) view->missing_image_bytes,
						 (uint64) view->payload_array_bytes, (uint64) view->payload_image_bytes,
						 (uint64) view->payload_stored_bytes, view->toast_heaps, view->toast_rows,
						 view->selected_chunks);
		copy_cost_json(output, &view->initial_cost);
		appendStringInfoString(output, ",\"source_cost\":");
		copy_cost_json(output, &view->source_cost);
		appendStringInfoString(output, ",\"final_cost\":");
		copy_cost_json(output, &view->final_cost);
		appendStringInfoChar(output, '}');
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
