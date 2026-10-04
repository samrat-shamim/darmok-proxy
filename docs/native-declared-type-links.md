# Native declared type links

Status: C/header/probe/oracle implementation is present; current paired builds,
ordinary verification and independent implementation review are pending.
Independent source review accepted the existing-reader selection/pass gate at
`1e4c7581130da8a6a0bffb6d2a218b0cf3425efa`, with no required design findings.
This component replaces direct live-column selection with root row types and the complete reachable
set of the three type OID links already stored in `pg_type`. It is a discovery
stage of the [full type definition collector](native-transitive-type-closure.md),
whose composite-field, range, enum, domain-constraint and provider gates remain
open. No completed type definition or execution admission follows from this set.

## Selection and owned facts

Seed each distinct application root's actual `pg_class.reltype`, and every live
positive root column's actual `atttypid`. Input bindings retain duplicate roots,
modes and ordered column identities. A zero-column root still has its actual row
type. Dropped columns retain their layout, but type0 supplies no live seed.
Require each root row type to be defined, composite, and linked back to that
root's actual relation OID.

Follow every nonzero `typbasetype`, `typelem` and `typarray` by actual OID. Copy
every selected row's existing fixed declarations and independently present or
absent binary/text default images. Each selected OID occurs once, sorted by OID;
seed provenance is supplied by the retained root/column identities and the actual
links in those rows. Ordinary element/array-companion cycles terminate through
visited membership. Category, names, subscript and I/O OIDs remain declarations.
A nonzero element link does not establish a true array or admit its handler.

Selected targets must exist, be defined, have a resolved actual namespace and
use a known native kind. A domain must have a nonzero base; a non-domain must
have a zero base. Validate selected domain base chains separately with bounded
iterative visiting/completed states: a domain-base cycle is an error, while an
element/companion cycle is normal. No recursion, heuristic target, partial result
or inherited-default substitution is permitted. A domain's own default, typmod,
dimensions, collation and NOT NULL declaration remain independent of its base.

The copied `typrelid` on other composite nodes, range/multirange declarations,
enum kind and domain kind retain the outstanding full-collector obligations.
This stage does not traverse other composites' columns, `pg_range`, `pg_enum`
or `pg_constraint`; their required admission and collection remain tracked by
the full contract. The private view must describe its limited graph explicitly.

## One coherent observation

Use the existing admitted six catalog descriptors and exact AS references. The
first `pg_type` pass copies fixed rows into an invocation-owned map, without
deforming unrelated defaults. After the fixed namespace/class/index/attribute/
type maps exist, select the reachable type OIDs using only copied bytes. A second
full `pg_type` scan copies the two default carriers only for selected rows. It
uses the same admitted descriptor and registered snapshot, in the same raw span;
it never obtains another fence with live readers.

Give that second scan its own tracked `TableScanDesc`, just as the current
selected class-options pass does. Scan completion/cleanup clears and ends that
exact increment once. A closes all scans, descriptors and its catalog horizon
before physical acquisition. B closes scans after capture and keeps only the
registered source horizon through the existing external-payload copying; that
horizon closes before C preparation or semantic acquisition. C independently
rebuilds the set from its own maps. A/B/C compare all selected identities, fixed
facts, independently absent/present defaults and raw carrier bytes. B alone
normalizes the admitted copied images. The pure consumer receives owned facts
and B images under the existing exact physical and semantic ownership.

All scan/descriptor opens, native refresh, payload fetching and callback-capable
cleanup preserve their existing placement. Native ERROR still requires abort;
only the established bounded pre-effect lifecycle/stamp retry can restart an
attempt. No data-snapshot reset or relaxed prepared-transaction behavior is added.

## Costs and verification

This replaces one selected fixed/default type pass with one whole fixed-map pass
and one selected-default pass. There are eight direct scans per observation,
24 across A/B/C, plus the existing selected metadata TOAST reads. Metadata heap
roots, their physical graphs and exact lock modes are unchanged. Expose separate
`type_rows` and `type_payload_rows` counts so the additional pass is measurable.
No database/protocol round trip or new descriptor kind is introduced.

The whole fixed map scales with native type count, but unrelated default carriers
are not copied. Use the existing checked cumulative allocation/context budgets
for map entries, the queue and final arrays. Bound selected types at4096 before
queue insertion; the worklist and domain-chain validation are iterative and
linear in the selected graph. Node membership avoids exponential path copies.
Selection limits fail explicitly and never revert to direct-only capture.

Required ordinary PG17/18 fixtures compare complete selected facts and default
images against an independent recursive native SQL oracle. Cover root row types
and zero-column roots, nested domain bases with independent defaults, domain over
array and array of domain, shared nodes and companion cycles, duplicate roots,
dropped slots and unrelated types/defaults, fresh and established RR data views,
own TEMP metadata, and both prepared DDL outcomes that change selected links.
Existing catalog, heap-storage, guard, module, dispatch and private native suites
remain required on rebuilt current product/probe packages. No new stress, forced
error, interruption, recovery or existing-profile lifecycle experiment is used.
Fresh implementation review remains a separate merge gate.

## Primary source and admission scope

The complete pinned `pg_type.h` headers establish these existing fixed fields,
domain base/element/companion meaning, defined shells, default presence rules,
known native kinds and original TOAST4171 declaration. Captured copies in
`logs/native-type-links-primary-v1` are selected leaves of the earlier archive-
bound source corpus. Facts SHA256 is
`14344de93dc3f19f6d54b0c061cfa574d4228f424d497d19dc55be21200762fa`;
the four-member nonself seal is
`cafbd4cc43aa510fbb8a495f56d87fcdf0d5bb5dc48d4d1a76ed01c54b2c2866`.
The PG17 header SHA256 is
`41f3027163983013a48d382bd3e108e5a9f5a4f9321354a24e6b212b3285e10c`;
the PG18 header SHA256 is
`8fb198749fd82b6c1818a3c18455f226f66116fcfeb3d4959c65ce3fda22802e`.
See the original [PG17 header](https://github.com/postgres/postgres/blob/REL_17_11/src/include/catalog/pg_type.h)
and [PG18 header](https://github.com/postgres/postgres/blob/REL_18_6/src/include/catalog/pg_type.h).

The proposed additional pass reuses the current raw-carrier copier, selected
type-default normalization, fixed native layout and admitted reader. It opens no
new catalog, TOAST or application descriptor and invokes no type provider. This
source gate therefore concerns that local selection, scan lifetime and existing
payload path; it cannot accept the separate new-descriptor bootstrap gate or
the full dependent definition, execution, performance and release gates.

The frozen source review is `logs/review-native-type-links-source-1e4c758-v1`:
report SHA256 `149439c60333ed6e6d4040d4e9d296f6801a1c76436963bd8b566645f012f38c`,
facts SHA256 `e2b18078a663e923c671b53b3865bc41b0f1a80babe4f6759a2724920b6d5bc8`,
and565-member nonself seal SHA256
`5243954027e5d30399e70ef55f2da45a6cd326df32e81b732ef8208849208681`.
Its16 completion companions and actual final/post/completion/observer outcomes
were independently executed0. Root11 rehashed587 unique paths and root12 separately
verified observer21 actual0 with empty stderr. The root rehash facts SHA256 is
`a33467334ebcb93e263e3162dd8b575148a647fd99526b0d4ae1e7c45e5683c5`.
The two failed passive reviewer readers and uninvoked draft remain preserved;
they are not runtime results. Source acceptance is limited to the proposed local
change and does not certify this implementation.
