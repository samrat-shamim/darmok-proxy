# Snapshot-neutral catalog discovery

Status: implemented and locally verified on PostgreSQL 17.11/18.6 under the
continuous private-owner profile below. The current
[reference scopes](native-bootstrap-references.md) have paired package and
ordinary-suite receipts at 62deefe017e947bfb2dba454ea36fb9ea5f2f662, bound to
unchanged native inputs built at 7abbcfd9fd385fc97c9dc58ab6859aa65835f647.
Independent fresh review is a merge gate. Revision and executed checks are
recorded in [release-plan.md](release-plan.md).
This component reads immutable native relation facts through the private
`NativeScope`. It does not admit, prepare or execute table queries. A returned
stamp is a historical observation, not an execution lease.

## Request and response

Inside an explicit owned transaction or savepoint, the owner sends one fixed
simple-query request:

```sql
SET LOCAL darmok_server.catalog_request_v1 = E'[["public","items"]]';
SHOW darmok_server.catalog_request_v1;
```

The value is JSON containing literal two-string namespace/relation pairs.
Names retain case, spaces, dots, quoting characters and Unicode bytes. There
is no search-path, `pg_temp` alias, truncation or normalization. Temporary
objects require their actual native namespace name. Input order and repeated
pairs are retained. The first missing pair fails the whole operation with its
zero-based index, before any result row is emitted.

SET is inert. Assignment, reset, startup processing and Describe never read
catalogs. The module handles only top-level SHOW of the exact protocol-1 setting.
It verifies the per-database extension version `1.0` and fixed `darmok_server`
namespace. A missing module can echo a placeholder GUC, but that value cannot
decode as an observation. Native SHOW with the default `[]` returns a header
without the four fact scans; the Rust scope's empty input instead performs no
SQL and returns `None`, without a stamp.

SHOW emits one native TEXT column named `darmok_server.catalog_request_v1`.
The JSON object contains `protocol: 1`, `stamp`, `relation_oids`, `relations`
and `types`. The stamp contains a 32-digit lowercase hex server incarnation,
physical database OID, backend identity, global generation and private local
generation. Counters are positive and fit native signed int8. Relations,
active user columns and declared types preserve the raw facts specified in
[native-catalog.md](native-catalog.md), including complete domain ancestry.
Relations and types are deduplicated; the resolved OID vector retains every
request. Unknown codes, duplicate fields or identities, contradictory names,
missing/extra facts and cyclic domain ancestry fail decoding. Decoding proves
representation only; it does not establish the source connection or freshness.

The owner requires the complete sequence SET, exact TEXT description, one
non-NULL row, SHOW and ReadyForQuery(Transaction). It drains ordinary backend
errors to readiness and never exposes a partial row. Only a requested missing
relation or exhausted pre-effect generation attempts, confirmed in a failed
transaction without a mismatch/transport error, permit explicit scope recovery.
Local request limits fail before submission and preserve the scope. Other
native errors, including missing/incorrect installation, unsafe boundaries,
budget/identity exhaustion and unsupported profiles, leave the owner uncertain.
Submission, transport, shape and decoding failures also leave it uncertain;
finish and recovery then reject reuse. An uncompleted read guard also leaves
only disposal available. This adds no asynchronous rollback or reset.

## Native phases and two-phase transactions

The one-shot reader uses the publication mechanism in
[server-catalog-lease.md](server-catalog-lease.md):

1. With no seed, catalog reader, scan or snapshot alive, acquire publication
   gate Share then global Share and observe global/private generation. End this
   reader scope and release global and gate before physical acquisition.
2. Acquire a transient seed with exact AccessShare references for the four
   fixed heap catalogs and two existing critical class/attribute indexes.
   Outside Share and reader bookkeeping, consume native invalidations, refresh
   catalog snapshot state, verify installation, open the four readers with
   NoLock and allocate byte-key maps. A prepared transaction may retain one of
   the seed's native physical tags. No reader crosses that seed acquisition.
3. Begin a new reader scope, try gate Share then global Share without a lifecycle
   CV wait, and compare generations. Changed generation or shared-drop intent
   ends the scope, closes all readers/snapshot and releases the complete seed
   before restart or another initial lifecycle wait. Sixteen unsuccessful
   attempts produce a native serialization error.
