# Native composite field discovery

Status: independently accepted existing-reader source design and implemented
copied composite declarations. Paired package/runtime receipt verification has
passed; fresh final implementation review remains open. The merged
[declared type-link stage](native-declared-type-links.md) supplies actual root
rowtypes and base/element/array links. This stage adds the positive slots of
selected named composites; the [full collector](native-transitive-type-closure.md),
new descriptor bootstrap, providers, execution, performance and release remain
open. Concurrent native PostgreSQL two-phase transactions remain required.

## Selected identities and field facts

Seed actual root rowtypes and retain input bindings, duplicate roots, exact
modes and root/column provenance. Process each selected type once through the
existing bounded worklist. In addition to its nonzero base/element/array links,
a selected composite resolves its actual nonzero `typrelid` in the copied
`pg_class` map and requires that row's `reltype` backlink to equal its type OID.
The associated relation must have an actual resolved namespace.

Admit copied field declarations for an ordinary nonpartitioned builtin heap
(`relkind=r`, builtin heap AM) or a standalone composite declaration
(`relkind=c`). Other associated relation kinds or table AMs fail explicitly.
These are structural declaration cases, not application descriptor admission.
A standalone declaration is not a physical heap, has no locator to manufacture,
and must not join the ordinary heap/TOAST graph. No application descriptor,
relation map, type cache, syscache or provider is opened to discover these facts.

Expose one composite record per actual associated relation, ordered by relation
OID. Record its type/relation identity, actual class namespace and name, native
kind/persistence/AM/partition declarations, native `relnatts`, and an offset into
the owned positive-slot array. This does not claim complete `pg_class`, options,
constraint, provider or descriptor closure. The existing physical graph remains
limited to application input roots and its admitted metadata graphs; a copied
composite record supplies no additional physical reference or execution lease.

For every selected composite, copy exactly positive ordinals 1 through `relnatts`,
including dropped layout. Require a nonnegative count no larger than the native
`MaxHeapAttributeNumber`, complete unique ordinals and no excess positive slots.
Live slots must have an actual defined selected type with matching length,
by-value and alignment declarations. Follow every live slot's actual `atttypid`
through the same worklist. Dropped type0 supplies no edge, while its native
remaining layout stays owned. Typmod, dimensions, storage/compression, collation,
NOT NULL and inheritance declarations remain independent column facts.

Keep the existing native default/generation/identity/missing checks for all
selected slots. Copy each selected slot's independently absent/present missing
carrier, its own actual attrdef expression when declared, and every selected
type's own two default fields. Reuse their existing opaque normalization paths;
do not substitute an ancestor default, parse an expression or decode a field
value. Root records retain input order and share the appropriate composite's
slot range. Zero-column root rowtypes still select their actual empty range.

Missing, undefined, contradictory or duplicate selected definitions fail loudly.
Visited type membership terminates companion/shared graph paths; independent
domain-base visiting/completed states still reject a domain-base cycle. Neither
selection nor per-seed provenance enumerates paths or recurses on the native C
stack. Anonymous RECORD typmods remain backend state and cannot be manufactured
from these named catalog identities.

## Coherent fixed and selected passes

The first `pg_attribute` pass copies every visible positive fixed slot into an
invocation-owned map, retaining the existing metadata-profile checks. It does
not deform unrelated missing values. Group fixed slots by actual relation OID
as they are copied, with distinct native ordinal keys. Each newly selected
composite traverses only its own group once; rescanning the whole attribute map
for each selected type is not the algorithm.

Selection uses the already copied namespace/class/index/attribute/type maps.
After its worklist closes, a separate full `pg_attribute` pass copies missing
carriers only for selected slots, followed by the existing selected type-default
and attrdef passes. Attribute fixed-map membership, selected membership and
payload completion are distinct. Require exactly one second-pass source for
every selected slot, including NULL carriers. Attrdef selection follows all
selected composites, and validates actual source OID, relation and ordinal.

The second attribute scan has its own tracked handle on the already admitted
descriptor, the same registered nonhistoric snapshot and the same raw span.
There is no new descriptor, second fence with live readers, SQL request, provider
call or native guard wait. End and clear this exact scan increment once with the
existing cleanup. A ends all scans/descriptors/catalog snapshots before physical
waits; B ends scans and retains only its registered source horizon through the
existing payload copying; that horizon ends before C preparation or semantic S.

