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
drop hook registers transaction-owned reader exclusion after database lookup/
locking. A short exclusive fence drains existing leases and advances generation
before that change, then releases the lock. New leases remain excluded until
native commit/abort. A tablespace drop uses the same mechanism before irreversible
directory removal. This keeps the fence available to metadata publishers while
native DROP waits for backend retirement or storage barriers. Native lifecycle
checks and their errors retain their original order. Database moves retain native
database locks across their file-copy and internal commit; their transactional
catalog publication receives the ordinary commit fence.

## Prepared transactions

PREPARE keeps metadata private. It rejects an active catalog lease or shared-drop
intent and releases any transaction publication fence before native two-phase
lock transfer. Private metadata invalidates the backend's local identity when
preparation completes. The module adds no marker locks, GID hash gates or native
completion-tag capture; PostgreSQL retains its normal preparation lifecycle and
completion tags.

Every SQL COMMIT PREPARED or ROLLBACK PREPARED that reaches the utility hook takes
the transaction-owned publication fence and advances generation before native
completion. This includes pure DML, rolled-back child DDL and attempts that later
fail native target checks. PostgreSQL alone resolves the exact GID and performs
its database, ownership, uniqueness and busy checks. A queued PREPARE is not a
valid target until native core makes it valid. The module does not inspect
`pg_prepared_xacts`, resolve text operators, copy private prepared-transaction
structures or classify retained locks. In particular, a target can retain an
exclusive lock on the prepared-transaction view without blocking its own finish
on a classifier query.

The fence remains owned by the finishing transaction through native completion
and transaction cleanup. Native errors release it through ordinary resource
ownership; they may conservatively invalidate generation. Prepared completions
serialize with catalog readers even when they publish no metadata. Their fence
duration includes native WAL/file work and any native completion waits, so it has
no fixed short-duration guarantee. The fixtures use normal completion and do not
run restart or interruption experiments.

The creation/completion ordering covers native SQL utility entry. Direct calls
to `FinishPreparedTransaction` by custom modules bypass that hook and are outside
the metadata-publication contract. PostgreSQL logical replication also has direct
native callers; native replication is not a schema/DDL publication integration
provided by this module.

A prepared transaction can alter table A while holding row locks in unrelated
B. A query waiting on B while holding the global lease would block completion
of that transaction. Prelocking B alone does not solve this cycle. Ending the
global lease before row execution is mandatory even with complete dependencies.

## Shared drops and backend exit

Native temporary-table cleanup publishes before the exiting backend retires its
process-signal slot. Holding the global fence across a database drop's backend
drain would prevent that cleanup from finishing; holding it across a storage
barrier would likewise prevent retirement. Reader exclusion therefore belongs
to shared-drop intent, rather than a writer lock spanning those native waits.

Intent is registered before draining old leases. New lease requests wait on a
condition variable without holding Share, and recheck the shared count after
acquiring Share to close the admission race. An intent keeps its earliest owning
subtransaction, promotes on subcommit and clears on owning subabort or top-level
commit/abort. Native exit cleanup also clears it. Multiple droppers each own one
intent; completion of one cannot reopen admission while another remains. A
backend owning intent cannot acquire its own read lease or transfer it to PREPARE.

Native pre-commit reacquires and retains the ordinary publication lock through
invalidation/lock cleanup, even when a shared drop has already advanced generation
before irreversible effects. At commit, clearing intent wakes readers, but that
lock still prevents acquisition before native invalidations are delivered.
Aborts may conservatively advance generation. Both ordinary and exit-time
publishers use PostgreSQL's native lock queue; the module has no exit-time polling,
barrier processing or interrupt-counter adjustment. The supported native database/
tablespace paths perform storage-barrier waits outside those publication locks;
custom late callbacks remain outside the declared contract.

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
Two atomic intent reads are added to uncontended acquisition. Shared drops add
one transaction-owned admission count, a short native drain lock and the ordinary
pre-commit publication lock; ordinary exit publishers need no special wait loop.
Publication inspection copies pending native invalidations only when present.
Prepared completion adds one publication lock and generation advance, without
classifier SQL, GID allocations or protocol round trips. All prepared completions
contend with readers and conservatively invalidate catalog cache identities,
including pure DML; ordinary pure-DML commits retain their previous behavior.
Local
lease numbers avoid a shared atomic allocation on every request. Separate fixed
begin/end queries cost two protocol round trips; an explicit check adds one.
The bounded sequential fixture measures those two calls, not proxy throughput,
cache hit rates, catalog-read cost or parallel contention.

Required native fixtures exercise actual fence grants/waits, simultaneous
readers, ordinary DML, external relation/schema/function/type/domain/collation
DDL, drop/recreate, metadata rollback, savepoint ownership, concurrent index
publication, cross-database hooks, enabled/default 2PC profiles, prepared DDL/DML,
rolled-back child DDL, mixed prepared row/catalog changes, exact case-distinct
GIDs under a schema-local text operator, prepared view locks and normal temp-backend
exit in the target or an unrelated database during removal, plus concurrent drop
intents and ordinary native busy-error cleanup. Missing module
dependencies fail. The Docker build installs the shared library, LLVM bitcode,
extension SQL/control files and Apache license; SDK tools stay in the build
stage. Other platform packages, hosted CI execution and the full artifact/
compatibility gates remain pending. Authentication, grants and other security
work remain deferred.
