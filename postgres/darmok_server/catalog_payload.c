/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#include "postgres.h"

#include "access/heapam.h"
#include "access/htup_details.h"
#include "access/toast_compression.h"
#include "access/toast_internals.h"
#include "access/heaptoast.h"
#include "utils/hsearch.h"
#include "utils/memutils.h"
#include "utils/snapmgr.h"

#include "catalog_payload.h"

typedef struct PayloadGroup
{
	uint64 key;
	struct varatt_external pointer;
	struct varlena *stored;
	bool *seen;
	Size chunks;
	Size copied;
	DarmokHeapPayloadImage image;
} PayloadGroup;

static void
payload_reserve(DarmokCatalogImageBudget *budget, Size bytes)
{
	if (bytes > MaxAllocSize || bytes > budget->limit ||
		budget->requested_bytes > budget->limit - bytes ||
		MemoryContextMemAllocated(budget->context, true) > budget->limit - bytes)
		ereport(ERROR,
				(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
				 errmsg("native catalog images exceed the 64 MiB phase budget")));
	budget->requested_bytes += bytes;
}

void *
darmok_catalog_image_alloc(DarmokCatalogImageBudget *budget, Size bytes, bool zero)
{
	payload_reserve(budget, bytes);
	return zero ? MemoryContextAllocZero(budget->context, bytes)
		: MemoryContextAlloc(budget->context, bytes);
}

Datum
darmok_catalog_present_attr(HeapTuple tuple, TupleDesc descriptor,
							AttrNumber number, bool *is_null)
{
	HeapTupleHeader header = tuple->t_data;
	int natts;
	Size minimum;

	if (tuple->t_len < SizeofHeapTupleHeader)
		elog(ERROR, "native catalog tuple has a truncated header");
	natts = HeapTupleHeaderGetNatts(header);
	minimum = SizeofHeapTupleHeader;
	if (HeapTupleHasNulls(tuple))
		minimum += BITMAPLEN(natts);
	if (number <= 0 || number > descriptor->natts || natts < number ||
		natts > descriptor->natts || header->t_hoff < MAXALIGN(minimum) ||
		header->t_hoff > tuple->t_len || header->t_hoff != MAXALIGN(header->t_hoff))
		elog(ERROR, "native catalog tuple does not physically contain its compiled ordinal");
	/* Admitted native layout walk only. Physical presence excludes the
	 * heap_getattr/getmissingattr path, including for a NULL field. */
	return fastgetattr(tuple, number, descriptor, is_null);
}

static Size
payload_tuple_carrier(HeapTuple tuple, const char *data, bool external_allowed)
{
	uintptr_t base = (uintptr_t) tuple->t_data;
	uintptr_t address = (uintptr_t) data;
	Size offset;
	Size available;
	Size bytes;
	uint32 aligned_header;

	if (address < base)
		elog(ERROR, "native catalog carrier is outside its tuple");
	offset = address - base;
	if (offset < tuple->t_data->t_hoff || offset >= tuple->t_len)
		elog(ERROR, "native catalog carrier is outside its tuple data");
	available = tuple->t_len - offset;
	/* First-byte classification precedes every larger header or tag read. */
	if (VARATT_IS_EXTERNAL(data))
	{
		if (available < VARHDRSZ_EXTERNAL)
			elog(ERROR, "native catalog carrier has a truncated external header");
		if (!external_allowed || VARTAG_EXTERNAL(data) != VARTAG_ONDISK)
			ereport(ERROR,
					(errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
					 errmsg("native catalog carrier has an unsupported external form")));
		bytes = VARHDRSZ_EXTERNAL + sizeof(struct varatt_external);
	}
	else if (VARATT_IS_SHORT(data))
	{
		bytes = VARSIZE_SHORT(data);
		if (bytes < VARHDRSZ_SHORT)
			elog(ERROR, "native catalog short carrier has an invalid size");
	}
	else
	{
		if (available < VARHDRSZ)
			elog(ERROR, "native catalog carrier has a truncated length header");
		memcpy(&aligned_header, data, VARHDRSZ);
		if (!VARATT_IS_4B_U(&aligned_header) && !VARATT_IS_4B_C(&aligned_header))
			elog(ERROR, "native catalog carrier has an unknown inline form");
		bytes = VARSIZE_4B(&aligned_header);
		if (bytes < (VARATT_IS_4B_C(&aligned_header) ? VARHDRSZ_COMPRESSED : VARHDRSZ))
			elog(ERROR, "native catalog carrier has an invalid inline header size");
	}
	if (bytes > available)
		elog(ERROR, "native catalog carrier extends past its tuple");
	return bytes;
}