C independently rebuilds selected types, composites, positive slots and payload
sources. A/B/C compare their complete exposed identities, composite class facts,
ordered slot ranges, fixed field/type facts, presence bits and exact carriers.
Only B normalizes images. The pure copying consumer receives owned C facts and
B images once, with existing exact physical and semantic ownership. Release S
before callback-capable cleanup and physical release/retention. Preserve caller
data-view/FirstSnapshotSet, transaction context and native ERROR abort behavior;
only the existing completed pre-effect lifecycle/stamp retry may restart.

## Costs and ordinary verification

This replaces the root-only fixed/missing attribute pass with a whole fixed pass
and a selected missing pass. It adds one full attribute scan: nine direct scans
per observation, 27 across A/B/C, plus existing selected metadata TOAST reads.
Expose `attribute_payload_rows` separately from `attribute_rows`. The six admitted
metadata roots, their exact modes and physical graphs remain unchanged.

The whole positive fixed map and relation groups scale with catalog size. Group
construction is linear in fixed rows; selected traversal is linear in selected
nodes/fields/declared edges, followed by deterministic sorting. The existing
4096 selected-type limit also bounds selected composites. Check aggregate slot
counts and allocation sizes before arrays or additions; charge worklist and
owned-array growth to the cumulative requested-copy budget and retain periodic
64 MiB native context checks for hash/map allocations. An old root-count-times-
column-limit bound cannot stand in for selected composite/field counts. These
are phase bounds, not peak-memory, allocation-event, contention or throughput
acceptance. Limit failures never revert to root-only discovery.

Rebuild strict product/probe packages and run the existing required PG17/18 suites
on fresh ordinary verification profiles. Independent recursive native SQL uses
UNION membership and actual composite class/slot joins. Compare the complete
selected type/class/field/default/missing/expression facts, sorted ranges and
costs, rather than using the C worklist or copied expected constants.

Ordinary fixtures cover nested standalone and stored table composites, transitive
domain/array fields, shared nodes, duplicate roots/modes, dropped layout, empty
root rowtypes, own TEMP, unrelated definitions, selected nonroot missing/default
images, release/retention and immutable historical outputs. Prepared DDL cases
change composite fields and an application root in one normal transaction, so
the existing root's real physical wait precedes a fresh complete B/C closure.
Verify commit and abort with both first-unselected and established RR data views.
No additional stress, forced error, interruption, recovery or existing-profile
lifecycle experiment belongs to this gate. Fresh implementation review remains
required after the independent source review and actual verification.

## Bounded paired primary evidence

The PG17.11/18.6 `pg_type`, `pg_class` and `pg_attribute` headers define actual
backlinks, positive ordinals, dropped type0 and native layout declarations.
Paired typcache rowtype excerpts show why descriptor/cache loading cannot replace
copied discovery: it opens the associated relation and retains a TupleDesc;
anonymous RECORD instead uses backend typmod state. See the pinned
[PG17 rowtype path](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/typcache.c)
and [PG18 rowtype path](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/typcache.c).

Selected cached bodies are frozen in
`logs/native-composite-type-fields-primary-v1`: facts SHA256
`caee0a316fefd230ba3fb359789565cabb94868b766986c950bbf7f9fa067b95`,
10-member nonself seal `81aa7c175caf6e63eaa65dbc6ddc2edbebac283168bda7a5d12978668521e130`.
The six complete header identities are bound; both class/attribute pairs and
bounded rowtype paths were read here, while the exact type-header bodies were
already fully read for the prior declared-link gate. No broader type-cache
registry or history graph was expanded.

`DefineCompositeType` delegates with `RELKIND_COMPOSITE_TYPE` to DefineRelation,
then heap_create_with_catalog and CheckAttributeNamesTypes enforce the same
native 1600-column bound. Eight selected release-archive bodies, with bounded
creation/limit excerpts, are frozen in
`logs/native-composite-type-fields-limits-primary-v1`: facts SHA256
`c47b7e1216392749efdfef3797b93410a165a8491972c0698fb5cbe5b5f8e9ac`,
12-member nonself seal `dd6da89c7d725c6ec3856b914fa0f113b5947a65cf58c3075bbf0177fd12d90a`.
They were rehashed against the exact named archive inventory entries, without
reopening the full source proof. Record10's schema printer accidentally emitted
opaque inventory keys; that output stays preserved, while record11 filtered only
the eight exact entries before printing. It is not a source census or acceptance
claim. No full-body, provider, new descriptor, runtime or release acceptance
follows from these excerpts or the prior type-link result.

