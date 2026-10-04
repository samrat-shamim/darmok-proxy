# Type metadata descriptor preparation

Status: proposed supported path and acquisition sequence. Independent source
review must accept this mechanism before collector C changes. No new descriptor,
runtime result or release gate is accepted by this proposal.

The [stock runtime contract](native-runtime-contract.md) remains the construction
base. PostgreSQL supplies correct cache, descriptor, reference and native lock
services in the admitted configuration; Darmok owns its acquisitions, pre-open
checks, neutral preparation and copied defining facts. Concurrent native
PostgreSQL two-phase transactions remain required.

## The finite descriptor frontier

The definition collector will read the existing six metadata heaps plus
`pg_range` (3541), `pg_enum` (3501) and `pg_constraint` (2606).
Constraint payloads add declared TOAST 2832. Range and enum have no declared
TOAST. A catalog's compiled identity is an expectation checked against actual
rows; it does not supply an invented relation, descriptor or locator.

Metadata discovery uses only the already admitted namespace, class, index,
attribute and type readers. It copies enough actual facts to construct all nine
metadata heaps' complete physical graphs, including declared TOAST and every
live index from `pg_index`. Conservative `relhasindex` hints never select that
membership. The two existing critical class-OID and attribute-ordinal indexes
remain the supported native lookup services. Newly discovered noncritical
indexes receive physical references; this collector does not open their
descriptors or invoke their AM/provider paths.

Application composites stay copied declarations. This mechanism does not open
a descriptor for a selected nonroot composite. Range opclasses/functions, enum
input and domain CHECK expressions remain separate unresolved execution
obligations.

## Initial admission base and preservation

Discovery D needs readers before a fresh profile exists. Its admission is an
explicit inherited native interface precondition, not a conclusion from D.
The supported configuration provides the original builtin catalog descriptor
contract for these six relations and the two initialized native lookup indexes:

| Bootstrap object | Native identity | Inherited descriptor path |
| --- | --- | --- |
| `pg_namespace` | 2615 | Permanent nonshared builtin catalog heap |
| `pg_class` | 1259 | Permanent nonshared builtin catalog heap; no declared TOAST for class carriers |
| `pg_index` | 2610 | Permanent nonshared builtin catalog heap |
| `pg_attribute` | 1249 | Permanent nonshared builtin catalog heap |
| `pg_type` | 1247 | Permanent nonshared builtin catalog heap |
| `pg_attrdef` | 2604 | Permanent nonshared builtin catalog heap |
| Class-OID lookup index | 2662 | Original initialized builtin-btree service on `pg_class` |
| Attribute-ordinal lookup index | 2659 | Original initialized builtin-btree service on `pg_attribute` |

The base includes each heap's native per-major positive attribute ABI, builtin
heap AM, original catalog namespace and kind, and absence of attribute defaults,
missing values, stored/virtual generation, declared CHECKs, rules and triggers.
Ordinary catalog NOT NULL declarations remain present. Raw class options have
the builtin text-array carrier layout; NULL, empty and nonempty values use the
complete admitted global options path below. The critical indexes retain their
original native ownership, key/layout/support contracts and initialized builtin
options path. A bootstrap open enters no application provider, expression
loader, local options registry or selected payload's TOAST reader.

Successful ordinary native startup establishes the service base before the
private query owner begins. Paired `RelationCacheInitializePhase3` either
restores native init-file data or uses `formrdesc` for critical class, attribute,
procedure and type catalogs; it then initializes critical indexes and finishes
native cache setup. This does not mean all six readers are hardwired or nailed.
Namespace, index and attrdef cold opens depend on the explicit original builtin
catalog contract above. Native startup may perform ordinary provider work; it
is not part of Darmok's subsequently neutral query preparation.