void
darmok_catalog_carrier_copy(DarmokCatalogImageBudget *budget,
						   DarmokCatalogCarrier *carrier, HeapTuple tuple,
						   TupleDesc descriptor, AttrNumber number, bool external_allowed)
{
	bool is_null = false;
	Datum datum = darmok_catalog_present_attr(tuple, descriptor, number, &is_null);
	const char *data;
	Size bytes;

	memset(carrier, 0, sizeof(*carrier));
	if (is_null)
		return;
	data = DatumGetPointer(datum);
	bytes = payload_tuple_carrier(tuple, data, external_allowed);
	carrier->data = darmok_catalog_image_alloc(budget, bytes, false);
	memcpy(carrier->data, data, bytes);
	carrier->bytes = bytes;
	carrier->present = true;
}

static char
payload_compression(uint32 method)
{
	if (method == TOAST_PGLZ_COMPRESSION_ID)
		return 'p';
	if (method == TOAST_LZ4_COMPRESSION_ID)
		return 'l';
	elog(ERROR, "native catalog carrier has an unknown compression method");
	return 0;
}

void
darmok_catalog_carrier_image(DarmokCatalogImageBudget *budget,
							const DarmokCatalogCarrier *carrier, DarmokHeapPayloadImage *image)
{
	struct varlena *data = carrier->data;
	struct varlena *plain;
	Size expanded;

	memset(image, 0, sizeof(*image));
	if (!carrier->present)
		return;
	if (data == NULL || carrier->bytes == 0 || VARATT_IS_EXTERNAL(data))
		elog(ERROR, "native catalog inline normalization has no admitted carrier");
	image->present = true;
	image->carrier_bytes = carrier->bytes;
	image->stored_bytes = carrier->bytes;
	if (VARATT_IS_SHORT(data))
	{
		expanded = carrier->bytes - VARHDRSZ_SHORT + VARHDRSZ;
		plain = darmok_catalog_image_alloc(budget, expanded, false);
		SET_VARSIZE(plain, expanded);
		memcpy(VARDATA(plain), VARDATA_SHORT(data), expanded - VARHDRSZ);
		image->carrier_kind = 's';
	}
	else if (VARATT_IS_COMPRESSED(data))
	{
		Size payload = VARDATA_COMPRESSED_GET_EXTSIZE(data);
		MemoryContext previous;

		if (payload > MaxAllocSize - VARHDRSZ)
			elog(ERROR, "native catalog expansion size exceeds native limits");
		expanded = payload + VARHDRSZ;
		image->compression_kind = payload_compression(VARDATA_COMPRESSED_GET_COMPRESS_METHOD(data));
		image->carrier_kind = image->compression_kind;
		/* These pinned builtin decoders allocate exactly this output size.
		 * No generic detoaster can follow another pointer or load a provider. */
		payload_reserve(budget, expanded);
		previous = MemoryContextSwitchTo(budget->context);
		plain = image->compression_kind == 'p'
			? pglz_decompress_datum(data) : lz4_decompress_datum(data);
		MemoryContextSwitchTo(previous);
	}
	else if (VARATT_IS_4B_U(data))
	{
		expanded = carrier->bytes;
		plain = data;
		image->carrier_kind = 'u';
	}
	else
		elog(ERROR, "native catalog normalization has an unknown inline form");
	if (!VARATT_IS_4B_U(plain) || (Size) VARSIZE(plain) != expanded)
		elog(ERROR, "native catalog decoder did not produce its exact declared length");
	image->image_bytes = expanded;
	image->image = (const char *) plain;
}

