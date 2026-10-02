# Server catalog lease

`postgres/darmok_server` implements a PostgreSQL 17/18 catalog generation and
transaction-owned read fence. It does not implement statement admission, fresh
catalog reads, the owning Rust execution scope, complete result definitions,
plan caching, or a MySQL table executor. Those remain required before exposing
catalog-dependent SQL. Component evidence is recorded in the release plan.

## Installation and identity

Build against the selected server's PGXS development files, preload
`darmok_server` at server startup, and explicitly `CREATE EXTENSION darmok_server`
in each selected physical database. Late loading is an error. The nonrelocatable
extension has version `1.0`; its namespace is `darmok_server`, separate from the
SQL compatibility functions in `darmok`. Current `init`/`schema verify` cover only
the latter. Combined installation and exact module verification remain pending.

Leases require a primary server. Concurrent native PostgreSQL two-phase
transactions are part of the v0.1 target; `max_prepared_transactions` may be
nonzero. This is separate from MySQL prepared statements and MySQL XA support.
The module does not change the server configuration.

WAL replay does not execute these utility hooks, so a standby cannot supply
this coordination mechanism. Counter exhaustion fails before publication, including before irreversible
shared-drop effects; identities never wrap. Each server start establishes a new 128-bit random incarnation, and
each participating backend receives an increasing identifier. Persistent cache
identities cannot assume generations survive a server restart or reuse a backend
identifier without its incarnation.

## API and ownership

The fixed acquisition query is:

```sql
SELECT * FROM darmok_server.begin_catalog_lease();
```

It requires a PostgreSQL transaction block, allows one active lease per backend,
and returns six non-NULL fields. An explicit `BEGIN` block can span requests.
A multi-statement simple query also creates an implicit block; a lease acquired
there expires at the request's native commit and cannot span requests. A single
standalone acquisition fails. The owning proxy executor must use an explicit
transaction for its separate acquisition, execution and release requests.

| Field | Native type | Meaning |
| --- | --- | --- |
| `lease_id` | `int8` | Increasing handle within this backend |
| `cluster_id` | `bytea` | Opaque 16-byte server incarnation |
| `database_oid` | `oid` | Exact physical database |
| `backend_id` | `int8` | Incarnation-scoped connection identity |
| `generation` | `int8` | Cluster catalog publication identity |
| `local_generation` | `int8` | This backend's private catalog identity |

`check_catalog_lease(lease_id, backend_id)` validates active ownership and both
generations. `end_catalog_lease(lease_id, backend_id)` releases the owned read
fence. NULL, expired, duplicate or mismatched handles produce errors; there is no
idempotent success for an absent lease. A backend identity is part of the handle
because different backends can both have a local lease number `1`. Acquisition
inside a metadata utility is rejected.

The lock belongs to `CurTransactionResourceOwner`, not the SQL function's portal.
A later command can release it. PostgreSQL releases unfinished leases at top-level
commit/rollback and at rollback of their owning savepoint. Savepoint release
promotes ownership to the parent. A parent lease survives a metadata-free child
error. The local catalog generation invalidates private facts on metadata rollback
or transaction prepare, while metadata-free transactions retain reusable catalog
identity. Handles and catalog cache identities have different lifetimes.

The global lease covers only admitted catalog resolution and validation. It
must end before any operation that can wait for user-relation, tuple, transaction
ID or user locks. PostgreSQL planning can execute user/support functions; planning
is not inherently safe under this lease. The future admission boundary must prove
its permitted preparation paths cannot perform those waits.

The required statement sequence is:

1. Read dependencies using fixed, fresh, non-row-locking catalog operations under
   a discovery lease; then end that lease.
2. Acquire the complete native relation/object dependency guards outside the
   global lease. These waits may include prepared transactions.
3. Reacquire a catalog lease and resolve under a fresh snapshot. Validate that
   dependencies are closed and unchanged. If another dependency is found, end
   the lease and restart before effects; never acquire another guard under it.