## Source review and current implementation

The independent source-only review at clean
`c43464987b02aa743e95cb7a53bc130e73221a58` accepted this finite design
conditionally, with no required corrections. Its report SHA256 is
`b0ea70dd1ff346c2c1a1223375002eb8a3f4dfd1adc202aaf0bf2a37faf4c772`;
facts `359d029c5182c8f487cd0d39b0e798b6d0757dedd7190cbffc30c6a804236de9`;
518-member nonself seal
`140844d4ff2eb701ab6b3e5468aaad8246f2e5c206e5c9cf8a8c92c86db6625d`.
Final/post/completion/observer each actually exited0; the separate root rehash
verified540 unique paths and16 completion companions. This is source/design
acceptance only. Reviewer reader17's out-of-range excerpt, root17's wrong
observer-summary key and root23's wrong probe pathname remain actual1 in their
original evidence. Distinct corrected readers18/18/25 exited0 respectively;
none is a native runtime failure or a successful retry label.

The implementation uses embedded links to group whole-map positive fixed slots,
separate selected and payload-complete bits, and a separately tracked missing
scan. Composite facts and slot ranges are compared in A/B/C; input roots keep
their original order and physical graphs. Independent SQL follows actual
composite field OIDs with recursive UNION membership and computes slot offsets
with an ordered native class query. Existing ordinary fixtures now check every
selected composite. Two additional ordinary fixtures cover nested standalone
and stored declarations, nonroot inheritance/default/missing facts, own TEMP,
historical outputs and prepared composite/root changes. The prepared fixture
renames a live composite field and adds a root field in one writer transaction;
it neither invents a physical composite lease nor disables native2PC.


## Preserved verification failure and oracle correction

Strict product/probe packages for PG17.11/18.6 completed fourteen commands at
implementation source `13ca94f1cd69637ae921df6de1788fe354e0ef81`; the package
facts SHA256 is
`3bb06ea8594c5cf4c4e4140f528433c6d1df5ee6aa4e435d95ea52df193109ed`.
All26 native inputs are bound. The first eight-profile setup (root31 actual1)
expired its separate readiness timer during stock initialization of the seventh
profile. That unpreloaded PG18 profile is preserved and unaccepted. A distinct
setup helper follows Docker's configured terminal health state; two fresh PG18
profiles completed actual0. The exclusive current-profile assembly binds six
individually completed profiles from the partial setup plus those two fresh
profiles, preserving the parent's failure. Its facts SHA256 is
`fdc9cec3df8c0617d63a3c9ccda63cf1d363d7756607a3982c922a46757dc9c3`.
No existing profile was stopped, restarted, signaled or relabeled.

Root39 PG17 integration actually exited101. Catalog discovery11, builtin2 and
publication18 cases passed; heap storage8 passed and11 timed out. The new nested
composite case passed, while the prepared composite case timed out in the
payload oracle. Later test binaries were not run. Raw stdout SHA256:
`bfffdc87b72de546474bc92d2aaf661f18862390da1fdb29c2758229e5174400`;
binding facts `88807c7229371180d6b83a0b8aa9adba6c481433c8b01756318ecb8772f6f12f`.
This failed namespace and all profiles remain preserved.

The bounded independent implementation/source diagnosis at clean13ca94f found
no required product-C correction within this copied-composite contract, and
one required fixture correction R1. Report SHA256:
`69741d517a75cecc6d1c6bae91d5c0c25c6a81ffd86ba654e8a7858893356b8e`;
311-member nonself seal
`9ebbedd8f4e6709bbb3485746255ef35ea74c564dae3db38c39cefe72b51af6d`.
Final/post/completion/observer each actually exited0, with16 completion companions.
Root46 independently rehashed333 unique members; root47 read the complete report
and terminal observer. R1 identifies a duplicate recursive selection and seven
serial payload-oracle requests inside a20s aggregate bound. It does not establish
the cause of all eleven timeouts. Current-idle/zero-prepared observations,
plan-only output and checkpoint durations do not prove past lock state or load.
Passive root41's wrong psql role (actual2) and root42's dictionary-as-URL error
(actual1) remain preserved; corrected root43 actually exited0.

