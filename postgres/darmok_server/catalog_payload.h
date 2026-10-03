/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#ifndef DARMOK_CATALOG_PAYLOAD_H
#define DARMOK_CATALOG_PAYLOAD_H

#include "access/htup.h"
#include "utils/relcache.h"
#include "heap_storage.h"

#define DARMOK_CATALOG_PHASE_BYTES ((Size) 64 * 1024 * 1024)

/* Internal memory/carrier mechanism. Caller admits all descriptors before
 * entry, owns their exact physical tags and retains the source observation's
 * registered catalog snapshot throughout external fetching. No raw or semantic
 * reference is held. No provider, expression, index, SQL or data-snapshot path
 * is introduced here. Returned pointers belong to the supplied phase context. */
typedef struct DarmokCatalogCarrier
{
	struct varlena *data;
	Size bytes;
	bool present;
} DarmokCatalogCarrier;

typedef struct DarmokCatalogImageBudget
{
	MemoryContext context;
	Size limit;
	Size requested_bytes;
} DarmokCatalogImageBudget;

typedef struct DarmokCatalogPayloadRequest
{
	const DarmokCatalogCarrier *carrier;
	DarmokHeapPayloadImage *image;
} DarmokCatalogPayloadRequest;

typedef struct DarmokCatalogPayloadCost
{
	uint64 toast_heaps;
	uint64 toast_rows;
	uint64 selected_chunks;
	Size stored_bytes;
	Size image_bytes;
} DarmokCatalogPayloadCost;

extern void *darmok_catalog_image_alloc(DarmokCatalogImageBudget *budget,
	Size bytes, bool zero);
extern Datum darmok_catalog_present_attr(HeapTuple tuple, TupleDesc descriptor,
	AttrNumber number, bool *is_null);
extern void darmok_catalog_carrier_copy(DarmokCatalogImageBudget *budget,
	DarmokCatalogCarrier *carrier, HeapTuple tuple, TupleDesc descriptor,
	AttrNumber number, bool external_allowed);
extern void darmok_catalog_carrier_image(DarmokCatalogImageBudget *budget,
	const DarmokCatalogCarrier *carrier, DarmokHeapPayloadImage *image);
extern Oid darmok_catalog_carrier_toast(const DarmokCatalogCarrier *carrier);
extern void darmok_catalog_payload_images(DarmokCatalogImageBudget *budget,
	const DarmokCatalogPayloadRequest *requests, int count,
	Relation *toast_heaps, int heap_count, DarmokCatalogPayloadCost *cost,
	void (*context_check)(void *), void *context);

#endif
