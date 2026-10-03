/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#ifndef DARMOK_STATEMENT_GUARD_H
#define DARMOK_STATEMENT_GUARD_H

/* Private C invocation reference, not a SQL/frontend handle or an admitted
 * plan. Physical/candidate closure and preparation remain separate gates.
 * Automatic tokens spanning PG_TRY must be declared volatile. Release an owned
 * token in PG_FINALLY before returning from the invocation. The module never
 * borrows this address and never trusts it to choose a ResourceOwner. */
typedef struct DarmokStatementGuard
{
	uint64 identity;
} DarmokStatementGuard;

/* false is an explicit lifecycle retry: no Share reference remains. The caller
 * must release its physical attempt refs before any lifecycle wait. */
extern PGDLLEXPORT bool darmok_statement_guard_acquire(volatile DarmokStatementGuard *guard);
extern PGDLLEXPORT void darmok_statement_guard_release(volatile DarmokStatementGuard *guard);
extern PGDLLEXPORT bool darmok_statement_guard_owned(const volatile DarmokStatementGuard *guard);

/* Native dummy inspection, without changing the one-time shared ready state.
 * No SQL registration; also used by the separate native test probe. */
extern PGDLLEXPORT void darmok_statement_guard_check_prepared(void);

#endif