The corrected fixture shares one independent SQL type/composite selection among
column, missing and payload projections for each metadata phase. Each DDL or
normal prepared completion gets a fresh selection; none is cached across phases.
The payload projection now makes four requests, and missing images one, without
another recursive traversal. Six catalog graph projections use one name lookup
and one batched graph query rather than twelve separate requests. Actual OID
membership, default sources, NULL/image assertions, graph ordering/modes,
historical outputs, native wait barriers and both real data-view states remain
required. The20s operation/60s case bounds stay unchanged. Oracle timing records
completion, timeout or interruption with elapsed time, including case cancellation.

This addresses the established source-level R1 cost; current ordinary runtime
acceptance and fresh final implementation review remain OPEN. These fixture
changes leave the26 packaged native inputs unchanged, permitting exact binary
and profile rebinding without rebuilding or relabeling the old evidence.


### Graph expectations complete the per-phase oracle sharing

At fixture source `a841adf51839fb88e35c6e1f8223399530ab9b90`, PG17 root52/53/54
actually exited0: heap19, remaining integration44 and private3 cases passed;
all90 before/after binding readers completed0. Root55 PG18 heap actually
exited101, with18 passed and one case-wide60s expiration in the pre-existing
owned-default carrier fixture. Both new composite fixtures passed on both majors.
Root55 raw stdout SHA256 is
`166343c1c86633e6f69af686b31fed5a54b8e53f925266801fc170d6f8d241da`;
binding facts
`25cb1dc300400f92ec148225626beceecb95d218861995743b493938138ced26`.
Its30 binding readers completed0 and the failed namespace remains preserved.

The complete failure output contains all12 carrier samples: three metadata
phases, both real data-view states, two captures each. Source and phase timings
show that each capture redundantly reread the six catalog graphs, despite no
DDL inside a phase; some batched graph reads took5–6 seconds. This establishes
avoidable repeated protocol work, while it does not identify the underlying
reason for variable query latency. The expiration occurred after the final
sample, without a declaration/carrier assertion failure.

The fixture now reads catalog graph expectations once after each phase's DDL,
alongside the application graph/column/payload/missing expectations, and compares
the full graph/mode/reference projection on every capture. It never shares these
expectations across DDL phases. All12 native captures, real reader snapshots,
exact physical/coordination lock checks, carrier samples and20s/60s deadlines
remain intact. Product/native inputs remain unchanged. Current paired heap
acceptance and final implementation review remain OPEN; earlier successes and
the root55 failure are not relabeled as current acceptance.

### Paired heap acceptance; publication witness correction pending

At source `83296344de0ae177a2bd8c17e81d0e0c70720ad2`, root59 PG17 and
root60 PG18 each actually exited0 with all19 heap cases passing. Both new
composite fixtures and all12 carrier samples retain their complete assertions.
The26 packaged native inputs remain identical to built source13ca94f. Root58's
unchanged-input proof retains root53/54's44 integration and3 private PG17
passes at their original source; it does not relabel their receipts.

Root61 PG18 remaining integration actually exited101: discovery11 and builtin2
passed; publication17 passed and one exact-generation assertion failed, with
recovered592 versus expected591. Later binaries and PG18 private cases had
not run. Failed stdout SHA256 is
`4c0dda8fdae0d91462ead853e053ec65b0964035c3f6a421e2a513b11c8c243a`;
binding facts SHA256
`830b59ba4ac79bba6233a9ee36f61871cbfdad79477dec35f8c3c3c6b4b9f259`.
The failed receipt, namespace and native profiles are preserved. This is not
paired ordinary runtime acceptance.

