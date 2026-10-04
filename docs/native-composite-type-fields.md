# Native composite field discovery

Status: independently accepted existing-reader source design; implementation is
present but paired package/runtime and fresh implementation review gates remain
open. The merged
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
