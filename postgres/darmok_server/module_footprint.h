/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#ifndef DARMOK_MODULE_FOOTPRINT_H
#define DARMOK_MODULE_FOOTPRINT_H

#include "postgres.h"

#define DARMOK_MODULE_FOOTPRINT_BYTES ((Size) 1024 * 1024)
#define DARMOK_MODULE_FOOTPRINT_PATHS 4096

typedef struct DarmokModuleFootprint DarmokModuleFootprint;

/* Private observation, outside module exclusion and every relation attempt.
 * The image is owned by a child of CurrentMemoryContext. It expires when that
 * context resets or when released, and contains pathname bytes, not identities
 * of executable contents, callbacks, providers or reference owners. */
extern PGDLLEXPORT DarmokModuleFootprint *darmok_module_footprint_capture(void);
extern PGDLLEXPORT const char *darmok_module_footprint_image(const DarmokModuleFootprint *footprint,
											  Size *bytes);
extern PGDLLEXPORT uint32 darmok_module_footprint_count(const DarmokModuleFootprint *footprint);
/* Requested record+buffer bytes; excludes context/allocator overhead. */
extern PGDLLEXPORT Size darmok_module_footprint_requested_bytes(const DarmokModuleFootprint *footprint);
extern PGDLLEXPORT void darmok_module_footprint_release(DarmokModuleFootprint *footprint);

#endif
