# Supported stock PostgreSQL runtime

Status: selected architecture for v0.1.
This defines the native trust boundary and the remaining functional acceptance
gates. It does not certify a new collector or a table executor.

Darmok uses a correctly operating supported stock PostgreSQL server with the
Darmok extension and an admitted extension configuration. Current native source
and ordinary fixture targets are PostgreSQL 17.11 and 18.6. A supported server
provides its ordinary descriptor, cache, ResourceOwner, lock and MVCC services.
Darmok must use those services correctly and establish the additional semantics
its MySQL frontend and captured metadata require.

This decision supersedes the earlier requirement to certify every native cache
entry, reference owner, registration, compiled implementation and init-file
producer before using those services. The missing public census APIs remain a
valid source finding. An exhaustive integrity census is no longer an entry
mechanism or release gate. The selected design uses stock PostgreSQL and the
project extension; a supplied PostgreSQL build remains unselected.

## Native services and project responsibilities

| PostgreSQL supplies under the supported runtime contract | Darmok must establish |
| --- | --- |
| Correct native descriptor construction, invalidation and init-file restoration | Supported metadata and provider paths before dependent opens; fresh defining catalog carriers |
| Correct native reference accounting and phased owner release | Each Darmok acquisition, increment, owner, scan, lifetime and cleanup path |
| Correct builtin heap/btree implementations and compatible native registry structures | The selected native kind/AM/options path and neutral preparation in the admitted extension configuration |
| Native locks, MVCC and prepared transactions | Complete physical dependency acquisition, publication/recheck coverage and statement stability |
| Native transaction and portal APIs | Exclusive fixed-command history, exact control outcomes and failure disposition |

The supported-runtime premise is an interface dependency, not a result inferred
from a passing fixture, module pathname, linked pointer or ReadyForQuery. No
caller-controlled flag creates it. Published names and current pointer equality
remain limited observations, not original implementation certificates.

The initial production profile is stock native services plus Darmok's own
preload/utility/transaction paths. The existing probe and ordered-hook profiles
are separately scoped test configurations. Additional active extension profiles
need admission for the paths they add. An installed extension's unrelated
schema objects do not by themselves execute code during raw metadata copying;
selected type/function/AM paths still require semantic admission. This preserves
existing native schemas without promising every extension behavior.

An admitted extension configuration must preserve the functional assumptions
of the paths Darmok uses. A legitimate analysis, utility, GUC, resource-release,
cache, input, support or planner callback can execute SQL or select a data
snapshot. Correct native operation alone does not make that callback suitable
for neutral preparation. The implementation must identify and admit relevant
paths before depending on them, and reject unsupported configuration or metadata
explicitly. It must not rely on a later generation comparison to excuse earlier
evaluation or snapshot selection. Darmok's own utility and transaction callbacks
are required paths, so requiring every hook to be NULL is also incorrect.

Binary attestation and defense against arbitrary native-code corruption are
outside this functional contract and the user's current security-work scope.
This does not permit silently ignoring a detectable unsupported native path.

## Startup and inherited native caches

Ordinary successful native startup is the construction base for PostgreSQL's
own cache structures and intrinsic pins. It need not be free of native provider
work. The fresh private query owner subsequently establishes its own fixed
control history and confirmed native state. Setup and query connections remain
separate; a setup DO manifest or another caller's Client cannot enter that
history.

The paired init-file consumer replaces serialized transient reference counts
with one intrinsic pin for nailed descriptors or zero otherwise. It reconstructs
handler pointers and recalculates lock and storage addressing. PostgreSQL 18
also populates compact attributes. The producer writes a temporary file,
receives SI under RelCacheInitLock and publishes only when its invalidation
check permits it. Native pre/post invalidation functions serialize file removal
with that publication.

These source findings explain the native service being trusted. They are not a
new live owner census or an independent acceptance of the whole startup call
graph. Handler initialization can precede an unsuccessful load's return; the
selected boundary does not require proving that native startup had no effects.
PostgreSQL may reconstruct its caches when an init file is rejected.

Darmok's defining schema facts come from captured actual catalog carriers and
attributes, including raw NULL, empty and nonempty option images. Parsed
rd_options, cached defaults, intrinsic counts and compiled layout expectations
must not replace those facts. Native cached options belong to PostgreSQL's
descriptor implementation when its supported machinery uses them. The collector
must still admit the actual path it invokes. No fresh data directory, init-file
deletion or separate producer certificate is required for existing schemas.

## Darmok-owned references and fixed transitions

The project must account for every Relation reference it acquires, including
scan and directory increments, plus copied TupleDesc storage and native lock
references. Intrinsic pins and references owned by native machinery are not
invented Darmok acquisitions. Equal aggregate counts or owner pointers cannot
prove matching project acquisitions or cleanup.

