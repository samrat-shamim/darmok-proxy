/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#ifndef DARMOK_RELATION_GUARD_H
#define DARMOK_RELATION_GUARD_H

#include "storage/lockdefs.h"

/* Private native C interface, not a SQL/frontend lease or a closure proof.
 * Caller order is preserved. Exact duplicate OID/mode declarations are errors.
 * Only AccessShareLock, RowShareLock and RowExclusiveLock are supported. */
#define DARMOK_RELATION_REQUEST_LIMIT 4096

typedef struct DarmokRelationRequest
{
	Oid relation_oid;
	LOCKMODE lock_mode;
} DarmokRelationRequest;

typedef struct DarmokRelationAttempt
{
	uint64 identity;
} DarmokRelationAttempt;

/* An automatic token spanning PG_TRY must be volatile. Release within this C
 * invocation, or retain the actual references for native transaction cleanup.
 * Native acquisition/cache-refresh ERROR requires native abort before reuse.
 * Native LockRelationOid dispatches invalidations and clears exact local modes
 * before module exclusion. Already-clear modes may skip dispatch: this is not
 * global SI freshness, callback admission or snapshot-neutral preparation. */
extern PGDLLEXPORT void darmok_relation_attempt_acquire(
	volatile DarmokRelationAttempt *attempt,
	const DarmokRelationRequest *requests, int count);
extern PGDLLEXPORT void darmok_relation_attempt_release(volatile DarmokRelationAttempt *attempt);
extern PGDLLEXPORT void darmok_relation_attempt_retain(volatile DarmokRelationAttempt *attempt);
extern PGDLLEXPORT bool darmok_relation_attempt_owned(const volatile DarmokRelationAttempt *attempt);

#endif