Bounded independent source review at8329634 records two medium fixture
corrections: P1 leaves an interval between unheld epoch sampling and a later
raw hold; P2 treats ErrorResponse as cleanup before ReadyForQuery. It establishes
neither a product C defect nor the origin of the extra shared increment. The
successful child recovery commands already supply a later readiness boundary
before the failed recovered assertion; P2 is not asserted to cause592.
The115-member review is at
`logs/review-native-publication-witness-8329634-source-v1`, report SHA256
`94eee52cc17fb48004ef85ff198ed0f8055fbb1e88671689722f08e1f5c109c4`,
seal `4a113f9bdd6fe2d6f330b4746098ddfb7ef781b6763d47356181aaa0edcd7db9`.
Final/post/completion/observer actually exited0; root66 rehash verifies137
unique paths, facts SHA256
`e8055d7281993921e412eb848a7d797dd2d13a68aeb590e340a9f120ce838875`.
Own03/04 source-reader failures remain preserved. Root67's report-read key error
also remains actual1; corrected root68 reads the full report and raw observer.

The correction uses a fifth, otherwise quiet backend owning only the existing
test-probe semantic Share. It acquires and verifies that reference before raw
holds and baseline SHOW, keeping ordinary/late publishers out before their
publication/raw queue. It neither replaces the real native finish wait nor
excludes unrelated native Finish calls: this is explicitly a closed fixture
cohort, not a production boundary. Epochs remain distinct and unheld. Both
missing-GID errors drain exactly one original UNDEFINED_OBJECT through Ready
Idle; both child attempts drain ACTIVE_SQL_TRANSACTION through Ready
FailedTransaction and stream end. Exact+1 assertions, recovery commands and20s
bounds remain. The sentinel's owned/outcome and actual native lock checks must
succeed; it is released before parent COMMIT. Failure cleanup releases the raw
holder and sentinel before draining writer commands and observes all driver
closures under existing bounds. No native module input changes.

Current corrected publication verification and final implementation review
remain OPEN. All full collector/provider/execution/performance/release gates,
issues46/15 and the overall goal remain OPEN; concurrent native2PC stays required.

### Successful native finishes require separate sentinel geometries

At corrected source `dc103508bf84e9ec896099f47ebe0fb73d6bd206`, strict Rust
root72 and PG17 publication root73 actually exited0; all18 publication cases
passed. PG18 root74 actually exited101: discovery11/builtin2 passed, publication
16 passed/two failed. The corrected error-cleanup fixture passed on both majors.
The GID fixture's exact+2 returned671 versus670; the lock-only catalog-view
fixture's exact+1 returned727 versus726. Later PG18 binaries did not run. Failed
stdout SHA256 is
`367c64c0fc575ac9a81610e3169826480e9a54db4b9dbe68eff0e4a4c4473375`,
binding facts `b5ae47e41b3cc83a851e7c96eede16f6af9069d18a73b393d90fa15112abbcd4`.
The failed profile is preserved, including the GID fixture's unreached ordinary
cleanup and replacement prepared transaction; it is not manually recovered.

Independent bounded source review atdc10350 records G1/V1. The complete known
semantic cohort must be verified before any raw reference. A granted Share
cannot span the GID fixture's prepared metadata RX; a pending Share reservation
can exclude later conflicting ordinary RX under the paired native queue rules.
Its pre-baseline multiset is exactly two prepared coverage AS rows, one prepared
metadata RX, and the sentinel's ungranted Share, with no other holders/waiters.
After both native finishes, acquisition must complete and prove owned/acquired
plus actual granted Share before reuse PREPARE/after SHOW. All three finishes,
duplicate PREPARE/native error, value/column oracles and exact+2 remain.

The view fixture warms guard/observer/empty SHOW dependencies before prepared
AX locks, then grants Share before LOCK/PREPARE. Its admitted lock-only path
transfers compatible AS without metadata RX. Both native outcomes, AX transfer
and release, GID absence, actual raw waits and exact+1 remain. Every epoch is
distinct and unheld; raw holds precede those SHOWs. Both geometries explicitly
assume a closed cohort without unrelated Finish, active shared-drop intent,
lock groups or unsupported semantic histories. pg_locks does not certify the
shared-drop intent counter or map a dummy lock row to its GID.

Native command consumers now preserve the original backend error and validate
command tags, exact Ready state and stream end. Tracked preparation/request/
completion state distinguishes the original and reused GID incarnations. On
failure, release raw first, normally finish only tracked prepared targets using
the intended outcomes, drain the queued sentinel, then rollback remaining
transactions and attempt every driver closure. Cleanup errors remain recorded
while the original failure is rethrown. No signal, restart or native error
experiment is added; all20s bounds remain.

