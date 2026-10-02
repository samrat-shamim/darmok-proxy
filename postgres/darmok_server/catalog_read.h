/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#ifndef DARMOK_CATALOG_READ_H
#define DARMOK_CATALOG_READ_H

#include "tcop/utility.h"

#define DARMOK_CATALOG_REQUEST "darmok_server.catalog_request_v1"

typedef struct DarmokCatalogStamp
{
	unsigned char cluster_id[16];
	Oid database_oid;
	uint64 backend_id;
	uint64 generation;
	uint64 local_generation;
} DarmokCatalogStamp;

/* Module-internal operations. None is a SQL function or a frontend handle. */
extern void darmok_catalog_reader_start(void);
extern void darmok_catalog_fence_acquire(DarmokCatalogStamp *stamp);
extern void darmok_catalog_fence_release(void);
extern bool darmok_catalog_stamp_equal(const DarmokCatalogStamp *left,
									  const DarmokCatalogStamp *right);
extern void darmok_catalog_reader_finish(void);
extern void darmok_catalog_define_guc(void);
extern void darmok_catalog_show(ProcessUtilityContext context, DestReceiver *dest,
							   QueryCompletion *completion);

#endif
