/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#include "postgres.h"

#include "fmgr.h"
#include "miscadmin.h"
#include "utils/fmgroids.h"
#include "utils/fmgrprotos.h"
#include "utils/fmgrtab.h"

#include "builtin_dispatch.h"
#include "catalog_read.h"

#if PG_VERSION_NUM < 170000 || PG_VERSION_NUM >= 190000
#error "builtin dispatch observation requires PostgreSQL 17 or 18"
#endif

StaticAssertDecl(F_HEAP_TABLEAM_HANDLER == 3, "unexpected heap handler OID");
StaticAssertDecl(F_BTHANDLER == 330, "unexpected btree handler OID");
StaticAssertDecl(sizeof("heap_tableam_handler") <= DARMOK_BUILTIN_DISPATCH_NAME_BYTES,
				 "heap handler name exceeds its inline record");
StaticAssertDecl(sizeof("bthandler") <= DARMOK_BUILTIN_DISPATCH_NAME_BYTES,
				 "btree handler name exceeds its inline record");

static void
unexpected_dispatch(Oid oid)
{
	ereport(ERROR,
			(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
			 errmsg("native builtin handler %u does not match its linked dispatch", oid)));
}

static void
capture_row(DarmokBuiltinDispatchRow *result, Oid oid,
			const char *name, Size name_bytes, PGFunction symbol)
{
	uint16 index;
	const FmgrBuiltin *row;

	if (oid > fmgr_last_builtin_oid)
		unexpected_dispatch(oid);
	index = fmgr_builtin_oid_index[oid];
	if (index == InvalidOidBuiltinMapping || index >= fmgr_nbuiltins)
		unexpected_dispatch(oid);
	row = &fmgr_builtins[index];
	/* The generated public native table and linked symbol definitions belong
	 * to the bound build. Compare them without fmgr_info, syscache lookup,
	 * OidFunctionCall or an AM routine invocation. A name comparison includes
	 * its NUL and is bounded by the fixed expected literal. */
	if (row->foid != oid || row->nargs != 1 || !row->strict || row->retset ||
		row->funcName == NULL || strncmp(row->funcName, name, name_bytes) != 0 ||
		row->func != symbol)
		unexpected_dispatch(oid);
	result->oid = row->foid;
	result->index = index;
	result->nargs = row->nargs;
	result->strict = row->strict;
	result->retset = row->retset;
	result->linked_symbol = true;
	memcpy(result->name, row->funcName, name_bytes);
}

void
darmok_builtin_dispatch_capture(DarmokBuiltinDispatch *result)
{
	DarmokBuiltinDispatch observation = {0};

	darmok_native_invocation_check();
	if (!IsNormalProcessingMode())
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("native builtin observation requires normal backend processing")));
	/* Mapping entries are uint16; their maximum value denotes absence. Reject
	 * an invalid count before interpreting either selected table index. */
	if (fmgr_nbuiltins <= 0 || fmgr_nbuiltins > InvalidOidBuiltinMapping)
		ereport(ERROR,
				(errcode(ERRCODE_OBJECT_NOT_IN_PREREQUISITE_STATE),
				 errmsg("native builtin dispatch table has an invalid count")));
	observation.builtin_count = fmgr_nbuiltins;
	observation.last_builtin_oid = fmgr_last_builtin_oid;
	capture_row(&observation.rows[0], F_HEAP_TABLEAM_HANDLER,
				"heap_tableam_handler", sizeof("heap_tableam_handler"), heap_tableam_handler);
	capture_row(&observation.rows[1], F_BTHANDLER,
				"bthandler", sizeof("bthandler"), bthandler);
	/* No output prefix escapes if either row fails. No allocation, loader,
	 * interrupt processing, callback or handler call occurs in the copy. */
	*result = observation;
}