The private connection and scoped C/Rust APIs must establish the following
transition argument:

1. Connection construction retains the client/driver exclusively, submits fixed
   lookup initialization, and checks complete control outcomes and readiness.
   It cannot adopt an arbitrary client or inherit setup history.
2. BEGIN and generated savepoint controls establish explicit native scopes.
   Supported analysis/utility/GUC paths must preserve the intended data view.
   Control completion alone does not prove that an unknown callback was neutral.
3. A catalog attempt records its own acquisitions and reader/snapshot lifetimes.
   Native increments and owner release are used according to their contracts;
   exact AccessShare references remain distinct from other lock modes.
4. Retry closes the whole attempt before another physical acquisition. Normal,
   native-error and subtransaction paths must close each owned scan/reference
   exactly once, with matching owner semantics. No passive all-owner census is
   substituted for this local lifetime argument.
5. Scope completion/recovery requires exact outcomes. Uncertain control, stream
   or cleanup results require disposal; observed readiness cannot repair an
   unconfirmed project scope.

The existing owner, collector and lease components establish only their recorded
scopes. New descriptor paths need current implementation review and ordinary
verification for this transition argument.

## Metadata admission, concurrency and execution

Finish the transitive relation/index/TOAST/catalog graph and capture all defining
bytes and identities used by binding. Include name and negative-candidate sets,
overloads, types/domains/ranges/enums/constraints and context where relevant.
Validate fresh observations, including same-owner changes. Expected native
layouts are checks against actual carriers, never replacement rows.

Before an operation can invoke an unsupported AM, descriptor branch, input,
support, domain or planner path, it must have the necessary admission. The
builtin heap/TOAST/btree options route can be admitted as a complete supported
native path; AS/RX does not freeze NULL options against SUE writers. Local
options/fillers and arbitrary AM callbacks remain separate functional paths.
The current collector's NULL-options restrictions stay implemented until that
generalization has its own proof and verification.

Close Darmok's catalog readers and snapshots before every new physical wait.
Acquire the complete dependency set before the semantic Share span. A newly
discovered dependency releases the whole attempt before reacquisition. Native
SI/cache work still has to preserve the admitted preparation context; the trust
boundary does not move a callback-capable or blocking project operation into
Share. End Share before callback-capable cleanup and row/data/XID waits.

Complete the relevant DDL and publication coverage, including PREPARE and native
prepared completion, without requiring max_prepared_transactions=0. Current
ordinary profiles use ten. Exact modes, ownership, ordering and retention must
prevent new lease-induced deadlocks; a stronger physical mode is not a substitute
for a required AccessShare reference.

Captured metadata must connect to immutable binding/planning and an explicit
execution boundary. After Share ends, prove how that meaning remains valid
through execution, decoding and frontend encoding. Reanalysis or newly discovered
dependencies cannot silently change it. Native correctness does not itself
freeze Darmok's captured names, types, functions or schema. The immutable
representation or retained protection must be stated and verified.

These requirements serve both MySQL applications using initialized PostgreSQL
databases and MySQL clients accessing existing native schemas. The architecture
does not require replacing the server or importing those existing databases.

## Costs, evidence and the next implementation

Measure the selected capture and execution paths: passes, catalog/TOAST bytes,
allocations, provider work, protocol round trips, retries, cache behavior and
lock contention. Native startup and descriptor work retain their own costs.
Using a stock API does not establish an unmeasured memory or latency bound.
Configured product bounds and native allocation failure must remain explicit.

The source-only record at logs/native-runtime-contract-primary-v1 binds two
previously archive-bound relcache inputs, eight complete selected producer,
consumer and pre/post-invalidation functions, and four paired differences.
Facts SHA256 is
44c3460fb4d99b7ada966cc000dc5b64b9e62aa0cb421fe7111cfe1068fa768c;
the 17-member seal is
1a278df35c02abc59bb91d16a5d20c74e3d7bc9a40951b52d496107376782b5f.
See the pinned [PG17 relcache source](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/relcache.c)
and [PG18 relcache source](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/relcache.c).
This supports the described interface decision; it makes no whole-file,
whole-call-graph, runtime or performance acceptance claim.

Next complete the supported pre-open descriptor/options argument, defining-writer
coverage and exact acquisition sequence, then implement the transitive collector
and connect captured metadata to binding/planning. Every observation needs a
named role in that mechanism. Current paired native packages, required ordinary
suites and independent implementation review remain necessary when native code
changes. Full table execution, serving, performance and release gates, issues
46/15 and the overall goal remain open. Standing exclusions remain in force.