The duplicate PREPARE error expects Ready Idle. The paired pinned xact.c
AbortCurrentTransactionInternal TBLOCK_PREPARE branches abort/clean the whole
transaction and reset TBLOCK_DEFAULT; TransactionBlockStatusCode maps that to
Idle. It differs from the child Finish attempts' failed-transaction state. The
finite source read corrects this expectation before ordinary execution; it does
not add a new native error experiment.

The171-member review is
`logs/review-native-publication-finishes-dc10350-source-v1`, report SHA256
`92e162dc2a5286b25f9d98cddb86a9740211cae5d3f00b8a5ec3e18d30101a51`,
seal `53522f9762827d9bf0dce1b6dbc05928216ecfd26d2aaa869e25132132db2ed9`.
Final/post/completion/observer actually exited0; root76 verifies193 paths, facts
`b658c3b0dc53a8458668aa3bcab8924dd1bd639c09abda96f0b9deffd3f173df`;
root77 reads the full report/raw observer. No producer of the extra increments
or product C defect is established. Root78's recorder preflight rejected dirty
source before its helper-read payload ran; that failed namespace is preserved.
Native26 package inputs remain unchanged. Fresh ordinary verification and final
implementation review remain OPEN, as do every broader gate and the goal.

### Paired ordinary receipt verification passed; final review open

At clean fixture source `6730a8e3cc05919b39b866fb8bec6b31beba020d`, strict Rust
root80 actually exited0. Root81 created two fresh ordinary primary profiles for
PG17.11/18.6, with native2PC enabled (`max_prepared_transactions=10`); all195
setup commands passed. It uses the unchanged Docker health policy. Every old
profile, including failed or unaccepted profiles, remains preserved. Root84
assembles these two profiles with six accepted control profiles, retaining the
original770-command setup anchor without relabeling its source or expanding its
historical member graph. Assembly facts SHA256:
`f747f6faa7ac1a88cbf8f1fd1ffff5b128acf2fc49911b07f57f92a5cc9b7429`.

Current root85 passed all18 PG17 publication cases; root86 passed PG18 discovery11,
builtin2, publication18, module2, relation6 and statement5; root87 passed all3
PG18 private catalog cases (101 filtered). Each command actually exited0 and
completed30 before/after profile and binary readers at actual0. All three
corrected publication fixtures passed on each major with their exact generation
counts and original bounds. Root82's paired finite source proof establishes the
duplicate PREPARE error's Ready Idle expectation; no new native error experiment
is added.

Root89's current input proof retains root59/60's19 heap cases per major at8329634
and root53's26 nonpublication plus root54's3 private PG17 cases at a841adf. The
18 older publication cases from root53 are explicitly excluded. Complete source
deltas establish that the retained targets and shared inputs are unchanged;
actual old receipts and profiles keep their original identities. The26 native
inputs still exactly match the four strict packages built at13ca94f. Rebinding
facts SHA256: `bb35d8348e89df505d8c66d6fd883a1fde133a31d0a8d80d88a49aca679e962a`.

Root95's ordinary evidence audit actually exited0, verifying1665 unique members
and132 selected passing cases (66 per major), with no failed/ignored selected
cases. It checks raw streams, all command source readers, the seven selected run
bindings, source deltas,14 package commands and195 fresh setup commands. Facts:
`logs/native-composite-type-fields-6730a8e-ordinary-verification-v5/facts.json`,
SHA256 `520b7877055a2f1d02b6110b1ef2feeb711467cfd980717a1161c0297b887861`.
Eleven earlier failures or preflight failures are preserved and excluded from
acceptance. Root88's mistaken folder pattern and root92's ordered receipt-list
comparison remain actual1; corrected readers and the new v5 audit compare the
complete receipt multiset without changing a count, hash or runtime assertion.

Final independent implementation review remains OPEN. This is finite copied
composite declaration verification, not complete collector/bootstrap/provider/
statement/execution/performance/release acceptance. Issues46/15 and the overall
goal remain OPEN; concurrent native PostgreSQL2PC remains required.
