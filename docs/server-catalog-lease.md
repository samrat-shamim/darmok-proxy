# Server catalog lease

`postgres/darmok_server` implements PostgreSQL 17/18 catalog publication and a
one-shot reader with internal, transaction-owned read fences. The reader and
private Rust scope integration have prior local PostgreSQL 17.11/18.6 evidence
under the continuous private-owner profile. The publication gate and prepared
utility completion repair below require fresh current-revision verification;
prior reader evidence does not certify the revised publication ordering.
Statement admission, complete dependency guards, result definitions, plan
caching and a MySQL table executor remain required before exposing table SQL.
Component evidence is recorded in the release plan.

## Installation and identity

Build against the selected server's PGXS development files, preload
`darmok_server` at server startup, and explicitly `CREATE EXTENSION darmok_server`
in each selected physical database. Late loading is an error. The nonrelocatable
extension has version `1.0`; its namespace is `darmok_server`, separate from the
SQL compatibility functions in `darmok`. Current `init`/`schema verify` cover only
the latter. Combined installation and exact module verification remain pending.

Discovery requires a primary server. Concurrent native PostgreSQL two-phase
transactions are part of the v0.1 target; `max_prepared_transactions` may be
nonzero. This is separate from MySQL prepared statements and MySQL XA support.
The module does not change the server configuration.

WAL replay does not execute these utility hooks, so a standby cannot supply
this coordination mechanism. Counter exhaustion fails before publication, including before irreversible
shared-drop effects; identities never wrap. Each server start establishes a new 128-bit random incarnation, and
each participating backend receives an increasing identifier. Persistent cache
identities cannot assume generations survive a server restart or reuse a backend
identifier without its incarnation.

## Reader API and ownership

[Catalog discovery](catalog-discovery.md) uses inert SET LOCAL followed by
top-level SHOW. The module returns immutable facts and a publication stamp; it
exports no begin/check/end lease functions or frontend handle. Native resource
ownership is confined to one SHOW invocation. All Share spans end before heap,
snapshot and descriptor cleanup, serialization and receiver work, including
ordinary ERROR paths. A suspended SHOW portal keeps historical facts only.

The private local generation invalidates facts on metadata rollback and native
transaction prepare; metadata-free transactions preserve that identity. Global
and local generations are cache inputs, not proof of later statement validity.
Moving a fact between backends cannot preserve its private ownership.

The future statement sequence must:

1. Discover current dependencies through the fixed one-shot reader.
2. Acquire complete native relation/object dependency guards outside the global
   fence. These waits may include prepared transactions.
3. Recheck fresh metadata and dependency closure. An added or changed dependency
   requires release/restart before effects; no guard may be acquired under Share.
4. Prepare and validate only proven nonblocking paths inside a controlled native
   boundary, then release any global fence before row execution.
5. Retain dependency guards through execution and output validation.

The reader supplies only the first component and fresh facts for future recheck.
The prepare/admission boundary is not implemented. PostgreSQL planning can
execute user/support functions; planning is not inherently safe under Share.
The global fence cannot protect a running plan. Native object replacements
without sufficient native locking need a verified guard integration.

## Catalog publication

The global fence is a synthetic object lock in PostgreSQL's default lock method:
database zero, `pg_extension` class, object zero and sub-ID `0x444d`. A zero object
OID cannot name an extension. Readers take `ShareLock`; short ordinary drains
and prepared completions take `ExclusiveLock`. A distinct publication gate uses
the same native tag fields with sub-ID `0x444e`. Readers take gate `ShareLock`;
ordinary publishers retain gate `RowExclusiveLock`. User advisory locks use a
separate lock method. The publisher modes coexist; the reader mode conflicts
with them. Neither tag is an actual extension or a prepared-transaction marker.

Ordinary metadata utilities mark the backend's private catalog identity before
attempting changes. At native pre-commit, after the utility's normal dependency
locks/effects and deferred triggers, they acquire the publication gate before a
short global drain. The module
inspects PostgreSQL's pending transactional invalidations through
[`xactGetCommittedInvalidationMessages`](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/inval.c),
which does not consume them. Native subabort removes a child's messages. A
rolled-back child DDL or a no-effect utility does not become a surviving metadata
publication merely because it was attempted. Private identity may still advance
conservatively. Ordinary DML without catalog publication takes no writer fence.

The pre-commit callback precedes native ON COMMIT actions. Rebuilding an existing
indexed temporary table on `ON COMMIT DELETE ROWS` can wait for `pg_class` after
that callback. Retaining global Exclusive across this wait would prevent a
prepared holder from finishing and releasing the catalog lock. The publisher
therefore retains only its transaction-owned gate through late native work:

1. Acquire gate RowExclusive outside the global fence.
2. Acquire global Exclusive, drain admitted raw observers, advance generation,
   and immediately release that exact owned reference, including ordinary ERROR.
3. Run native ON COMMIT work, catalog waits and invalidation delivery under the
   gate alone. Prepared completion can still acquire global Exclusive.
4. Let native transaction lock cleanup release the gate after invalidations.
   Resetting bookkeeping at the COMMIT callback must not release it early.

Late core temporary-object drops use the same ordering through object hooks.
Additional changes while the gate is held need no second global drain. Arbitrary
extension callbacks need their own complete ordering proof. Physical statistics,
storage mappings and sequence values are not immutable schema dependencies;
this mechanism does not freeze those non-MVCC values.

Each reader span acquires gate Share before global Share and releases global
before gate. Both initial generation observation and raw fact scanning use this
order. Both locks are released between spans before consuming native
invalidations and preparing descriptors, and before native cleanup/output.
Concurrent publishers can advance generation in a different order from their
commits: readers remain excluded until all conflicting gates have been released
after native invalidations. Aborts may conservatively invalidate identities.
Internal commits, including concurrent index phases, each get a new generation.
The final valid-index transition receives its own publication gate/drain.
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

