# Private fixed column and type facts

Status: the finite fixed-fact extension is implemented on the feature branch,
following conditional design review at clean `23842c4` on PostgreSQL 17.11/18.6
primary source evidence. Exact-source build/runtime and independent
implementation verification are pending. This extends the [private heap-storage
invocation](native-heap-storage.md); it does not admit SQL or an executor.

One coherent raw observation copies fixed `pg_namespace`, `pg_class` and
`pg_index` maps, resolves exact counted literal root names to actual OIDs, then
fully scans `pg_attribute` and `pg_type` under the same registered nonhistoric
catalog snapshot. Only positive slots for the selected roots and fixed type
rows referenced by their live slots are copied. Pure literal/hash construction
rejects missing roots and duplicate identities during copying; native ERROR
unwinds and requires abort. Full cardinality/layout/schema validation and
graph construction occur after raw release from the coherent copies. All five descriptors are
prepared outside raw exclusion and semantic S. No index/syscache/provider lane
or join across independently stamped observations replaces these scans.

Each input binding receives its actual rowtype OID, declared column count and
range in the shared ordered attribute array. Duplicate roots share ranges.
Every ordinal `1..relnatts`, including dropped slots, must occur exactly once.
Empty heaps have empty ranges. Dropped slots retain layout with actual type
zero, even after the former type has been dropped. Live slots require exactly
one defined type row. Names are exact and padding is normalized; signed native
lengths, typmods and dimensions are retained. No type or rowtype OID is guessed.

Attributes describe names, type OIDs, ordinals, length/by-value/alignment,
typmods, dimensions, storage/compression, NOT NULL/default/missing/identity/
generation declarations, dropped/local/inherited status and collation OIDs.
Types describe actual namespace/name, layout, kind/category/preference/defined
status, delimiter, relation/subscript/element/array/provider OIDs, storage,
domain base/typmod/dimensions, NOT NULL declaration and collation OID.
Only live `attlen/attbyval/attalign` must agree with `typlen/typbyval/typalign`.
Column storage/compression, typmods, dimensions, collation and nullability may
legitimately differ from type defaults. They remain independent defining facts.
Native cache offsets and PostgreSQL 18 compact descriptor state are excluded.

These are descriptions, not a constructed TupleDesc, TypeCache entry, codec
choice or transitive dependency graph. OID edges are not followed. No variable
default, missing value, expression, enum label, constraint, rule, trigger,
provider or collation configuration is fetched, detoasted or evaluated.
In PostgreSQL 18 the raw column NOT NULL declaration can name an invalid
constraint; it cannot certify NULL-free data or a MySQL NOT_NULL wire flag.

The existing whole-invocation protocol is mandatory: every observation uses
the gate-first/raw try-fence; initial scans/snapshot and all five descriptor
increments close before physical waits. Current SI, installation and final
descriptor preparation happen before S under native refresh exclusion. At S,
publication/private/backend/database/context identity, complete storage graph
and all defining root/attribute/type fields must agree. An unexplained
same-identity change errors. Only completed pre-effect lifecycle/generation
changes can retry, up to sixteen times; native ERROR requires native abort.

The source-admitted pure C consumer runs once after raw release with semantic
and exact physical references owned. It may copy facts but cannot invoke SQL,
providers, descriptors, output, data waits or transaction mutation, and no
borrowed pointer escapes. S ends before native reader cleanup and physical
release/retention. Copies then become historical. Borrowed/native-retained
counts and ordinary subtransaction/prepared outcomes remain native-owned.
Concurrent PostgreSQL two-phase transactions remain supported.

Both complete observations still perform five full sequential catalog scans.
Added copied attribute/type memory scales with selected slots/types; the
original namespace/class/index maps still copy all their rows. Deterministic
ordering, hashing, array construction and defining comparison add costs.
4096 root/exact-reference and 1MiB counted-name limits remain; each copied
observation/graph phase has a periodic 64MiB check with possible overshoot.
This is neither a peak nor total process-memory bound. Record each catalog's
scanned rows, selected attribute/type counts, defined array bytes and each
observation context's allocated bytes separately. Context allocation excludes
other process/cache/consumer contexts and does not count allocation events.

Required paired ordinary verification covers exact literal/duplicate/mode
bindings, empty/dropped/renamed/maximum-byte names, true layout equality and
valid per-column overrides; domains/arrays/enums/composites and builtin-like
type names remain descriptive. Removed dropped types, repeated live type IDs,
missing/default/identity/generation flags, cold/warm descriptors, actual TEMP,
native ownership and prepared commit/rollback must be exercised. Strict fresh
product/probe images, affected and complete native groups, private-owner checks,
static checks, final environment, finite costs and independent implementation
review remain pending. Historical tests certify their historical source only.

Full variable/transitive descriptor/provider/name/candidate closure, effects,
immutable executable binding, native Parse/Bind/Describe/reanalysis admission,
data-derived storage and table execution, MySQL read/lock equivalence, serving
and release gates remain open. See [release-plan.md](release-plan.md).
