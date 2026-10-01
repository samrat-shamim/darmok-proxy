# Explicit native transaction characteristics

Status: **native control component; frontend transaction equivalence is
pending**. `NativeTransactionSpec` supplies explicit choices to
`NativeBackend::begin` and `NativeBackend::transaction_scope`.

An unqualified BEGIN inherits PostgreSQL's configured transaction defaults.
Those settings are not a declared proxy session policy. Depending on connection
setup would also leave an owned statement transaction's choices implicit. The
owner instead selects one fixed BEGIN string for each complete specification.
This puts the choices in the same checked control request as transaction start,
without a separate SET or a configuration/readback round trip.

## Choices and boundaries

The specification requires both isolation and access; it has no Default:

- `NativeIsolation`: ReadCommitted, RepeatableRead or Serializable.
- `NativeTransactionAccess`: ReadWrite or ReadOnly.
- Every transaction explicitly uses NOT DEFERRABLE.

The six combinations override session defaults for that transaction and leave
the defaults unchanged. These are
[native PostgreSQL BEGIN characteristics](https://www.postgresql.org/docs/18/sql-begin.html).
The initial API has no deferrable mode, imported snapshot or ReadUncommitted
choice. PostgreSQL treats ReadUncommitted as ReadCommitted; this API does not
introduce that alias as a frontend isolation mapping. See
[SET TRANSACTION](https://www.postgresql.org/docs/17/sql-set-transaction.html).

`begin(spec)` starts an explicit native transaction from confirmed idle.
`transaction_scope(spec)` starts a borrowed owned-transaction scope from
confirmed idle. `savepoint_scope()` requires an existing confirmed transaction
and retains its parent's characteristics. A transaction scope inside a
transaction, or a savepoint scope outside one, fails before submission. There
is no state-dependent API which accepts choices and silently ignores them.

Savepoint identities, complete control checking, explicit recovery and
uncertainty disposition follow the [backend ownership contract](native-backend.md).
The scope still exposes no row executor, raw SQL or Client.

## Costs and evidence

Specification selection borrows a static SQL string and performs no SQL-builder
allocation. BEGIN remains one control request, with additional SQL bytes and
mode parsing compared with an unqualified BEGIN. Savepoint request/identity
costs are unchanged. These are structural costs; latency and workload effects
remain unmeasured.

Two additional required native-owner fixtures run on PostgreSQL 17 and 18.
They read actual current isolation/access/deferrability for all six combinations
through both explicit BEGIN and transaction scopes, override deliberately
different session defaults and check those defaults remain unchanged. The
savepoint fixture checks all six parent combinations through rollback-and-release
and successful release, plus both wrong-boundary errors. Readback is test-only;
production controls do not add those queries.

These fixtures do not prove MySQL snapshots, lock retention, frontend SET/BEGIN
semantics, status flags, error classification, catalog validity or admitted
statement execution. A future frontend controller must select characteristics
from verified session policy and retain the scope through output validation.
The [stock MySQL characteristic fixture](mysql-transaction-characteristics.md)
observes session/next/active lifetimes separately; it does not establish native
transaction equivalence. M2/M4 and release gates remain pending.
Security-related work remains deferred.
