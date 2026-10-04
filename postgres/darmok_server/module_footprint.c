/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#include "postgres.h"

#include "fmgr.h"
#include "miscadmin.h"
#include "utils/memutils.h"

#include "catalog_read.h"
#include "module_footprint.h"

#if PG_VERSION_NUM < 170000 || PG_VERSION_NUM >= 190000
#error "module footprint observation requires PostgreSQL 17 or 18"
#endif

struct DarmokModuleFootprint
{
	MemoryContext context;
	Size bytes;
	Size requested_bytes;
	uint32 count;
	char image[FLEXIBLE_ARRAY_MEMBER];
};

static void
invalid_image(void)
{
	ereport(ERROR,
			(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
			 errmsg("native module pathname image is not complete")));
}

DarmokModuleFootprint *
darmok_module_footprint_capture(void)
{
	MemoryContext scratch;
	DarmokModuleFootprint *volatile result = NULL;

	darmok_native_invocation_check();
	if (!IsNormalProcessingMode())
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("native module observation requires normal backend processing")));
	scratch = AllocSetContextCreate(CurrentMemoryContext,
									  "Darmok module footprint", ALLOCSET_DEFAULT_SIZES);
	PG_TRY();
	{
		Size capacity = EstimateLibraryStateSpace();
		Size requested;
		Size bytes;
		Size cursor = 0;
		uint32 count = 0;

		if (capacity == 0 || capacity > DARMOK_MODULE_FOOTPRINT_BYTES)
			ereport(ERROR,
					(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
					 errmsg("native module pathname image exceeds its 1 MiB limit")));
		requested = add_size(sizeof(*result), capacity);
		result = MemoryContextAlloc(scratch, requested);

		/* Allocation precedes the final observation. Between this estimator and
		 * serialization there is no allocation, loader, interrupt processing or
		 * callback call. Native backend execution keeps the list unchanged in
		 * that window; no private DynamicFileList layout is inspected. Growth
		 * beyond capacity fails before the Assert-only serializer can overrun. */
		bytes = EstimateLibraryStateSpace();
		if (bytes == 0 || bytes > capacity)
			ereport(ERROR,
					(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
					 errmsg("native module pathname image outgrew its allocated buffer")));
		SerializeLibraryState(bytes, result->image);

		/* Each nonempty pathname has one terminator, followed by the final NUL.
		 * Preserve raw bytes and native order; never deduplicate or decode them
		 * as SQL/server-encoded text. An empty list is exactly one NUL byte. */
		while (cursor < bytes - 1)
		{
			const char *end = memchr(result->image + cursor, '\0', bytes - cursor);

			if (end == NULL || end == result->image + cursor)
				invalid_image();
			cursor = (Size) (end - result->image) + 1;
			if (++count > DARMOK_MODULE_FOOTPRINT_PATHS)
				ereport(ERROR,
						(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
						 errmsg("native module pathname image exceeds 4096 paths")));
		}
		if (cursor != bytes - 1 || result->image[cursor] != '\0')
			invalid_image();
		result->context = scratch;
		result->bytes = bytes;
		result->requested_bytes = requested;
		result->count = count;
	}
	PG_CATCH();
	{
		MemoryContextDelete(scratch);
		PG_RE_THROW();
	}
	PG_END_TRY();
	return result;
}

const char *
darmok_module_footprint_image(const DarmokModuleFootprint *footprint, Size *bytes)
{
	*bytes = footprint->bytes;
	return footprint->image;
}

uint32
darmok_module_footprint_count(const DarmokModuleFootprint *footprint)
{
	return footprint->count;
}

Size
darmok_module_footprint_requested_bytes(const DarmokModuleFootprint *footprint)
{
	return footprint->requested_bytes;
}

void
darmok_module_footprint_release(DarmokModuleFootprint *footprint)
{
	MemoryContextDelete(footprint->context);
}