static void
payload_pointer(const DarmokCatalogCarrier *carrier, struct varatt_external *pointer)
{
	Size stored;
	Size raw;

	if (!carrier->present || carrier->data == NULL ||
		carrier->bytes != VARHDRSZ_EXTERNAL + sizeof(*pointer) ||
		!VARATT_IS_EXTERNAL_ONDISK(carrier->data))
		elog(ERROR, "native catalog external normalization has no admitted pointer");
	memcpy(pointer, VARDATA_EXTERNAL(carrier->data), sizeof(*pointer));
	if (pointer->va_rawsize < VARHDRSZ || !OidIsValid(pointer->va_valueid) ||
		!OidIsValid(pointer->va_toastrelid))
		elog(ERROR, "native catalog external pointer has an invalid identity or raw size");
	raw = (Size) pointer->va_rawsize - VARHDRSZ;
	stored = VARATT_EXTERNAL_GET_EXTSIZE(*pointer);
	if (stored > raw || raw > MaxAllocSize - VARHDRSZ ||
		stored > MaxAllocSize - VARHDRSZ)
		elog(ERROR, "native catalog external pointer has inconsistent stored/raw sizes");
	if ((Size) pointer->va_rawsize > DARMOK_CATALOG_PHASE_BYTES)
		ereport(ERROR,
				(errcode(ERRCODE_PROGRAM_LIMIT_EXCEEDED),
				 errmsg("native catalog external image exceeds the 64 MiB phase budget")));
	if (VARATT_EXTERNAL_IS_COMPRESSED(*pointer))
	{
		if (stored < sizeof(uint32))
			elog(ERROR, "native catalog compressed external pointer has a truncated header");
		(void) payload_compression(VARATT_EXTERNAL_GET_COMPRESS_METHOD(*pointer));
	}
	else if (stored == 0 && pointer->va_rawsize != VARHDRSZ)
		elog(ERROR, "native catalog zero-size pointer is not an empty plain value");
}

static PayloadGroup *
payload_group(HTAB *groups, const DarmokCatalogCarrier *carrier,
			  DarmokCatalogImageBudget *budget, bool create)
{
	struct varatt_external pointer;
	uint64 key;
	PayloadGroup *group;
	bool found;
	Size stored;

	payload_pointer(carrier, &pointer);
	key = ((uint64) pointer.va_toastrelid << 32) | pointer.va_valueid;
	group = hash_search(groups, &key, create ? HASH_ENTER : HASH_FIND, &found);
	if (group == NULL)
		elog(ERROR, "native catalog lost a selected external value");
	if (found)
	{
		if (memcmp(&group->pointer, &pointer, sizeof(pointer)) != 0)
			elog(ERROR, "native catalog shared external value has contradictory pointer facts");
		return group;
	}
	memset((char *) group + sizeof(group->key), 0, sizeof(*group) - sizeof(group->key));
	group->pointer = pointer;
	stored = VARATT_EXTERNAL_GET_EXTSIZE(pointer);
	/* Ceiling division avoids addition overflow. Native chunk size is pinned
	 * to this build; sequence offsets remain Size, not a signed product. */
	group->chunks = stored / TOAST_MAX_CHUNK_SIZE + (stored % TOAST_MAX_CHUNK_SIZE != 0);
	if (group->chunks > MaxAllocSize / sizeof(bool))
		elog(ERROR, "native catalog chunk seen-state exceeds native limits");
	group->stored = darmok_catalog_image_alloc(budget, stored + VARHDRSZ, false);
	if (group->chunks > 0)
		group->seen = darmok_catalog_image_alloc(budget, group->chunks * sizeof(bool), true);
	if (VARATT_EXTERNAL_IS_COMPRESSED(pointer))
		SET_VARSIZE_COMPRESSED(group->stored, stored + VARHDRSZ);
	else
		SET_VARSIZE(group->stored, stored + VARHDRSZ);
	return group;
}

