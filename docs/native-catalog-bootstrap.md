# Native metadata descriptor bootstrap

Status: supported path investigation under the selected
[stock runtime contract](native-runtime-contract.md). The new descriptor
bootstrap implementation gate remains OPEN before adding new collector paths.
Native cache/reference services are trusted; Darmok's own acquisitions, pre-open path
admission and exact guard sequence still require proof. This does not certify
an implementation-ready collector, new runtime behavior or an execution lease.
The accepted six-catalog and direct-payload components retain their original scope.
Concurrent native PostgreSQL two-phase transactions remain required.

The [entry mechanism comparison](native-entry-boundary.md) records the selected
stock-server/extension direction. Exhaustive native owner/registry/producer
censuses are superseded acceptance premises. The functional pre-open
requirements below remain in force.

The [catalog reference scopes](native-bootstrap-references.md) separate
explicit initial AS acquisition from the existing readers and follow each
returned scan, snapshot and descriptor through matching cleanup. This change
admits no new descriptor/options path. Current paired ordinary verification covers this
finite implementation; independent fresh review is its merge gate. Broader
supported-path and writer gates remain open.

The [transitive type investigation](native-transitive-type-closure.md) adds
`pg_range`, `pg_enum`, `pg_constraint` and constraint TOAST2832 to the intended
metadata graph. Each new native descriptor must have a pre-open admission
proof, including cold and invalidated paths. Opening it and checking afterward
cannot establish that proof.

## Guard alternatives and cost

The pinned native conflict matrices determine these differences. AS means
AccessShare, RX means RowExclusive and SUE means ShareUpdateExclusive. An exact
AS reference is still required when a native reader expects it; another mode
does not replace that reference.

| Candidate | Defining coverage still needed | Native concurrency consequence |
| --- | --- | --- |
| AS + RX, requiring NULL options | Does not exclude SUE option writers; rejected as a durable NULL-options proof | Reader attempts and ordinary RX catalog writers can coexist |
| AS + SUE, requiring NULL options | Must cover every relevant writer and the entered/rebuild paths | SUE conflicts with itself, so attempts on the same metadata relation serialize; maintenance also waits |
| AS + Share, requiring NULL options | Must cover every relevant writer and the entered/rebuild paths | Share permits other Share readers but conflicts with RX catalog writes and SUE maintenance |
| AS + RX, admitting complete supported options paths | Must prove the actual heap/builtin-btree path, carriers, registry/profile and entered/rebuild ownership | Avoids introducing either stronger mode's global coupling |

The last candidate is the preferred direction to investigate. This choice is
based on native concurrency and complete path admission, not a claim that RX
freezes options. No new mode is implemented or approximated by an old mode.
The [PG17 conflict matrix](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/storage/lmgr/lock.c)
and [PG18 conflict matrix](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/storage/lmgr/lock.c)
support the table. None of these modes freezes external provider libraries or
establishes the private callback profile by itself.

The [type metadata preparation proposal](native-type-payload-bootstrap.md)
now gives that candidate a concrete separate-profile and exact AS/RX sequence.
Independent source review remains required before new collector C changes.