PREPARE keeps metadata private. It rejects reentry during discovery or shared-drop
intent or active completion and releases any tracked publication gate before
native two-phase lock transfer. Private metadata invalidates the backend's local identity when
preparation completes. The module adds no marker locks, GID hash gates or native
completion-tag capture; PostgreSQL retains its normal preparation lifecycle and
completion tags.

Every SQL COMMIT PREPARED or ROLLBACK PREPARED that reaches the utility hook takes
an invocation-owned global Exclusive reference and advances generation before
native completion. It never acquires the publication gate. This includes pure DML, rolled-back child DDL and attempts that later
fail native target checks. PostgreSQL alone resolves the exact GID and performs
its database, ownership, uniqueness and busy checks. A queued PREPARE is not a
valid target until native core makes it valid. The module does not inspect
`pg_prepared_xacts`, resolve text operators, copy private prepared-transaction
structures or classify retained locks. In particular, a target can retain an
exclusive lock on the prepared-transaction view without blocking its own finish
on a classifier query.

Native `FinishPreparedTransaction` sends the target's saved invalidations and
releases the target's locks and state before returning. The utility invocation
releases its exact global reference in `PG_FINALLY` on return or ordinary ERROR,
before its caller's own PRE_COMMIT, ON COMMIT or abort cleanup. That caller may
have earlier native Parse/Bind temporary-table history without a Sync; native
transaction-block checks do not prove an empty cleanup history. Its own later
publication uses the normal gate-first protocol. This avoids a global-to-gate
inversion and permits a second prepared holder to release a late catalog wait.

Early native errors may conservatively invalidate generation. An irreversible
native finish must not be reported as an invented rollback or retried through a
custom callback failure. Prepared completions serialize with catalog readers
even when they publish no metadata. Their global span includes native WAL/file
work and completion waits, with no fixed short-duration guarantee. Fixtures use
normal completion without restart or interruption experiments.

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
condition variable without either lock, and recheck the shared count after
acquiring gate Share then global Share to close the admission race. If the
recheck finds intent, release both before waiting again. Sleeping while retaining
gate Share could block the exiting publisher that DROP needs to retire. An intent keeps its earliest owning
subtransaction, promotes on subcommit and clears on owning subabort or top-level
commit/abort. Native exit cleanup also clears it. Multiple droppers each own one
intent; completion of one cannot reopen admission while another remains. A
backend owning intent cannot acquire its own read lease or transfer it to PREPARE.

Native pre-commit acquires the ordinary publication gate and performs a short
global drain, even when shared drop already advanced generation before
irreversible effects. It retains the gate through invalidation/lock cleanup.
At commit, clearing intent wakes readers, but the gate still prevents observation
before native invalidations are delivered.
Aborts may conservatively advance generation. Both ordinary and exit-time
publishers use PostgreSQL's native lock queue; the module has no exit-time polling,
barrier processing or interrupt-counter adjustment. The supported native database/
tablespace paths perform storage-barrier waits outside those publication locks;
custom late callbacks remain outside the declared contract.

## Caller requirements and limits

An internal Share span prevents publication only while held. The fixed reader
uses a fresh native catalog snapshot independent of an old repeatable-read data
view and leaves no live fence after returning. Snapshot neutrality requires its
continuous private-owner builtin native profile; arbitrary backend histories
are not certified. MySQL data-snapshot policy and savepoint row-lock retention
remain separate, unimplemented gates.

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

The module runs no SQL during internal acquisition or release. It uses fixed
backend state, one backend-identity atomic allocation per participating connection
and ordinary native lock ownership.
Two atomic intent reads are added to uncontended acquisition. Shared drops add
one transaction-owned admission count, a short native drain lock and the ordinary
pre-commit publication gate/drain; ordinary exit publishers need no special wait
loop. Ordinary publication adds one retained gate acquisition; the global drain
ends before late cleanup. Gate contention spans that cleanup across databases.
Publication inspection copies pending native invalidations only when present.
Prepared completion adds one publication lock and generation advance, without
classifier SQL, GID allocations or protocol round trips. All prepared completions
contend with readers and conservatively invalidate catalog cache identities,
including pure DML; ordinary pure-DML commits retain their previous behavior.
The one-shot reader needs one SET/SHOW round trip per nonempty request. An
uncontended successful attempt has four Share acquisitions (two gate and two
global) and four full fact-heap scans. Shared-drop admission retries add acquisitions; a changed
generation restarts the attempt and adds acquisitions and preparation. Its
[phase budgets and sequential fixture](catalog-discovery.md) do not establish
proxy throughput, cache hit rates or parallel contention.

Required publication fixtures use a separately built test-only native Share
probe; no probe module or long-held reader API is installed in the product
image. They exercise actual fence grants/waits, simultaneous synthetic readers,
ordinary DML, external relation/schema/function/type/domain/collation DDL,
drop/recreate, metadata rollback, concurrent index publication, cross-database
hooks, prepared DDL/DML,
rolled-back child DDL, mixed prepared row/catalog changes, exact case-distinct
GIDs under a schema-local text operator, prepared view locks and normal temp-backend
exit in the target or an unrelated database during removal, plus concurrent drop
intents and ordinary native busy-error cleanup. Missing module
dependencies fail. The Docker build installs the shared library, LLVM bitcode,
extension SQL/control files and Apache license; SDK tools stay in the build
stage. Other platform packages, hosted CI execution and the full artifact/
compatibility gates remain pending. Authentication, grants and other security
work remain deferred.