4. Under unchanged Share, register one refreshed nonhistoric catalog snapshot
   and directly scan the builtin heaps `pg_namespace`, `pg_class`,
   `pg_attribute` and `pg_type`. Copy only fixed tuple prefixes. This span uses
   no index/syscache/TOAST lookup, SQL, SPI, user table AM/operator/receiver,
   invalidation dispatch or user relation/tuple/transaction-ID lock request.
5. Release global Share then gate Share before ending scans, unregistering and
   invalidating the catalog snapshot, closing descriptors with NoLock and
   releasing exact seed references. Select copied domain ancestors, serialize
   and emit output afterward. Ordinary ERROR cleanup releases raw bookkeeping
   first and requires matching native abort. Shared-drop lifecycle waits carry
   neither a seed nor reader/scan/snapshot references, including after a race.

No reader fence survives SHOW. A suspended native SHOW portal retains an old
materialized observation; resuming it does not rediscover or renew a lease.
COMMIT/ROLLBACK PREPARED can finish while the reader waits for a catalog heap
lock outside Share. Prepared user DDL remains private until completion, then
the next discovery sees current metadata even in an existing repeatable-read
data view. Neither behavior requires native 2PC to be disabled.

## Continuous backend profile

Snapshot neutrality is conditional on the private owner retaining a supported
builtin native backend history. It creates and owns both fresh connector halves,
accepts no arbitrary Client, exposes no raw SQL and permits no escaped user
portals or positive user-relation descriptor references. Compatibility
installation occurs in a separate idle initialization transaction and commits
before opening a discovery scope. Future admission must maintain this invariant
continuously, including uncommitted relcache/create/relfilenode state.

Moving invalidation processing outside Share is necessary but insufficient:
an existing custom table-AM descriptor can invoke its handler during rebuild,
and that handler can issue SQL and select the first data snapshot. Arbitrary
backend histories, custom AMs and extension callbacks are not certified by this
reader. SHOW also requires an unused physical/semantic invocation boundary;
it cannot execute through another live guard. It rejects historic/parallel or
unsafe publication boundaries and
checks that the native `FirstSnapshotSet` flag has not changed during discovery.
A detected profile failure is an error requiring owner disposal; the module
never resets or repairs native snapshot state. This check is not a universal
proof about callbacks that ran before the utility hook.

The reader leaves the first native data snapshot unselected under this profile.
It does not implement MySQL read-view selection, locking reads or savepoint lock
retention. Facts can become obsolete immediately after Share is released.
Complete native dependency guards, fresh recheck, supported preparation paths,
semantic admission and result validation remain required for statement execution.

## Costs, bounds and fixtures

A nonempty owner request has one SET/SHOW round trip. An uncontended successful
native attempt has four short Share acquisitions (two gate and two global),
six seed AccessShare acquisitions and four full heap scans. Its NoLock readers
have their own descriptor and scan increments. Shared-drop admission retries
can add acquisitions in the initial lifecycle phase;
generation changes restart the attempt and add acquisitions and preparation.
These counts are not a bound on a request that encounters contention. It copies
all namespaces/types and only requested relations/columns, then selects domain
ancestors in memory. Cost scales with catalog size, not just request count;
there is no per-object index optimization or throughput/cache claim.

Requests are limited to 1 MiB of JSON and 4096 pairs. The owner checks size while
encoding, before submission; E-literal escaping can make its SQL string larger.
The native attempt checks copied-map allocation against 64 MiB periodically
and after reading/closure, and bounds serialized length below 64 MiB before
buffer growth. Serialized-context allocation is checked before receiver work.
These are phase budgets, not a hard total process-memory bound: request parsing,
simultaneously retained maps/result, allocator overhead, native TEXT/transport
copies and Rust decoding consume additional memory. Periodic checks can
overshoot before failing. Limits fail loudly without truncated success.

Required ordinary fixtures cover first/existing data views, exact facts and
literal names, private/savepoint/temporary catalog state, prepared catalog and
user-relation locks, request restoration/errors/limits, per-database module
proof, inert Describe, historical portals and a bounded sequential cost report.
Private-owner fixtures additionally check both scope boundaries, recoverable
missing-relation errors and rejection of both missing per-database installation
and an installed but unpreloaded module's placeholder echo. Fresh PostgreSQL
17/18 product/probe images, three profiles per version and current-revision
fixtures have local executed receipts in the release plan. No stress, forced
interruption, security work or hosted CI claim belongs to these fixtures.