4. Prepare and validate only admitted nonblocking paths, then end the global
   lease before row execution.
5. Retain the complete dependency guards through execution and output validation.

These guards, fresh reads and semantic admission are still required components.
The global lease alone cannot protect a running plan. Native object replacements
without sufficient native locking also need a verified guard integration.
Dropped or failed scopes cannot assert a live lease.

## Catalog publication

The global fence is a synthetic object lock in PostgreSQL's default lock method:
database zero, `pg_extension` class, object zero and sub-ID `0x444d`. A zero object
OID cannot name an extension. Readers take `ShareLock`; metadata publishers take
`ExclusiveLock`. User advisory locks use a separate lock method.

Ordinary metadata utilities mark the backend's private catalog identity before
attempting changes. They acquire the global fence at native pre-commit, after
the utility's normal dependency locks/effects and deferred triggers. The module
inspects PostgreSQL's pending transactional invalidations through
[`xactGetCommittedInvalidationMessages`](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/inval.c),
which does not consume them. Native subabort removes a child's messages. A
rolled-back child DDL or a no-effect utility does not become a surviving metadata
publication merely because it was attempted. Private identity may still advance
conservatively. Ordinary DML without catalog publication takes no writer fence.

The pre-commit callback precedes native ON COMMIT actions. Core temporary-object
drops are also covered by object hooks after the callback starts. Arbitrary
extension callbacks that introduce new blocking dependencies after this boundary
need their own ordering proof and are outside this contract. Physical statistics,
storage mappings and sequence values are not immutable schema dependencies;
this mechanism does not freeze those non-MVCC values.

A writer retains its transaction fence through native catalog invalidation and
lock cleanup. Waiting readers consume invalidations after acquisition. Internal
commits, including concurrent index phases, each get a new publication generation.
The final valid-index transition is fenced at the final transaction's pre-commit.
No session fence spans the old-snapshot waits between phases.

`DROP DATABASE` changes its invalid marker in place before commit. Its native
drop hook therefore acquires the fence after database lookup/locking and before
that change. A tablespace drop is likewise fenced at its native drop hook before
irreversible directory removal. Taking either fence at utility entry would
reverse the native dependency ordering. Database moves retain native database
locks across their file-copy and internal commit; their transactional catalog
publication receives the ordinary commit fence.

## Prepared transactions

PREPARE does not publish metadata and never transfers the global fence. The
module intentionally transfers a separate default-method object marker keyed by
physical database, exact native XID and sub-ID `0x444e`. `AccessShareLock` proves
participation; an additional `RowExclusiveLock` marks pending catalog publication.
PostgreSQL's native [lock two-phase records](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/storage/lmgr/lock.c)
persist, transfer and recover these tag/mode pairs. This recovery argument is
based on native source; the ordinary fixtures do not run restart experiments.

PREPARE, COMMIT PREPARED and ROLLBACK PREPARED acquire a default-method **session**
gate keyed by a GID hash and sub-ID `0x444f`. Completion holds it through the native finish operation. PREPARE queues its
work at utility entry, so creation retains the gate until the native prepare
callback after validity and marker-lock transfer, or until abort. Native busy
checks protect the remaining detach cleanup. Utility errors and owning
subtransaction abort also release it. It is never transferred to a prepared
transaction.
Hash collisions only add serialization. The gate prevents completion and GID
reuse from changing the target between the exact native `pg_prepared_xacts`
lookup, conditional XID-marker probes and native completion. A metadata-marked
completion takes the publication fence; a proven metadata-free one does not.
An unmarked prepared transaction has no metadata-free proof and receives the
publication fence. Native database, ownership and busy checks still run.

The creation/completion ordering covers native SQL utility entry. Direct calls
to `FinishPreparedTransaction` by custom modules bypass that hook and are outside
the metadata-publication contract. PostgreSQL logical replication also has direct
native callers; native replication is not a schema/DDL publication integration
provided by this module.