Oid
darmok_catalog_carrier_toast(const DarmokCatalogCarrier *carrier)
{
	struct varatt_external pointer;

	if (!carrier->present || !VARATT_IS_EXTERNAL(carrier->data))
		return InvalidOid;
	payload_pointer(carrier, &pointer);
	return pointer.va_toastrelid;
}

static void
payload_scan(Relation heap, HTAB *groups, DarmokCatalogPayloadCost *cost,
			 void (*context_check)(void *), void *context)
{
	volatile TableScanDesc scan = NULL;
	Snapshot snapshot;
#if PG_VERSION_NUM < 180000
	SnapshotData toast_snapshot;

	init_toast_snapshot(&toast_snapshot);
	snapshot = &toast_snapshot;
#else
	snapshot = get_toast_snapshot();
#endif
	PG_TRY();
	{
		HeapTuple tuple;
		TupleDesc descriptor = RelationGetDescr(heap);

		/* Caller has freshly admitted this heap before opening its descriptor.
		 * Direct zero-key heap scanning never opens a TOAST index. */
		scan = heap_beginscan(heap, snapshot, 0, NULL, NULL, SO_TYPE_SEQSCAN);
		cost->toast_heaps++;
		while ((tuple = heap_getnext(scan, ForwardScanDirection)) != NULL)
		{
			bool is_null = false;
			Oid value_oid = DatumGetObjectId(darmok_catalog_present_attr(tuple, descriptor, 1, &is_null));
			uint64 key;
			PayloadGroup *group;
			int32 sequence;
			Datum datum;
			const char *data;
			Size carrier_bytes;
			Size offset;
			Size expected;
			Size length;

			cost->toast_rows++;
			if (cost->toast_rows % 1024 == 0)
				context_check(context);
			if (is_null)
				elog(ERROR, "native catalog TOAST chunk has a NULL value identity");
			key = ((uint64) RelationGetRelid(heap) << 32) | value_oid;
			group = hash_search(groups, &key, HASH_FIND, NULL);
			if (group == NULL)
				continue; /* Do not read data of unselected values. */
			sequence = DatumGetInt32(darmok_catalog_present_attr(tuple, descriptor, 2, &is_null));
			if (is_null || sequence < 0 || (Size) sequence >= group->chunks || group->seen[sequence])
				elog(ERROR, "native catalog TOAST sequence is NULL, duplicate or out of range");
			datum = darmok_catalog_present_attr(tuple, descriptor, 3, &is_null);
			if (is_null)
				elog(ERROR, "native catalog TOAST chunk has NULL data");
			data = DatumGetPointer(datum);
			carrier_bytes = payload_tuple_carrier(tuple, data, false);
			if (VARATT_IS_COMPRESSED(data))
				elog(ERROR, "native catalog TOAST chunk data must be plain or short inline bytea");
			length = carrier_bytes - (VARATT_IS_SHORT(data) ? VARHDRSZ_SHORT : VARHDRSZ);
			offset = (Size) sequence * TOAST_MAX_CHUNK_SIZE;
			expected = Min((Size) TOAST_MAX_CHUNK_SIZE,
						   (Size) VARATT_EXTERNAL_GET_EXTSIZE(group->pointer) - offset);
			if (length != expected)
				elog(ERROR, "native catalog TOAST chunk has an inconsistent length");
			memcpy(VARDATA(group->stored) + offset, VARDATA_ANY(data), length);
			group->seen[sequence] = true;
			group->copied++;
			cost->selected_chunks++;
			context_check(context);
		}
		context_check(context);
	}
	PG_FINALLY();
	{
		if (scan != NULL)
			heap_endscan(scan);
	}
	PG_END_TRY();
}

