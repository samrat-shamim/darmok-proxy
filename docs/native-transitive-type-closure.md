# Transitive native type definitions

Status: source investigation and design gates, not an implementation-ready
collector or statement-admission certificate. The existing
[variable catalog payload component](native-variable-catalog-payloads.md)
owns images for directly referenced types and selected columns. It does not
follow type edges. The next [declared type-link stage](native-declared-type-links.md)
specifies discovery through the already admitted `pg_type` reader, before the
additional catalog descriptors required by this full collector. The proposed
extension below must close its new descriptor
bootstrap gate before native implementation. Issues46/15 and the overall
release gates remain open. Concurrent native PostgreSQL two-phase transactions
remain required; disabling them is not an alternative.

The [stock runtime contract](native-runtime-contract.md) defines the native API
assumptions. New collector paths still require functional admission, project
reference lifetimes and exact guards; an exhaustive native integrity census is
not a bootstrap prerequisite.

## Why direct type rows are insufficient

A table column can name a domain over a domain, an array whose element is a
domain, or a named composite with fields that introduce further types. A range
stores its subtype and multirange association in `pg_range`, rather than in
`pg_type.typelem`. Enum values live in `pg_enum`; domain checks live in
`pg_constraint`. Copying the original column's `pg_type` row cannot describe
these definitions. Conversely, `typarray` is a companion link: traversing it
and the array's `typelem` creates an ordinary cycle, not evidence of corruption.

The target is an invocation-owned structural definition graph, followed by a
separate statement-dependent provider/expression admission proof. Native type
cache loading cannot substitute for this copying boundary. In both pinned
majors, domain constraint loading reads ancestor domains and plans their CHECK
expressions. Composite loading opens its associated relation with native AS;
range loading initializes comparison/canonical/subdifference function state.
Those paths can introduce work that an already-held semantic reference does
not admit. See paired [PG17 type-cache paths](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/typcache.c)
and [PG18 type-cache paths](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/typcache.c).

## Selection and defining facts

Seed the graph with actual root rowtype OIDs and the actual type OID of every
live positive root slot. Preserve each seed's relation/ordinal identity and
input-root provenance. Dropped slots retain their native layout but type0 adds
no live edge. Duplicate roots share definitions without erasing their modes or
seed identities. Names, categories and preferred-type flags are declarations;
none selects a builtin codec or proves a handler's behavior.
Every selected type must have one actual defined row and a resolved namespace;
an undefined shell or unknown native kind cannot become a completed definition.

Follow nonzero declared base, element and array-companion OIDs by actual
identity. Domains retain every ancestor's own default fields, typmod,
dimensions, collation and NOT NULL declaration; a parent's current default
must not overwrite a child's independently stored default. A nonzero `typelem`
alone does not prove a true array or authorize its subscript handler. Preserve
the actual handler OID and edge kind. Use visited-node membership to terminate
ordinary companion/composite cycles, with an explicit selected-type bound.
Validate a domain base chain separately; a missing target or domain-base cycle
must error rather than return a partial graph.

For each named composite, resolve `typrelid` to its actual `pg_class` row and
require its actual `reltype` backlink. Copy every positive slot, including
dropped layout, and follow each live field's type. Record whether that relation
is an ordinary stored heap or a standalone composite declaration. A standalone
composite has no heap locator to invent. Its descriptor facts must not be
silently routed through the ordinary heap/TOAST graph. Other relation kinds
need their own explicit physical/descriptor proof before admission. An
anonymous RECORD typmod is backend runtime state, not a named catalog row;
this graph cannot manufacture its descriptor. See
[PG17 rowtype lookup](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/typcache.c)
and [PG18 rowtype lookup](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/typcache.c).