The [stock runtime contract](native-runtime-contract.md#startup-and-inherited-native-caches)
supplies correct native construction, restoration, invalidation and reference
services. The added finite bootstrap prerequisite supplies the selected original
catalog path. A warm cache, `criticalRelcachesBuilt`, module pathname, caller
Boolean or later generation comparison cannot create it. D still copies and
checks actual rows; compiled identities/layouts never replace defining facts.

The admitted operations preserve this base between attempts as well as while
guards are owned. Ordinary application DDL and its catalog row writes do not
alter these builtin relations' descriptor declarations. Statistics and
clearing-only hint maintenance do not enable missing/default/CHECK/rule/trigger
branches. Supported options changes stay within the admitted global heap/btree
parser and compatible definition/parse-table contract. Supported native index
maintenance preserves the original critical service through its native reload
and restoration contract; locators are always observed afresh. A configuration
that changes builtin catalog schema, loaders, AM/support identity or registry
correspondence is outside this functional bootstrap contract. Direct writes or
extension paths that make such changes cannot be classified as ordinary
application catalog writers. This is an interface precondition, not a new
admission policy or an integrity census.

Before any project refresh, invalidation, increment or open in D, acquire exact
AS and RX for every one of the eight named seed objects: 16 independently owned
OID/mode references in the existing native order. Acquisition uses their native
fixed identities and opens no descriptor. There is no AS-only reader window in
this proposal. The same AS/RX seed is part of every expanded metadata and full
physical attempt. A missing seed reference errors before preparation.

Native SI delivery inside a lock acquisition uses the already admitted native
service/configuration base. The acquisition does not itself certify neutrality
or create that base. No Darmok reader, scan or registered catalog horizon is
alive while the seed is being assembled; explicit project refresh/invalidation
and reader preparation begin only after the complete seed ledger is owned.

The following transition argument is separate from a post-open check:

1. Entry inherits the named native descriptor/lookup base and closed functional
   configuration before any project reader exists.
2. AS excludes AX layout/default/missing/generation/AM, rename, drop and rewrite
   work on the seed. RX also excludes SRX trigger work and Share maintenance
   while the seed is held. The selected ALTER dispatcher supplies these
   loader-changing modes; `CreateTriggerFiringOn` explicitly takes SRX before
   changing a relation's trigger state. The older AS-only seed cannot supply
   this argument.
3. Compatible SUE options/statistics paths remain admitted. Paired
   `vac_update_relstats` changes counters/frozen IDs and can only clear the
   relevant conservative hints; it cannot enable false rule/trigger hints.
   Ordinary RX catalog row writes remain compatible. Their defining application
   facts still need coherent observations and publication/private/context rechecks.
4. A cold/rebuilt heap uses the admitted class/attribute lookup paths.
   `ScanPgRelation` opens class with AS and uses its initialized class-OID
   service; `RelationBuildTupleDesc` uses the attribute-ordinal service. Both
   exact AS references are already owned. The builtin catalog table-AM branch
   selects the original heap handler rather than a captured application AM.
   Absent loader declarations prevent additional default/missing/CHECK/rule/
   trigger reader branches. Native increments have their native matching close
   paths; they do not transfer seed ownership.
5. Critical-index reload retains initialized support state, copies the current
   class row, parses admitted options and refreshes the physical address.
   Reloading the class-OID index uses a class heap scan to avoid recursion.
   These system indexes skip the non-system `pg_index` refresh branch. Project
   cache operations and opens stay outside raw/S; native cache services retain
   their admitted contracts.
6. D must agree with the inherited path and discover the complete current
   physical graph. Disagreement errors; it neither repairs native state nor
   retrospectively admits the earlier open. Close D completely before seed
   release. The closed admitted operations preserve the base through that
   reader-free gap. Reacquire the full graph before P_A opens; P_A/P_B/P_C
   admit the new dependent descriptors from fresh actual facts.

The finite initial readers use this separately stated inherited source-admitted
base. Every descriptor beyond it needs a fresh preparation profile before entry.
This exception does not extend to application descriptors, new catalogs, TOAST
or arbitrary indexes, and does not turn compiled bootstrap layouts into copied
defining schema facts.

## Pre-open profile

For each dependent descriptor beyond the finite initial base, a fresh coherent
preparation observation copies its actual class, positive attributes, relevant
type rows, index membership and selected class option carriers. It owns its
copied bytes.
The profile must establish the supported namespace, permanent nonshared kind,
builtin heap or already admitted critical-btree path, declared physical identity,
exact positive ordinals and the fields that select native descriptor loaders.

Check each ordinal's actual name, type identity and native length/by-value/
alignment layout against the per-major compiled ABI. Preserve actual NOT NULL
and other declarations rather than substitute compiled rows. Require absence of
defaults, missing values and stored/virtual generation in these metadata
descriptors, including the actual missing carrier's NULL state. Require zero
declared CHECK count and absence of unsupported rule/trigger/rewrite branches.
An unexpected layout or dependent path errors before its descriptor is opened.

The existing TOAST profile's three actual chunk slots extend to constraint
TOAST 2832. Their type/length/alignment and absence checks stay mandatory.
The critical index prerequisites remain exact actual ownership plus the
supported initialized native service; a cached pointer or nailed flag is a
postcondition observation, not the pre-open argument.

Both pinned `RelationBuildTupleDesc` bodies load defaults only when an actual
attribute declares them, and CHECKs only when the actual relation declares
them. PostgreSQL 18 additionally handles compact attributes and virtual
generation. Its NOT NULL loader is skipped for catalog relations when CHECK
count is zero, so ordinary catalog NOT NULL attributes must be accepted.
The fixed catalog profile prevents entering those unadmitted default/check
loaders; it does not invent an empty expression set for an application domain.

`RelationBuildDesc` selects the table-AM branch, parses options and conditionally
loads rules/triggers. Under this profile the catalog branch reaches the original
builtin heap implementation traced in [the bootstrap investigation](native-catalog-bootstrap.md#original-builtin-heap-dispatch).
Native relation/cache/mapping services keep their supported interface contracts.

## Complete supported options path

Admit the native global heap/TOAST options route and the already supported
critical-btree route. Preserve NULL, empty and nonempty defining option carriers
and their owned images independently from native parsed `rd_options`.
The class carrier uses the admitted no-TOAST `pg_class` layout. Unsupported
external targets or carriers remain explicit errors.

`RelationParseRelOptions` uses `extractRelOptions` with the native hardwired
class descriptor. The supported heap/TOAST kinds use `heap_reloptions`, then
`default_reloptions` and `build_reloptions`. Critical btree options use
`btoptions` and the same global builder. The selected kind, compiled parse table
and compatible global definitions must correspond in the admitted configuration.

The false-validation global route still reaches allocation/filling code.
It is not safe merely because validation is false. The existing paired source
trace establishes that builtin global string definitions are empty and global
string registration supplies no fill callback; local registration/fillers are a
different path. String validators are conditional on validation. The production
stock-plus-Darmok configuration and separately admitted test profiles must
preserve this global path and neutral callback assumptions. No private registry
census, passive pathname observation or caller Boolean supplies that admission.

Array deconstruction selects the builtin TEXTOID layout and copies text values;
it does not select a captured type's I/O provider. Native parsing and cache
allocation retain their own resource/error costs. This proposal does not parse
options into a competing Darmok cache or use native ignored/default values as
replacement defining catalog facts.

The complete options path also governs invalidation reloads and rebuilds.
PostgreSQL 17's active-entry clear path and PostgreSQL 18's separate rebuild
path can reach `RelationBuildDesc`; critical index reload parses options again.
All such project work stays outside raw observation and semantic Share.
The continuous supported path, fixed private history and matching project
references remain prerequisites before entry and through cleanup.

## Exact guards and writer classes

Use exact AccessShare plus RowExclusive for metadata roots, with the existing
physical graph's mode propagation to declared TOAST and live indexes. Preserve
one independently owned native reference per OID/mode after unioning aliases.
Neither mode substitutes for an AccessShare reference. No new native lock mode
is introduced.

AccessShare excludes native AccessExclusive layout, rename, drop and rewrite
operations. RowExclusive additionally excludes ShareRowExclusive trigger work
and Share index/schema operations relevant to the descriptor frontier.
Both remain compatible with ordinary RowExclusive catalog data writers.
ShareUpdateExclusive option/statistics/maintenance paths can still run:
options parsing is admitted rather than claiming NULL options remain frozen.

The selected ALTER lock dispatch must be checked field by field against the
profile. Defining layout/default/missing/generation changes and loader-enabling
flags require exclusion; statistics, benign option changes and conservative
hint maintenance need classification rather than a blanket writer claim.
`vac_update_relstats` updates native counters and can clear index/rule/trigger
hints. Heap descriptor preparation does not use the index hint as membership.
A false rule/trigger pre-open profile must remain valid against enabling writers;
a body inventory alone does not prove that preservation.

Index creation/reindex membership and locator changes use the existing complete
physical graph, fresh observations and publication/private rechecks. A new
dependency releases the whole attempt before another acquisition. Guards do
not freeze global definitions, arbitrary callbacks or library behavior.

This guard set does not serialize ordinary RowExclusive metadata writers with
one another. It does conflict with Share maintenance/index work. Retained
references can extend that wait until native transaction completion, which must
be recorded as part of the actual invocation footprint.

## Acquisition and observation sequence

A preparation observation is separate from a full definition observation.
It scans namespace/class/index/attribute/type maps and a selected class-options
pass under one registered nonhistoric catalog snapshot and one raw span.
Selection between fixed maps and carrier copying uses only owned bytes.
It performs no new descriptor open, provider dispatch or physical wait there.

1. Observe the publication/private/context boundary with no project reader or
   preparation snapshot retained. Acquire the fixed bootstrap seed's 16 exact
   references: AS and RX for each of six initial heaps and two critical indexes.
   Establish the local seed ledger before any project cache preparation or open.
2. Under the inherited base, open only those readers using NoLock outside
   raw/Share. Capture discovery profile D and the proposed complete metadata
   graph. End every
   scan/snapshot/reader increment, invalidate the ephemeral catalog snapshot and
   release the seed.
3. Acquire the complete metadata graph's exact AS/RX references with no project
   catalog reader or registered horizon alive. All physical waits precede
   semantic Share, including waits for locks held by prepared transactions.
4. Using the inherited base and the owned complete AS/RX seed references,
   open only the initial readers, capture fresh preparation profile P_A,
   validate its graph and defining prerequisites,
   then close those readers and its snapshot. An expanded/changed dependency
   unwinds the whole metadata attempt before a bounded pre-effect retry.
5. Prepare all nine admitted definition readers outside raw/Share using NoLock.
   Capture full definition observation A under its own coherent snapshot/raw
   span. Close its readers/horizon and release the metadata attempt.
6. Acquire A's complete application and metadata graph, preserving requested
   application modes and metadata AS/RX. No A reader or horizon crosses this
   acquisition.
7. Capture and close P_B through the admitted initial readers under the complete
   physical attempt. Validate fresh profiles and graph before preparing the nine
   B readers. Capture coherent B and normalize selected opaque payload images
   through the admitted TOAST profiles while B's source snapshot stays registered.
8. End B's readers/horizon before native cache preparation. Capture and close
   P_C, validate it, then prepare the final nine readers outside raw/Share.
   Acquire semantic Share only after this preparation and every physical wait.
9. Capture independent coherent C; compare defining graphs/carriers and all
   actual identities with the existing publication/private/context rules.
   The pure copying consumer runs once on owned B images. It performs no native
   operation, planning or protocol output inside Share.
10. Release Share before callback-capable reader/snapshot cleanup and complete
    physical release or explicit native retention. Returned copies are historical;
    they are not an execution lease or admitted immutable plan.

A preparation graph change has the same disposition as a definition graph
change: close all readers and release the whole attempt before reacquisition.
A completed pre-effect lifecycle/stamp change may retry under the existing bound.
A native ERROR or uncertain cleanup requires the owning native abort and never
becomes a successful false retry.

The catalog seed, metadata attempt and full physical attempt each have a local
ledger. Every returned reader, scan and registered snapshot has one matching
close/unregister on the admitted owner. Detach each pointer before cleanup.
Keep backing observation/invocation storage alive while a native attempt or
uncertain abort-owned resource can refer to it. No aggregate owner/count census
replaces this local argument. Data-view, command identity and FirstSnapshotSet
remain unchanged; preparation never resets or repairs a data snapshot.

## Defining bytes and costs

The later type collector keeps the twelve-pass full definition design:
five fixed maps plus range, followed by selected attribute/type/attrdef/enum/
constraint and class-options passes. A/B/C therefore cost 36 direct definition
passes. Every preparation observation above has six direct passes; D, P_A, P_B
and P_C add 24. The candidate total is 60 direct catalog passes before selected
TOAST scans and native internal cache work. This is a design count, not a measured
invocation result or accepted implementation total.

The fixed seed grows from eight AS references to sixteen exact AS/RX references
before discovery; that acquisition introduces no extra catalog pass. RX adds
real Share/SRX contention and possible native transaction retention.
The fixed maps scale with whole-catalog row counts. Selected metadata attributes,
profile/type sets, graph edges and option images require checked phase and
cumulative requested-copy bounds before allocation. Actual exact references
remain within the existing 4096 limit after every mode union. Per-phase bounds
do not certify invocation peak memory; retaining multiple profiles and copied
definitions needs its own accounting. Native options/cache allocation, retries,
protocol round trips and lock contention remain separate measured obligations.

Constraint rows preserve actual own `contypid`, owner fields, kind and flags.
Both headers declare domain NOT NULL as well as CHECK support; CHECK requires
its actual `conbin` carrier, whereas a nonexpression constraint preserves its
independent absence. PostgreSQL 18's `conenforced` and `conperiod` field presence
must be represented explicitly rather than synthesized on 17. Native expressions
stay opaque; collecting a constraint does not validate or execute it.

## Review and implementation gates

Before C changes, independently challenge the supported options path, loader
conditions, field/writer exclusions, critical lookup dependencies, profile
freshness, complete native mode union, reference cleanup and every wait boundary.
Resolve any missing source argument rather than opening a descriptor as a probe.
The source gate remains OPEN at this revision. Independent review at
`fec65d959ef1c3c73588ae81e98330a3b074caa1` found high R1: AS-only initial opens
lacked a noncircular admission/preservation argument. The inherited base and
sixteen-reference sequence above propose its correction; fresh source review
is required before C. R1's original report is immutable at
`logs/review-native-type-payload-bootstrap-fec65d9-source-v1`, report SHA256
`3b8e7cd8565d9f3986c808c7cbdedb661bc23ce0face1ef138db3a94ac3bf432`.

After acceptance, replace the old preparation mechanism in one greenfield change
and implement range/enum/domain-constraint collection against the same copied
graph. Run strict current product/probe packages and the required ordinary suites
on both pinned majors, including fresh/established views, independent constraint
carriers and both prepared DDL outcomes. Then independently review implementation.
No passing prior component or body count certifies this new mechanism.

The finite source record `logs/native-type-payload-bootstrap-primary-v1` binds
12 already pinned source inputs and 29 complete selected function bodies;
facts SHA256 `6817d5b04564327d6e6a67b4316f08315f21e94002bc6579aafb4ee469f380c8`.
The selected cold builders and paired headers are separately saved in root04's
source transcript. Counts establish byte provenance, not whole writer or
functional-path acceptance. Additional named source obligations must stay finite.

The finite initial-base record
`logs/native-type-payload-bootstrap-initial-base-primary-v1` binds eight of the
same pinned source files and sixteen complete selected startup, class lookup,
builtin table-AM, catalog ALTER, trigger-state and reindex entry bodies; facts
SHA256 `5c6c8d4c841a6a02ba059f9cf6c618e40dd6e5f3f9a40df9d0d6cce8bbeeff07`.
Paired trigger creation's explicit SRX acquisition is a separate bounded source
span. These byte records support the named path argument; their body counts
do not certify all writers, startup effects or the complete native service graph.

Full providers/expressions, negative/name candidates, statement/binding/execution,
performance and release gates, issues46/15 and the overall goal remain OPEN.