The first proposal's independent review found R1: its initial AS-only opens
preceded the fresh profile and complete guards. The revised
[initial-base and preservation argument](native-type-payload-bootstrap.md#initial-admission-base-and-preservation)
distinguishes inherited native bootstrap admission from fresh dependent checks
and requires 16 exact AS/RX seed references before project preparation. That
correction still requires fresh source review; no new native result is claimed.

All physical waits, including waits for locks retained by a prepared transaction,
must occur before S. No catalog snapshot or reader descriptor increment may
cross a new physical wait. Stronger catalog locks would add real waits for
otherwise unrelated native catalog writes; disabling native two-phase support
is not an acceptable way to remove them. Any adopted guard set needs exact
ownership, ordering, complete-attempt release/retry and retention proof.

## Follow the actual options path

For ordinary catalog heaps/TOAST, `RelationParseRelOptions` reaches
`extractRelOptions`, which selects `heap_reloptions(..., false)` for the admitted
kinds. The path then uses the global registry through `default_reloptions` and
`build_reloptions`. Array deconstruction uses the builtin TEXTOID layout, rather
than consulting a selected type's I/O provider. Already initialized builtin
btree descriptors use `btoptions` and `build_reloptions` with the BTREE kind;
the native `bthandler` sets that options function. These are specific paths,
not permission to invoke an arbitrary access method's options callback.
See paired [PG17 array primitives](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/adt/arrayfuncs.c),
[PG18 array primitives](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/adt/arrayfuncs.c),
[PG17 btree options](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/nbtree/nbtutils.c)
and [PG18 btree options](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/nbtree/nbtutils.c).

A generic call-site inspection is insufficient: `allocateReloptStruct` and
`fillRelOptions` contain fill-callback calls even when validation is false.
The registration path decides reachability. In both pinned majors, global
`add_string_reloption` passes a NULL filler; local string registration accepts
a filler and belongs to a separate local registry. The native builtin global
string table is empty. The false-validation path does not invoke the registered
string validator; registration itself can validate a default and is not part
of descriptor parsing. Local registration/parsing cannot be silently treated
as the global heap/btree path. See paired
[PG17 registration and parsing](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/common/reloptions.c)
and [PG18 registration and parsing](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/common/reloptions.c).

This narrows the callback question; it does not accept every extension
configuration, provider, allocation or option datum. A concrete profile still
needs functional registry/provider admission, supported native carrier paths
and resource accounting before it calls native parsing. Preserve actual
NULL/empty/nonempty option images independently from parsed `rd_options`.
The [registry construction trace](native-reloptions-registry.md) identifies
direct kind-mask registration, referenced enum data, the missing public census
and PostgreSQL 18's explicit-set field. The supported configuration must preserve
registration/parse-table correspondence before those paths run; a private
registry census is not required by the stock runtime contract.
Native cache defaults or ignored options are not an alternative source of
the collector's defining bytes. Unexpected carriers or unsupported paths must fail
before opening the path that depends on them.

## Cold and invalidated descriptors

`RelationBuildDesc` loads class and attribute facts, selects table/index access
information, parses options and conditionally loads other defining state.
Fresh copied profile facts must establish each new dependent descriptor's
admitted native kind, namespace, persistence, handler and physical identity,
exact positive attribute layout,
default/missing/generated declarations and the flags that select additional
descriptor paths. Compiled names/types/length/alignment are expectations to
check against actual rows, not fabricated replacement catalog facts.

The initial six-reader discovery is separately admitted by the named original
builtin catalog interface and its closed permitted transitions in the new
proposal. Later fresh checks cannot retrospectively admit those first opens.
This finite bootstrap exception neither extends to newly selected descriptors
nor substitutes native cached values for actual defining carriers.

The [fixed descriptor declarations](native-descriptor-declarations.md)
component now exposes the actual class CHECK count and rule/trigger hints from
the existing observation. Its current verification is separately recorded;
it adds no descriptor path. These inputs do not complete the pre-open profile:
conservative hints are not complete rule/trigger definitions, and every new
dependent branch still requires admission before it can open or rebuild.

The tuple-descriptor implementation differs by major:18 populates compact
attributes and handles virtual generation and native NOT NULL state differently.
Catalog NOT NULL declarations must not be rejected merely because ordinary
table NOT NULL handling takes a different branch. A zero-check profile must
still establish actual default/missing/generation absence before skipping those
loaders. Additional rule/trigger or other descriptor paths need their own
admission; the bootstrap does not execute or interpret them.

Cache invalidation is an entry concern as well as a recheck concern. In17,
`RelationFlushRelation` routes active entries through rebuilding
`RelationClearRelation`;18 separates clear and rebuild functions. Active entries
can reach `RelationBuildDesc`, while inactive pre-existing entries can be
discarded. Locally created/relocated entries and nailed/index descriptors take
other paths. `RelationReloadIndexInfo` parses options again. A new guard acquired
later does not retroactively admit an unsupported functional callback or repair
an unmatched project reference. Warm-cache state and critical flags do not
establish fresh defining facts. Native internal entries remain trusted services.
See paired
[PG17 relcache paths](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/relcache.c)
and [PG18 relcache paths](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/relcache.c).

The required project reference lifetimes and supported callback/path admission
remain OPEN for new descriptors. They must govern project cache operations,
native snapshot use, opens, invalidation and cleanup. An unsupported dependent
path cannot be probed by opening/rebuilding it and checking afterward. Ordinary
native cache entries and intrinsic pins belong to the trusted runtime; neither
a private Rust owner nor a passing fixture proves Darmok's own acquisitions.

## Original builtin heap dispatch

The paired source narrows one provider question. When
`RelationInitTableAccessMethod` takes its catalog branch, it selects
`F_HEAP_TABLEAM_HANDLER`. `InitTableAmRoutine` calls `GetTableAmRoutine`, whose
zero-argument OID call passes through `fmgr_info` to the original-builtin
lookup. `fmgr_isbuiltin` uses the compiled OID index/function table. A hit
initializes the function pointer and returns before the `pg_proc` lookup and
nonbuiltin dispatch branches. `FunctionCall0Coll` invokes that pointer. The
native heap handler returns the same static `heapam_methods` object as
`GetHeapamTableAmRoutine`.

Both pinned bootstrap data rows assign original OID 3 to
`heap_tableam_handler`. This source finding applies to the original compiled
identity and admitted catalog branch. A function alias goes through a catalog
lookup; a matching name alone cannot establish this dispatch. The
supported native runtime, catalog-branch prerequisites and complete functional
options/carrier, callback and project-resource path still need to be stated.
No original-binary attestation or complete native integrity census is required.
There is no new runtime/build result or universal provider certificate here.
See paired [PG17 table-AM dispatch](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/table/tableamapi.c),
[PG18 table-AM dispatch](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/table/tableamapi.c),
[PG17 function lookup](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/fmgr/fmgr.c),
[PG18 function lookup](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/fmgr/fmgr.c),
[PG17 bootstrap function rows](https://github.com/postgres/postgres/blob/REL_17_11/src/include/catalog/pg_proc.dat),
[PG18 bootstrap function rows](https://github.com/postgres/postgres/blob/REL_18_6/src/include/catalog/pg_proc.dat),
[PG17 heap handler](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/heap/heapam_handler.c)
and [PG18 heap handler](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/heap/heapam_handler.c).

This exact handler path uses bounded stack call/lookup state and returns a
static routine pointer; it adds no catalog or protocol round trip. The enclosing
descriptor build, options parsing, cache work and native owner allocations keep
their separate costs. This source cost analysis is not a measured bootstrap
latency or peak-memory result.

## References, reset and the entry witness

`RelationIncrementReferenceCount` increments `rd_refcnt` and remembers the
reference in `CurrentResourceOwner` during normal processing.
`RelationDecrementReferenceCount` decrements it and forgets that exact owner
reference. Bulk release removes the owner item before invoking its release
routine; `ResOwnerReleaseRelation` consequently decrements without forgetting
the item again. Native relcache references use the BEFORE_LOCKS release phase.
Darmok-owned increments and matching owner/count semantics must be retained
through every project phase; native internal owner bookkeeping is trusted.

In the ordinary successful simple-query path, `exec_simple_query` runs its
unnamed portal to completion and drops it before reporting command completion.
`PortalDrop(..., false)` runs the remaining cleanup, releases its resource
owner in all three phases and deletes it. Successful non-top-level lock cleanup
reassigns locks to the parent transaction while releasing the descriptor
references. This supports a construction proof for fixed private controls;
it does not establish cleanup of unrelated portals or references. Portal and
resource release can invoke callbacks, so their admitted paths must remain
outside S. See paired [PG17 portal cleanup](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/mmgr/portalmem.c),
[PG18 portal cleanup](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/mmgr/portalmem.c),
[PG17 native owners](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/resowner/resowner.c)
and [PG18 native owners](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/resowner/resowner.c).

The existing [private connection owner](native-backend.md) establishes fresh
connector ownership and fixed command submission. Its current control API and
the C invocation's owner-identity checks supply different observations:

| Observation | Established fact | Additional proof still required |
| --- | --- | --- |
| Exclusive Rust owner | Commands cannot interleave through an escaped client | Supported native context, fixed transitions and relevant callback admission |
| Current and transaction ResourceOwner identity | The recorded invocation boundary is unchanged | Each Darmok acquisition and matching cleanup |
| `rd_refcnt` on an obtained descriptor | Native total reference count | Project increments/lifetimes; an intrinsic pin does not supply an owned lock |
| `criticalRelcachesBuilt` | Native critical initialization completed | Fresh defining facts and supported dependent paths |

The pinned public relcache header offers lookup and invalidation routines, but
no passive iterator over the cache's active references. The public owner type
is opaque. `RelationIdGetRelation` can increment and rebuild an invalid entry
before returning, so it cannot be used as a pre-open inspection shortcut.
Neither a caller-supplied flag nor matching owner pointers proves project
lifetimes. The selected construction relies on ordinary supported native
services and a closed private project history; it does not enumerate every
native owner item. Its local lifetime and supported-path argument remains OPEN
for new descriptors. See paired
[PG17 cache API](https://github.com/postgres/postgres/blob/REL_17_11/src/include/utils/relcache.h),
[PG18 cache API](https://github.com/postgres/postgres/blob/REL_18_6/src/include/utils/relcache.h),
[PG17 owner API](https://github.com/postgres/postgres/blob/REL_17_11/src/include/utils/resowner.h)
and [PG18 owner API](https://github.com/postgres/postgres/blob/REL_18_6/src/include/utils/resowner.h).

The reset route needs its own continuous admission. Native SI receipt can call
`InvalidateSystemCaches`; its extended form invalidates snapshots/caches,
processes the relation cache and invokes registered callbacks. Both majors invoke
syscache and relcache callback lists; PostgreSQL 18 additionally invokes its native
relation-sync callback list. Reference-free entries can be discarded, while
active entries follow the major-specific invalidate/rebuild paths described
above. This is source investigation of the reset path, not an executed queue
reset, interruption or recovery experiment. See paired
[PG17 invalidation](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/inval.c)
and [PG18 invalidation](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/inval.c).

Before collector C changes, establish the supported native paths and extension
configuration, each project reference lifetime and exact acquisition sequence.
Preserve neutral preparation across SI, new opens, reloads and cleanup.
The [startup reference trace](native-startup-references.md) distinguishes
intrinsic nailed pins from owned reader items and retained locks. It also
identifies restored init-file options/provider work and the limited assertion
scope of end-of-transaction cleanup. Those findings describe the native services
trusted at startup. A critical-cache flag cannot replace fresh defining facts
or project lifetime accounting.
The [fixed private-command trace](native-fixed-command-history.md) identifies
parse/utility/string-object hooks, GUC restoration, resource-release callbacks
and savepoint lifecycle that this preservation proof must cover. Requiring
every hook to be absent would reject Darmok's own catalog utility route.
Each new wait must still follow complete reader/snapshot closure and precede S.
This investigation supplies concrete source obligations; it closes no
authoritative entry, bootstrap, whole statement or implementation gate.

## Passive module footprint and its limits

The separate [private module observer](native-module-footprint.md) implements
owned pathname capture without a catalog descriptor open. Its instrumentation
and ordinary component verification can precede the complete entry certificate;
this does not relax the pre-open proof required for new collector descriptors.
Its current implementation/review status is recorded separately.

The separate [live builtin dispatch observer](native-builtin-dispatch.md)
compares the running backend's selected public heap/btree rows with their linked
symbols without a catalog open or handler invocation. Like pathname capture,
this passive instrumentation can precede the complete entry certificate. It
does not admit descriptor construction, provider execution or an original-build
identity claim, and does not relax the remaining registry/reference obligations.

Both pinned majors expose `EstimateLibraryStateSpace` and
`SerializeLibraryState` through `fmgr.h`. The estimator walks the native loaded
file list and accounts for each pathname terminator plus the final terminator.
The serializer walks that list and copies the pathname strings, ending with an
additional NUL. Neither selected function loads a module, opens a catalog
reader or invokes a registered callback. They are concrete passive observation
primitives for supported-configuration checks or diagnostics, not an accepted
project lifetime or statement-validity proof.

The estimator and serializer must observe the same unchanged list. The copy
routine relies on its caller's capacity: its per-entry check is an `Assert`,
not a production error for an undersized buffer. A proposed use must bound the
actual total bytes, allocate outside S, prevent intervening module loads, and
validate the copied representation within that bound. `RestoreLibraryState`
is a different operation: it calls `internal_load_library` for every copied
pathname and must never be substituted for passive observation.

PostgreSQL 18 additionally exposes an opaque `DynamicFileList` iterator:
`get_first_loaded_module`, `get_next_loaded_module` and
`get_loaded_module_details`. The selected implementations return existing
list links, pathname and magic-block metadata. Module name and version can be
NULL. The native comment explicitly provides no protection against changes to
the list during a scan. A proposed observer must maintain a stable traversal,
copy any retained strings, account for their bytes, and preserve NULL rather
than inventing a name/version. PostgreSQL 17's selected public API section
contains the estimator/serializer but not these iterator APIs; do not emulate
18's private struct access on17. See paired
[PG17 loader API](https://github.com/postgres/postgres/blob/REL_17_11/src/include/fmgr.h),
[PG18 loader API](https://github.com/postgres/postgres/blob/REL_18_6/src/include/fmgr.h),
[PG17 loader implementation](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/fmgr/dfmgr.c)
and [PG18 loader implementation](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/fmgr/dfmgr.c).

The list's publication point matters. On the pinned Linux builds,
`internal_load_library` first searches by pathname, then by device/inode
identity. A new module undergoes
`dlopen`, magic checking and its optional `_PG_init` call before being linked
into the list. Completed nested loads can therefore be linked before their
parent. If initialization raises an error before publication, the list cannot
serve as a record of that initialization's earlier effects. This is a source
path finding; no failed-load or interruption experiment was executed. Repeated
`load_file` calls still use this coalescing loader, rather than proving a fresh
initialization merely because a caller requested a load.

The major-specific differences also constrain identity.18 stores the magic
block pointer and compares its separate ABI fields. Its
`load_external_function` strips a simple `$libdir/` prefix before path
expansion, while leaving nested paths to the expansion routine. Consequently,
a declaration string, a loaded pathname, a name/version pair and native ABI
compatibility supply different facts. None alone binds the executable build,
actual registry contents, callback history or descriptor-reference owners.
The observer cannot establish those separate facts. The selected stock contract
does not require an exhaustive native integrity or producer certificate.
The [file observations](native-build-observations.md) add matching release/API
bytes, installed generated headers and the original heap-handler row in each
captured executable. Its declared one-argument signature differs from the
zero-argument native invocation traced above. Neither link-image pointer targets
nor header declarations establish live provider binding or the entry base.

Shared preload processing calls `load_libraries`; session processing calls it
for both session and local preload lists. The common loader treats NULL/empty
lists as no work, parses declared paths and invokes `load_file` for each path.
Invalid list syntax is logged and returns from this selected function. Its
configuration text therefore cannot be treated as the actual successful-load
list. The selected bodies establish loader/preload lifecycle only; they do
not certify complete backend startup or other behavior in these source files.
See paired [PG17 preload processing](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/init/miscinit.c)
and [PG18 preload processing](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/init/miscinit.c).

Passive pathname capture is linear in module count and total pathname bytes;
18's iteration adds one visit per module and any copied metadata bytes. These
selected observations add no database or protocol round trip. Their buffer,
resource bound and stable-list construction remain design obligations, with
no measured latency/peak-memory claim. Use a footprint only for its named
supported-configuration or diagnostic role. Fixed project transitions, relevant
callback/path admission, writer coverage, guard sequence and whole-statement
validity remain OPEN for new implementation; no descriptor path is admitted by
this source investigation.

## Writer coverage and remaining sequence

The complete paired `AlterTableGetLockLevel` bodies distinguish rewriting,
default/missing/generation/layout changes from trigger changes and SUE
statistics/options/maintenance changes. Their option dispatch is the earlier
NULL-options counterexample. Separate trigger/index/maintenance entry points
are part of the source inventory; an inventory is not proof that every writer
has been challenged. Names, index membership, physical mapping and in-place
hints need their own defining-field classification. See paired
[PG17 ALTER lock selection](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/commands/tablecmds.c)
and [PG18 ALTER lock selection](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/commands/tablecmds.c).

`vac_update_relstats` is not only a tuple-count writer: it updates in-place
class state and can clear relation index/rule/trigger hints.18 also carries its
native all-frozen statistic. A concrete collector must distinguish those
actual hint updates from schema/provider changes, without inventing18 fields
on17 or using a hint as proof of complete live index membership. This source
read does not exclude all hint writers or certify a publication/cache shortcut.

Before new collector C implementation, close the local reference-lifetime and
supported options/path argument; finish writer and defining-field coverage;
then select the exact
bootstrap observations and guard sequence. Discovery must use only admitted
readers. Close the discovery horizon before acquisition, collect fresh profile
facts after acquisition, and admit each new descriptor before opening it.
An expanded graph must release the whole attempt before further waits. A/B/C
observations must remain independently coherent, preserve the entered data view
and command identity, and compare all defining carriers and graph identities.
The pure copying consumer runs once; S ends before callback-capable cleanup,
physical release/retention and any row/data/XID wait.

The transitive candidate now needs36 definition passes; the separate preparation
proposal adds24 bootstrap passes before TOAST/native cache work. This candidate
count remains subject to source review and implementation. There is no accepted
measured total pass count,
peak memory, throughput or contention result. Complete ordinary paired
fixtures, strict product/probe packages, existing required suites and a fresh
implementation review are required after an adopted implementation. No new
stress, corruption, forced error, interruption, recovery or existing-profile
lifecycle experiment is authorized by this source investigation.

## Finite source evidence

The v1 primary record rehashes26 selected cached bodies and captures eight new
HTTP-200 trigger/index/vacuum/analyze bodies,17 per major. Under
`logs/native-catalog-bootstrap-primary-v1`, facts SHA256 is
`7a5dbd8882cb8c58e27da3b403ffc75a275373ad363d4010eac1953de32fd2b5`
and the47-member nonself seal is
`944c1b1126b165ea07edb61160c1542e83cf8d498a8c063a012fb7949751b3fa`.
V2 rehashes those34 bodies and four selected array/btree bodies, then captures
two HTTP-200 btree utility bodies:20 per major. Its facts/48-member seal are
`a13755f9ec985ccddcb06df32197d784e1e0dc8ef0ebd38858cacf90452e6bc4`
and `78b61856fca1b4cbeb7f9a8b771f666ed522d2898710eea46468b06891e87ea7`.
These records do not recursively recertify prior runtime review graphs.

Three distinct selected-function records preserve38 cold/options/writer bodies,
11 rebuild bodies and16 global/local/array/btree bodies. Five separately saved
paired differences retain native major variations. Source counts and identical
body comparisons are byte/provenance facts; they do not mean all40 source files
or65 selected bodies have complete semantic admission. The new descriptor,
registry/owner, writer and whole statement gates remain OPEN. No C, Rust, SQL,
native build or runtime result is introduced by this checkpoint. Standing
security/compiler/hosted-CI/publication exclusions remain in force.

The entry follow-up records12 selected cached lifecycle/API bodies plus ten
HTTP-200 handler/lookup/public-header bodies under
`logs/native-catalog-entry-primary-v1`:11 per major. V2 rehashes those22 bodies
and adds six HTTP-200 heap-handler/bootstrap-data/function-macro bodies:14 per
major. Selected records preserve54 complete function bodies and six complete
native macros/data rows; eight saved paired differences retain actual major
variations. These are finite byte/provenance records and targeted source reads,
not semantic acceptance of all28 files or all60 selected records. The entry,
callback/registry, writer, sequence and implementation/runtime gates stay OPEN.