For range or multirange types, select the actual `pg_range` row by owning range
or multirange OID, require an unambiguous reciprocal association, and follow its
range/multirange/subtype type edges. Copy collation, opclass, canonical and
subdifference OIDs as unresolved provider declarations. Optional zero provider
OIDs remain absent. Type category, naming and a copied opclass OID cannot
establish comparison behavior. Paired [PG17 range definitions](https://github.com/postgres/postgres/blob/REL_17_11/src/include/catalog/pg_range.h)
and [PG18 range definitions](https://github.com/postgres/postgres/blob/REL_18_6/src/include/catalog/pg_range.h)
define these identities.

For each selected enum, copy the complete visible label set with actual row
OID, owner type OID, native sort-order bits and exact label. Keep label and OID
identity distinct from ordering. Presence in a catalog observation does not
authorize native input use of a newly added uncommitted label; PostgreSQL's
enum input safety state is a later execution concern. No codec is selected or
invoked here. See paired [PG17 enum fields](https://github.com/postgres/postgres/blob/REL_17_11/src/include/catalog/pg_enum.h),
[PG18 enum fields](https://github.com/postgres/postgres/blob/REL_18_6/src/include/catalog/pg_enum.h)
and [native enum input checks](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/adt/enum.c).

For every selected domain, copy all its own constraint rows by actual
`contypid`, including constraints on ancestor domains. Preserve actual
constraint OID, name/namespace, kind, owner fields and defining flags. Domain
rows must not be confused with relation constraints that happen to have the
same name. Copy the independently present/absent `conbin` carrier and own its
normalized image through the same native payload mechanism. Do not parse or
plan it. PostgreSQL18 adds defining flags absent from17; record native-major
field presence rather than synthesize18 values on17. Domain NOT NULL and CHECK
declarations do not certify data or frontend wire flags. Paired
[PG17 constraint fields](https://github.com/postgres/postgres/blob/REL_17_11/src/include/catalog/pg_constraint.h)
and [PG18 constraint fields](https://github.com/postgres/postgres/blob/REL_18_6/src/include/catalog/pg_constraint.h)
remain the field source of truth. Relation/FK/trigger constraint closure belongs
to the full statement-dependent graph, not an inferred expansion from a domain.
A selected CHECK with NULL/physically unavailable `conbin`, contradictory owner
fields or an unexpected domain constraint variant must error; no empty-check
or inherited-default substitution is allowed.

Type I/O, typmod, analyze and subscript provider OIDs, collations, range opclass
providers and opaque expressions remain unresolved admission obligations.
`pg_depend` is useful supporting evidence but is not a complete traversal
algorithm: builtin dependencies may be omitted, essential edges live in other
catalogs, and standalone composite ownership reverses the ordinary rowtype
dependency direction. No generic transitive walk of dependency rows can replace
kind-specific native defining contracts. See paired
[PG17 dependency fields](https://github.com/postgres/postgres/blob/REL_17_11/src/include/catalog/pg_depend.h),
[PG18 dependency fields](https://github.com/postgres/postgres/blob/REL_18_6/src/include/catalog/pg_depend.h)
and [type dependency construction](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/catalog/pg_type.c).

## Coherent collection target

Keep one registered nonhistoric catalog snapshot for each logical A/B/C
observation. A must end every scan/snapshot/descriptor increment before any
physical wait. B begins only after the complete native physical attempt and
cache clearing; its source snapshot stays registered through selected external
payload copying. C runs under semantic S only after descriptor/provider paths
have been admitted outside S. All copied maps, selected identities, edge sets,
presence bits and raw carriers must agree before the pure consumer sees owned
B images. ERROR remains native-abort-required; only completed pre-effect
lifecycle/stamp changes can use the bounded retry path.

An added or changed dependency at recheck requires ending S and releasing the
entire physical attempt before a pre-effect retry; no extra guard is acquired
under S. Preserve the entered data-view/FirstSnapshotSet and command identity
without snapshot reset or repair. The source-admitted consumer remains pure C
copying and runs once; release S before descriptor/snapshot cleanup and physical
release/retention. Returned copies become historical, with no frontend handle or
execution lease. They cannot construct an immutable admitted plan.

The collection candidate first scans fixed namespace/class/index/attribute/type/
range maps, builds the selected structural graph using only copied bytes, then
scans attribute/type/attrdef/enum/constraint rows for the selected images and
sets, followed by the existing selected class-options pass. This is twelve full
passes over nine catalog heaps per logical observation: attributes, types and
classes are each scanned twice. Both groups use the same registered snapshot,
unchanged publication/private/context identity and one raw span. Between groups,
pure graph construction uses only copied bytes, without descriptor calls,
providers, SQL, output or native waits. There is no second fence acquisition with
live readers. A stamp change discards the observation, rather than joining old
fixed maps to new images. C cannot use
an A/B selection as a shortcut that misses newly visible defining edges.

This candidate avoids copying every unrelated large default/missing carrier
merely to discover which types matter. Its first fixed maps still scale with
whole-catalog size. Selection cannot be obtained by independent syscache/index
lookups or by evaluating a domain/array/composite codec. A single-pass alternative
would retain variable carriers for all possible future selections; it trades
fewer scans for potentially large unrelated copies and must not silently become
a fallback when the selected-pass design hits a limit.

## New descriptor bootstrap gate

The existing implementation admits a finite six-catalog builtin profile and
selected pinned metadata TOAST descriptors. That acceptance does not admit
`pg_range`, `pg_enum`, `pg_constraint`, constraint TOAST2832 or application
composite descriptors. Additional metadata heaps must join the complete native
physical graph, including all live indexes and declared TOAST, before their
descriptors are opened. Exact AS references remain separate from other modes.

Before choosing the bootstrap algorithm, prove the complete cold and invalidated
descriptor path for each new heap: fresh defining class/attribute/type facts,
physical NULL options, default/missing/generation/rule/trigger fields, builtin
heap handlers, already initialized native critical descriptors/indexes and exact
owned references. The public view currently omits some of these bootstrap facts;
it cannot supply a proof by itself. A post-open check or remembered warm-cache
status is insufficient.

An initial candidate uses the already admitted critical catalog readers to
collect the new heaps' physical/profile facts, ends that snapshot before lock
acquisition, then obtains a fresh coherent profile after acquisition. Normal AS
alone does not establish exclusion of every descriptor-affecting DDL mode.
Additional native modes must be chosen against the actual writer conflict
matrix, not their names. AS plus RX is insufficient to freeze a NULL-options
profile: SET/RESET relation options derives its lock level from the selected
options, and ordinary heap options such as `autovacuum_enabled` require
ShareUpdateExclusive, compatible with both AS and RX. This is a source-level
counterexample to the proposed general writer coverage, not an executed catalog
mutation. See paired [PG17 option lock selection](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/common/reloptions.c),
[PG18 option lock selection](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/common/reloptions.c),
[PG17 writer dispatch](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/commands/tablecmds.c)
and [PG18 conflict matrix](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/storage/lmgr/lock.c).

Evaluate stronger exact native guards against admitting the complete non-NULL
descriptor/options path within the supported provider profile. Stronger catalog
guards can also block ordinary native catalog writers, introduce native deadlock
edges and extend prepared-transaction waits; their retention/cleanup and cost
need explicit proof. A narrower continuous profile must actually establish its
writer exclusions, not assume that a fresh check stays valid after release.
Any new mode requires the native exact-mode ownership contract to change
explicitly; it must not be approximated by RX or silently added under S. Native
cache refresh and every descriptor open remain outside raw/S, under the explicit
continuous builtin callback profile. No catalog snapshot may survive a new
physical guard wait, and all waits must precede S.

The [metadata bootstrap follow-up](native-catalog-bootstrap.md) compares exact
guard costs and follows global/local option and native rebuild paths. Its
preferred complete-options direction remains conditional on functional
registry/provider admission, project reference lifetimes and writer coverage;
it closes no bootstrap or runtime gate.

The bootstrap gate remains OPEN until paired source proof covers all admitted
writers, invalidation rebuilds and cold paths, and explains which exact guards
keep the pre-open facts valid. In particular, supported ordinary in-place hints
must be distinguished from defining schema/provider state. The eventual
implementation cannot substitute extra exclusion modes for missing semantic
facts or assume that modes freeze external libraries or callbacks. Any remaining
unsupported history must fail explicitly before opening a path that depends on
it. This document authorizes no provider or arbitrary extension callback.

## Cost and verification gates

The full candidate's A/B/C definition collection costs36 catalog passes before
additional bootstrap observations or TOAST scans, compared with the current21.
The separately proposed declared type-link stage would cost24 direct passes.
Do not publish a total scan count until the bootstrap sequence is fixed. Account
for its retained references, observations and native cache work separately.
Whole attribute/type fixed maps increase memory; selected graphs and per-seed
provenance must use visited nodes/edges rather than enumerate exponentially many
paths. Retain the existing root/name/exact-reference limits, cap selected types
at4096 in this candidate, and use checked cumulative requested-copy and context
bounds before map/edge/provenance/image allocations. These are phase bounds,
not invocation peak or process-memory acceptance.

Implementation acceptance requires current strict PG17/18 product/probe packages
and the existing required suites, with ordinary independent native fixtures for
nested domains and their independent defaults/checks; array companions and
shared/cyclic graph membership; named/standalone/table composites with dropped
slots and transitive fields; ranges/multiranges with actual subtype associations;
enum addition/rename and domain constraint changes; NULL/empty/inline/external/
compressed constraint/default images; duplicate seeds and unrelated metadata;
fresh unset and established RR; private/own TEMP definitions; both ordinary
prepared DDL outcomes; exact native ownership and release/retention. No result
at the current direct-type revision certifies these new cases.

New catalog layouts, bootstrap guards and payload paths require independent
source review before C changes and a fresh implementation review afterward.
Measure complete ordinary invocation costs and each phase separately, including
all bootstrap scans; do not infer throughput or lock-contention acceptance from
bounded samples. No new stress, catalog corruption, forced error, interruption,
recovery or existing-profile lifecycle experiment is part of this work.

## Primary source record

`logs/native-transitive-type-closure-primary-v1` records17 pinned bodies per major
for PostgreSQL17.11/18.6: eight selected cached bodies rehashed against the fixed
prior manifest and26 new HTTP-200 bodies. Facts SHA256 is
`a6cbe480715de1edfa3b15184014d98f1152e3b3f0ff916bce9e80e62bdfa009`;
the64-member nonself seal SHA256 is
`04346a5952b1cf202851e237cc2fc84ded598984909d5379331b4051d2180773`.
The selected native paths motivate these design gates. The source record does
not certify bootstrap writer coverage, artifact identity, runtime behavior,
provider/expression admission or full transitive statement closure.

The separate v2 primary record rehashes all34 selected bodies and adds four
HTTP-200 lock/writer bodies:19 bodies per major. Its facts SHA256 is
`9496d7a04ac4211a5062e19076314dca298de00b5356e3f302038de1a84f5422`;
the46-member nonself seal SHA256 is
`a7af45690161cb86f44560ae6c7ba47b7bfb6e0ca9a3f204500ec2265c212d17`.
Two previously pinned `reloptions.c` bodies are separately rehashed/read in
`logs/native-transitive-type-closure-reloptions-read-v2`; they are not claimed
as members of the v2 primary body list. The original reader05 fails its
overstrict one-occurrence assertion because the native option occurs in both
definition and parsing tables; distinct reader05b retains both occurrences and
actually exits0. These are passive source reads, not native runtime results.

Full name/negative/overload candidate closure, neutral native Parse/Bind/Describe/
Execute/reanalysis, immutable IR, rollback-capable row execution, MySQL read/lock
equivalence, complete performance, serving and release remain open. Standing
security/compiler/hosted-CI/publication exclusions remain in force.