void
darmok_catalog_payload_images(DarmokCatalogImageBudget *budget,
							  const DarmokCatalogPayloadRequest *requests, int count,
							  Relation *toast_heaps, int heap_count,
							  DarmokCatalogPayloadCost *cost,
							  void (*context_check)(void *), void *context)
{
	HASHCTL ctl = {0};
	HTAB *groups;
	HASH_SEQ_STATUS iter;
	PayloadGroup *group;

	ctl.keysize = sizeof(uint64);
	ctl.entrysize = sizeof(PayloadGroup);
	ctl.hcxt = budget->context;
	groups = hash_create("native catalog external values", 32, &ctl,
						 HASH_ELEM | HASH_BLOBS | HASH_CONTEXT);
	for (int i = 0; i < count; i++)
	{
		const DarmokCatalogCarrier *carrier = requests[i].carrier;

		if (carrier->present && VARATT_IS_EXTERNAL(carrier->data))
		{
			bool admitted = false;

			group = payload_group(groups, carrier, budget, true);
			for (int j = 0; j < heap_count; j++)
				admitted |= RelationGetRelid(toast_heaps[j]) == group->pointer.va_toastrelid;
			if (!admitted)
				elog(ERROR, "native catalog external value has no admitted TOAST descriptor");
		}
		else
			darmok_catalog_carrier_image(budget, carrier, requests[i].image);
		context_check(context);
	}
	for (int i = 0; i < heap_count; i++)
		payload_scan(toast_heaps[i], groups, cost, context_check, context);
	hash_seq_init(&iter, groups);
	while ((group = hash_seq_search(&iter)) != NULL)
	{
		DarmokCatalogCarrier reconstructed;

		if (group->copied != group->chunks)
			elog(ERROR, "native catalog external value has missing TOAST chunks");
		if (VARATT_EXTERNAL_IS_COMPRESSED(group->pointer) &&
			(VARDATA_COMPRESSED_GET_EXTSIZE(group->stored) !=
			 (Size) group->pointer.va_rawsize - VARHDRSZ ||
			 VARDATA_COMPRESSED_GET_COMPRESS_METHOD(group->stored) !=
			 VARATT_EXTERNAL_GET_COMPRESS_METHOD(group->pointer)))
			elog(ERROR, "native catalog compressed header disagrees with its external pointer");
		reconstructed.data = group->stored;
		reconstructed.bytes = VARATT_EXTERNAL_GET_EXTSIZE(group->pointer) + VARHDRSZ;
		reconstructed.present = true;
		darmok_catalog_carrier_image(budget, &reconstructed, &group->image);
		if (group->image.image_bytes != (Size) group->pointer.va_rawsize)
			elog(ERROR, "native catalog external image has an inconsistent raw length");
		group->image.carrier_kind = 'e';
		group->image.carrier_bytes = VARHDRSZ_EXTERNAL + sizeof(struct varatt_external);
		group->image.stored_bytes = VARATT_EXTERNAL_GET_EXTSIZE(group->pointer);
		group->image.toast_oid = group->pointer.va_toastrelid;
		group->image.value_oid = group->pointer.va_valueid;
		context_check(context);
	}
	for (int i = 0; i < count; i++)
	{
		const DarmokCatalogCarrier *carrier = requests[i].carrier;
		DarmokHeapPayloadImage *image = requests[i].image;

		if (carrier->present && VARATT_IS_EXTERNAL(carrier->data))
			*image = payload_group(groups, carrier, budget, false)->image;
		/* Per-source lengths retain duplicate-source identity. Decoder/group
		 * allocation is shared; budget.requested_bytes counts actual requests. */
		if (image->stored_bytes > budget->limit - cost->stored_bytes ||
			image->image_bytes > budget->limit - cost->image_bytes)
			elog(ERROR, "native catalog selected image accounting exceeds phase limits");
		cost->stored_bytes += image->stored_bytes;
		cost->image_bytes += image->image_bytes;
	}
}
