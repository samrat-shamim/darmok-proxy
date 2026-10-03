/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#ifndef DARMOK_HEAP_STORAGE_H
#define DARMOK_HEAP_STORAGE_H

#include "relation_guard.h"

typedef struct DarmokHeapStorageRoot
{
	const char *schema;
	Size schema_bytes;
	const char *name;
	Size name_bytes;
	LOCKMODE lock_mode;
} DarmokHeapStorageRoot;

typedef struct DarmokHeapStorageFact
{
	Oid oid;
	Oid schema_oid;
	NameData schema_name;
	NameData name;
	Oid access_method_oid;
	Oid toast_oid;
	Oid parent_oid;
	Oid tablespace_oid;
	Oid stored_file_number;
	Oid row_type_oid;
	int16 declared_attribute_count;
	Oid file_tablespace_oid;
	Oid file_database_oid;
	uint32 file_number;
	int32 file_proc_number;
	char kind;
	char persistence;
	uint8 mode_mask;
	bool shared;
	bool is_partition;
	bool has_indexes;
	bool has_subclasses;
	bool index_live;
	bool index_ready;
	bool index_valid;
	bool index_check_xmin;
} DarmokHeapStorageFact;

typedef struct DarmokHeapStorageRootFact
{
	Oid oid;
	Oid row_type_oid;
	int16 declared_attribute_count;
	int attribute_offset;
} DarmokHeapStorageRootFact;

/* Positive native slots, including dropped layout/type0. These are fixed
 * declarations, not a TupleDesc, value codec or frontend nullability proof. */
typedef struct DarmokHeapAttributeFact
{
	Oid relation_oid;
	int16 number;
	NameData name;
	Oid type_oid;
	int16 length;
	int32 typmod;
	int16 dimensions;
	bool by_value;
	char alignment;
	char storage;
	char compression;
	bool not_null_declared;
	bool has_default;
	bool has_missing;
	char identity;
	char generated;
	bool dropped;
	bool local;
	int16 inheritance_count;
	Oid collation_oid;
} DarmokHeapAttributeFact;

/* Direct live-column types only. OID edges are declarations, not completed
 * transitive dependencies or permission to invoke the named providers. */
typedef struct DarmokHeapTypeFact
{
	Oid oid;
	Oid schema_oid;
	NameData schema_name;
	NameData name;
	int16 length;
	bool by_value;
	char kind;
	char category;
	bool preferred;
	bool defined;
	char delimiter;
	Oid relation_oid;
	Oid subscript_oid;
	Oid element_oid;
	Oid array_oid;
	Oid input_oid;
	Oid output_oid;
	Oid receive_oid;
	Oid send_oid;
	Oid typmod_input_oid;
	Oid typmod_output_oid;
	Oid analyze_oid;
	char alignment;
	char storage;
	bool not_null_declared;
	Oid base_type_oid;
	int32 typmod;
	int32 dimensions;
	Oid collation_oid;
} DarmokHeapTypeFact;

typedef struct DarmokHeapObservationCost
{
	uint64 namespace_rows;
	uint64 relation_rows;
	uint64 index_rows;
	uint64 attribute_rows;
	uint64 type_rows;
	Size allocated_bytes;
} DarmokHeapObservationCost;

typedef struct DarmokHeapStorageView
{
	/* Input bindings preserve duplicates; facts/references are deterministic
	 * parent-first with each exact OID/mode increment represented once. */
	const DarmokRelationRequest *roots;
	int root_count;
	/* Root facts follow input order; duplicates share ordered fact ranges. */
	const DarmokHeapStorageRootFact *root_facts;
	const DarmokHeapAttributeFact *attributes;
	int attribute_count;
	const DarmokHeapTypeFact *types;
	int type_count;
	Size attribute_array_bytes;
	Size type_array_bytes;
	DarmokHeapObservationCost initial_cost;
	DarmokHeapObservationCost final_cost;
	const DarmokHeapStorageFact *facts;
	int fact_count;
	const DarmokRelationRequest *references;
	int reference_count;
	uint64 generation;
	uint64 local_generation;
	int attempts;
	bool physical_owned;
	bool metadata_owned;
	bool first_snapshot_set;
} DarmokHeapStorageView;

/* Private source-admitted pure C consumer only. It may copy facts, but no
 * borrowed pointer escapes. No SQL, provider, descriptor, output, row wait or
 * transaction mutation is permitted. Raw/gate refs have ended; semantic S and
 * all exact physical refs are owned for this call. This is not a purity
 * detector, SQL admission, immutable plan, or data-derived TOAST closure. */
typedef void (*DarmokHeapStorageConsumer)(const DarmokHeapStorageView *, void *);

/* Ordinary nonpartitioned builtin heaps with builtin btree live indexes only.
 * All observations use bounded no-CV lifecycle/generation retry. Native waits
 * and ERROR keep ordinary abort semantics. After the consumer, semantic S ends
 * before reader cleanup. Release completes all counts; retain leaves only
 * native transaction/subtransaction counts. Copies are then historical. */
extern PGDLLEXPORT void darmok_heap_storage_metadata(
	const DarmokHeapStorageRoot *roots, int count,
	DarmokHeapStorageConsumer consumer, void *consumer_state, bool retain);

#endif