A prepared transaction can alter table A while holding row locks in unrelated
B. A query waiting on B while holding the global lease would block completion
of that transaction. Prelocking B alone does not solve this cycle. Ending the
global lease before row execution is mandatory even with complete dependencies.

## Backend exit and native barriers

Native temporary-table cleanup runs before the exiting backend retires its
process-signal slot. Exit suppresses ordinary interrupt handling. A backend
waiting for the global fence there could prevent a database-removal storage
barrier from completing while removal owns that same fence.

At the temporary cleanup's pre-commit boundary, after its deletion/storage calls
have returned, the module uses conditional fence acquisition and absorbs pending
native process-signal barriers. It requires the original exit interrupt holdoff
of one and no critical section, and never resets interrupt counters or processes
cancel/termination requests. This follows the native
[process-signal barrier](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/storage/ipc/procsignal.c)
and [storage reentrancy](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/storage/smgr/smgr.c)
constraints. Other exit-time publication boundaries fail explicitly.

An exiting publisher advertises shared intent while waiting. New lease requests
wait without holding the global read fence, and recheck intent after acquiring
it to close the admission race. Existing leases can finish. This prevents new
readers from starving a conditional writer that is not in the native lock queue.
A condition variable wakes readers; native exit/error cleanup clears the intent.
The exceptional exit path has a 100ms timed latch fallback for lock availability;
ordinary acquisition continues to use the native lock queue.

## Caller requirements and limits

A lease prevents catalog publication during its lifetime; it does not turn an
old repeatable-read data snapshot into a current catalog snapshot. The fixed
server catalog-read boundary under a fresh snapshot remains pending. This is
separate from the MySQL data-snapshot policy and savepoint row-lock retention,
which this module does not implement.

The stamp is only one part of a cache key. Physical/backend identity, route,
schema, SQL mode, charset/collation, time zone, parameter shape, installation
version and semantic dependencies remain required. Private facts cannot move
between owners. Prepared plans require the same dependency validation as text queries.

This mechanism coordinates ordinary PostgreSQL catalog utilities and native
object hooks on the supported primary versions. Direct system-catalog DML,
modules bypassing native hooks/publication boundaries, custom transaction
implementations and standby replay are outside its contract. The future executor
must not silently admit those paths. Dependency notifications alone are not a
replacement for this lease.

## Cost and evidence

The fence is deliberately broad: a reader can delay unrelated metadata DDL in
any physical database. Readers coexist, and ordinary DML does not acquire a
catalog writer fence. Maintenance utilities may conservatively serialize with
readers. A narrower fence requires its own complete dependency and publication
argument before replacing this mechanism.

The module runs no SQL during acquisition or release. It uses fixed backend
state, one backend-identity atomic allocation per participating connection,
ordinary native lock ownership and per-acquisition result allocation.
Two atomic intent reads are added to uncontended acquisition. Publication
inspection copies pending native invalidations only when present; prepared
commands add a session gate and fixed native catalog lookup/marker probes,
without adding SQL to ordinary lease acquisition or release. Local
lease numbers avoid a shared atomic allocation on every request. Separate fixed
begin/end queries cost two protocol round trips; an explicit check adds one.
The bounded sequential fixture measures those two calls, not proxy throughput,
cache hit rates, catalog-read cost or parallel contention.

Required native fixtures exercise actual fence grants/waits, simultaneous
readers, ordinary DML, external relation/schema/function/type/domain/collation
DDL, drop/recreate, metadata rollback, savepoint ownership, concurrent index
publication, cross-database hooks, enabled/default 2PC profiles, prepared DDL/DML,
rolled-back child DDL, mixed prepared row/catalog changes and normal temp-backend
exit during database removal. Missing module
dependencies fail. The Docker build installs the shared library, LLVM bitcode,
extension SQL/control files and Apache license; SDK tools stay in the build
stage. Other platform packages, hosted CI execution and the full artifact/
compatibility gates remain pending. Authentication, grants and other security
work remain deferred.
