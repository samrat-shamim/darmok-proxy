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

Leases require a primary server and `max_prepared_transactions=0`. This is
PostgreSQL's [default native two-phase transaction setting](https://www.postgresql.org/docs/18/runtime-config-resource.html#GUC-MAX-PREPARED-TRANSACTIONS),
and is separate from MySQL prepared statements. A prepared DDL transaction can
retain table locks indefinitely; a leased row execution waiting for that table
would also block the fenced prepared completion. The selected first-release
profile excludes that dependency explicitly. Acquisition reports an error when
native 2PC is enabled and does not change the server configuration. The preloaded
module still chains native utility hooks in an unleased server with 2PC enabled.

WAL replay does not execute these utility hooks, so a standby cannot supply
this coordination mechanism. Counter exhaustion fails before effects; identities
never wrap. Each server start establishes a new 128-bit random incarnation, and
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

The future owning executor must hold the lease through catalog resolution,
prepare/bind, execution and output validation, then end it before finishing the
native scope. Dropped or failed scopes cannot assert a live lease. This API does
not grant semantic approval to arbitrary SQL or effectful native functions.

## Catalog publication

The module uses a synthetic cluster-wide object lock in PostgreSQL's default lock
method: database zero, `pg_extension` class, object zero and sub-ID `0x444d`. A
zero object OID cannot name an extension. The lock is distinct from user advisory
locks. Readers take `ShareLock`; metadata writers take `ExclusiveLock`.

The utility hook fences metadata utilities before their effects and advances the
generation. Known transaction/session/data-only utilities do not require this
fence. Unknown utility variants receive the stronger lock; this conservative lock
classification is not an SQL-admission fallback. Object create/alter/drop hooks
cover catalog mutations outside an enclosing metadata utility. Existing hooks
are chained, including object access events unrelated to metadata mutation.

A writer keeps the transaction fence through native catalog invalidation
publication and lock cleanup. A waiting reader consumes invalidations after
acquiring its fence. An aborted DDL attempt may advance the generation without
changing committed facts; this is a conservative cache miss.

Internally committing utilities, including concurrent index creation, receive a
writer fence and generation advance at each publication. Internal commits are
fenced by the pre-commit callback; a new final transaction still open when the
utility returns receives both before the outer commit. Thus the committed
ready-but-invalid index and its final valid state have distinct cache identities.
Readers can run between phases and
see PostgreSQL's committed intermediate catalog state. Holding one exclusive
session fence across all phases is incorrect: the utility can wait for an old
reader snapshot while that reader waits for the fence. The publication boundary
follows PostgreSQL's [transaction implementation](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/transam/xact.c),
where invalidations precede transaction-lock cleanup. Callbacks after commit do
not throw or reacquire locks. A catalog-only prepared-transaction fixture is not
evidence of safe leased row execution through prepared DDL.

## Caller requirements and limits

A lease prevents catalog publication during its lifetime; it does not turn an
old repeatable-read data snapshot into a current catalog snapshot. The fixed
server catalog-read boundary under a fresh snapshot remains pending. This is
separate from the MySQL data-snapshot policy and savepoint row-lock retention,
which this module does not implement.

The stamp is only one part of a cache key. Physical/backend identity, route,
schema, SQL mode, charset/collation, time zone, parameter shape, installation
version and semantic dependencies remain required. Private facts cannot move
between owners. Prepared plans require the same live lease as text queries.

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
ordinary native lock ownership and per-acquisition result allocation. Local
lease numbers avoid a shared atomic allocation on every request. Separate fixed
begin/end queries cost two protocol round trips; an explicit check adds one.
The bounded sequential fixture measures those two calls, not proxy throughput,
cache hit rates, catalog-read cost or parallel contention.

Required native fixtures exercise actual fence grants/waits, simultaneous
readers, ordinary DML, external relation/schema/function/type/domain/collation
DDL, drop/recreate, metadata rollback, savepoint ownership, concurrent index
publication, cross-database hooks and explicit profile errors. Missing module
dependencies fail. The Docker build installs the shared library, LLVM bitcode,
extension SQL/control files and Apache license; SDK tools stay in the build
stage. Other platform packages, hosted CI execution and the full artifact/
compatibility gates remain pending. Authentication, grants and other security
work remain deferred.
