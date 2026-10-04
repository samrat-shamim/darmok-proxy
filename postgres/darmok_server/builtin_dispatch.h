/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#ifndef DARMOK_BUILTIN_DISPATCH_H
#define DARMOK_BUILTIN_DISPATCH_H

#include "postgres.h"

#define DARMOK_BUILTIN_DISPATCH_ROWS 2
#define DARMOK_BUILTIN_DISPATCH_NAME_BYTES 32

typedef struct DarmokBuiltinDispatchRow
{
	Oid oid;
	uint16 index;
	int16 nargs;
	bool strict;
	bool retset;
	bool linked_symbol;
	char name[DARMOK_BUILTIN_DISPATCH_NAME_BYTES];
} DarmokBuiltinDispatchRow;

typedef struct DarmokBuiltinDispatch
{
	int32 builtin_count;
	Oid last_builtin_oid;
	DarmokBuiltinDispatchRow rows[DARMOK_BUILTIN_DISPATCH_ROWS];
} DarmokBuiltinDispatch;

/* Private observation outside relation attempts, refresh and module fences.
 * The caller supplies a live writable record. Successful return copies both
 * linked builtin dispatch rows; it retains no native pointer, opens no catalog
 * and invokes neither handler. It is not descriptor/provider admission or a
 * certificate of original executable contents or callback/registry history. */
extern PGDLLEXPORT void darmok_builtin_dispatch_capture(DarmokBuiltinDispatch *result);

#endif
