# Private variable catalog payloads

Status: the private C implementation and bounded ordinary fixtures are present;
paired product/probe builds, required runtime suites and independent implementation
review are pending. This is the contract for the private
[heap-storage invocation](native-heap-storage.md). Source bodies have been
checked for PostgreSQL 17.11/18.6; no implementation, package, runtime or
performance acceptance is claimed here. This work extends
[missing-value images](native-missing-values.md). It keeps concurrent native
PostgreSQL two-phase transactions supported.

## Problem and choice

A default declaration is insufficient for native row interpretation or later
statement admission. A column's default or generation expression lives in
`pg_attrdef.adbin`; a directly referenced type can have `pg_type.typdefaultbin`
and `typdefault`. Both catalogs have TOAST storage. Copying an external pointer
does not copy its value or keep its chunks alive after the catalog snapshot
closes. Fetching it under the semantic reference S can introduce storage/cache
work and provider calls into the wrong lifetime boundary.

The chosen mechanism copies exact source carriers during coherent catalog
observations, acquires their complete physical storage before S, and assembles
selected external values using direct builtin heap scans outside raw/S. A fresh
registered catalog snapshot stays alive from the observation that supplied
those pointers until their chunks have been copied. A final observation under
S must agree before the pure C consumer receives the owned images.

Two alternatives are rejected for this component. Keeping the first snapshot
across physical acquisition would unnecessarily retain its vacuum horizon
through potentially indefinite native lock waits. Calling the ordinary native
detoaster would follow `va_toastrelid`, open indexes and use indexed support
machinery. Admitting that path needs the full index/provider closure, which this
metadata-copy component does not establish. Permanent rejection of ordinary
TOAST-backed defaults is also unacceptable. Direct heap assembly supports them
without expression evaluation; its extra scanning cost must be measured.

## Facts and exact source identity

One observation scans six builtin catalog heaps under one registered
nonhistoric catalog snapshot: `pg_namespace`, `pg_class`, `pg_index`,
`pg_attribute`, `pg_type` and `pg_attrdef`. Existing literal root, positive-slot,
live-type and dropped-slot rules remain. Inspect every `pg_attrdef` row whose
actual `adrelid` is a selected root; require its ordinal to identify an existing
positive slot. Include a row that contradicts the copied column flags;
inconsistent or out-of-range rows must be detected rather than filtered away
using `atthasdef` or an ordinal range predicate.

Each live selected column with `atthasdef` requires exactly one row with its
actual `adrelid`/`adnum`, nonzero unique attrdef OID and non-NULL `adbin`.
No other selected slot may have an attrdef row. Generation and identity remain
separate column declarations: generation expressions use the attrdef image;
an identity declaration alone does not fabricate an expression or sequence
dependency. Dropped slots cannot have an admitted expression. Names, NULL
declarations and generation flags do not certify a MySQL wire definition.
Require the generation declaration to be one of the pinned major's native
values, and a nonempty generation declaration to have `atthasdef` and its
expression record. It must not coexist with an identity declaration.

For every directly referenced live type, copy the independent presence and
source carrier of both `typdefaultbin` and `typdefault`. A present binary
expression requires present text, as declared by the native catalog; text-only
defaults are valid and stay text-only. Absence and an empty present value remain
distinct. No domain base, element, composite or inherited effective default is
substituted for these directly observed facts. Transitive types remain a
separate required closure.

Column-expression records carry attrdef OID, actual root OID and ordinal.
Type-default records carry actual type OID and which of the two fields they
describe. Missing images keep their relation/ordinal/element-type identity.
Duplicate roots and repeated type references share ordered records. Internal
source keys include the source catalog, exact native row identity and target
ordinal; equal bytes from different objects do not merge their identities.

The output consists of invocation-owned, flat, uncompressed native varlena
images with explicit lengths and presence. They are opaque PostgreSQL-major/
architecture-specific bytes, not frontend values or executable nodes. Do not
call `stringToNode`, `pg_get_expr`, type input/output, expression evaluation,
domain checks, a type cache or a codec to produce them. `adbin` may itself name
functions, sequences, types and other relations; copying it admits none of
those dependencies or effects.

## Carrier and physical storage admission

Prepare the six catalog descriptors outside raw/S under the existing native
refresh exclusion. Check their actual builtin heap routine, compiled column
counts and each accessed target's native OID/by-reference/varlena/alignment
layout. Every selected target must physically exist in the tuple, even when
NULL. Use the admitted `fastgetattr`/`nocachegetattr` layout walk only after that
check; never enter `getmissingattr`. Before inspecting a carrier header or
copying its bytes, check its pointer and complete header/value range against
the tuple's actual byte bounds. Layout traversal of preceding fields is not
semantic interpretation of those fields.

Short, ordinary four-byte, inline-compressed and on-disk external carriers are
supported. Indirect, expanded and unknown carriers error before dereference or
flattening. Inline missing arrays retain the reviewed `pg_attribute` profile
with no declared TOAST relation. The shared carrier mechanism must preserve its
double alignment and singleton-array validation; default strings do not use
the user column's layout.

For an external carrier, copy the complete native pointer into aligned owned
memory only after validating its tag-specific stored length. Validate value
OID, actual `va_toastrelid`, raw/stored sizes and checked allocation/chunk-count
arithmetic. The actual target must equal the source catalog's copied declared
TOAST OID in this ordinary, nonrewrite native catalog profile. Independently
validate the target's copied class/storage/namespace facts; matching an OID is
not storage closure. Active/uncommitted rewrite or retargeted catalog storage
is an explicit unsupported profile, never an implicit fetch from another heap.
Ordinary external values in the declared catalog TOAST heaps are required.

The combined physical graph contains the existing root graphs and AS graphs
for all six fact catalogs: heaps, all live index siblings, declared TOAST heaps
and their live index siblings. Every actual pointer target must occur in that
graph. Preserve deterministic parent-first ordering, distinct exact native
OID/mode counts, mapped-file resolution and shared/temp/database/tablespace
validation. Deduplicate shared graph nodes and exact references across catalog
and application roots; retain provenance for each use. The combined count must
fit the existing 4096-reference limit. No descriptor/AM provider is opened for
an unvalidated pointer target.

These catalog storage nodes do not become additional application root bindings
or expand the selected application column/type set. Their physical carrier
layouts are validated separately.

## Pre-open descriptor admission

Do not open a selected TOAST descriptor before B has freshly observed and
validated its actual profile. A declared target and an eventual descriptor
postcheck are insufficient: `RelationBuildDesc` builds attributes before its
table-AM routine. Attribute construction can fetch missing arrays, defaults and
constraints, and a general relation open can initialize a catalog-selected
handler. A later stamp mismatch cannot undo an unadmitted callback.

This component admits the continuous builtin bootstrap catalog profile, not
arbitrary catalog descriptor/provider histories. Its selected pointer targets
are exactly the compiled `pg_attrdef` TOAST heap 2830 and `pg_type` TOAST heap
4171. In both pinned majors, `IsCatalogRelationOid` classifies these pinned
OIDs without a catalog lookup. Together with the checked heap AM and native
kind, that selects the hardwired `F_HEAP_TABLEAM_HANDLER` branch; TOAST kind
alone has no such shortcut. Native builtin function dispatch and catalog index
support belong to the existing source-admitted builtin profile; this is not
general provider or extension-hook admission.

Each A/B/C raw observation copies additional descriptor-profile facts for the
selected metadata TOAST heaps and the two prerequisite critical indexes.
They remain metadata storage nodes, not application column bindings. From the
actual class row and physically present fields, require the compiled target
identity, `RELKIND_TOASTVALUE`, `HEAP_TABLE_AM_OID`, `PG_TOAST_NAMESPACE`, native
permanent/local-database storage identity, exactly three positive attributes,
no recursive TOAST target or rewrite identity, zero `relchecks`, and no rule,
trigger or other descriptor-loading branch beyond the native bootstrap profile:
`relhasrules`, `relhastriggers`, `relrowsecurity` and `relispartition` are false.
Check `pg_class.reloptions` is physically present and NULL before using its
NULL bit; do not detoast or parse a non-NULL option carrier.
The prepared fact descriptor must match the native four-byte alignment of
`reloptions` (`text[]`); `attmissingval` (`anyarray`) separately requires native
eight-byte alignment. An array's declared element layout does not justify
substituting another array type's alignment.

Copy every positive attribute row for the target, not just ordinals 1–3, and
reject duplicates, extras, dropped slots, missing/default/generated/identity
declarations or contradictory expression records. The three rows must match
the native `(chunk_id oid, chunk_seq int4, chunk_data bytea)` types, lengths,
by-value/alignment, ordinals and names, with plain storage, no compression,
native typmod/dimension/collation declarations and no missing carrier. This
profile is established from fixed facts/presence without following an array or
expression. Native catalog NOT NULL flags do not require a constraint fetch:
PG17 checks `relchecks`; PG18's catalog classification skips the additional
noncatalog NOT NULL path. Do not substitute that rule for the other checks.

NULL options are the bootstrap metadata-TOAST profile: `BootstrapToastTable`
passes a zero options Datum, and `extractRelOptions` returns before any parser
when the class field is NULL. `validate=false` alone is not callback admission;
registered string options can still invoke fill callbacks. Altered options or
structural/provider definitions of these pinned catalog descriptors are an
unsupported profile for this component. This restriction does not reject large
or compressed ordinary defaults. Admitting mutable catalog options requires
their independent carrier/provider closure rather than a permissive postcheck.

The backend must have completed native critical relcache initialization:
`criticalRelcachesBuilt` is already true, and `ClassOidIndexId` and
`AttributeRelidNumIndexId` are the native nailed, fully initialized indexes
with their source-admitted builtin btree support. Check their actual fixed
class/storage/namespace identity and physically present NULL options in B
before entering a descriptor path. Do not set the flag, emulate startup, cold
build replacement critical support or accept arbitrary callback history. Native
startup initialization is a prerequisite, not work performed under B.

After B's raw/fact scans end and A/B facts agree, open selected TOAST heaps
under native refresh exclusion with B's snapshot still registered. Use `NoLock`
for the target because the exact AS reference is already owned. A cold TOAST
entry uses `RelationBuildDesc`; an invalidated warm entry uses its full rebuild.
Their admitted paths read only `pg_class` and `pg_attribute` using the two
initialized critical indexes, or builtin heap scans if native index use is
disabled. A critical index invalidation uses `RelationReloadIndexInfo`, retaining
its initialized support and refreshing class/options/physical identity; it does
not rebuild arbitrary AM/opclass support. NULL options exclude option callbacks
in that path too. The continuous profile excludes structural/options/provider
mutations of these descriptors while this path runs; ordinary application DDL
and global SI refresh are still allowed.

Before B registration, the physical attempt must already own AS for both
underlying catalog heaps, both exact critical indexes, all six fact graphs and
every selected TOAST target. Native nested opens may increment those already
owned tags but may not wait for a new conflicting relation tag while B is
registered. Descriptor reference increments and native lock increments are
separate resources; record and close each exact count without disturbing the
physical attempt. A descriptor postcheck confirms the admitted facts, builtin
routine and no rewrite target; it is never the admission proof. Unexpected
prerequisite/profile disagreement fails before a new descriptor path. Buffer
and IO work outside raw/S retain native waits and ERROR behavior.

## Observation and fetch ordering

1. **A — discover.** Prepare descriptors, obtain the expected native stamp and
   make one gate-first/raw try-fenced coherent six-catalog observation. Copy
   defining fixed facts and selected raw carriers without external fetch or
   provider execution. Close all A scans, its registered snapshot and descriptor
   increments before building/acquiring the combined physical graph.
2. **Acquire.** Obtain every exact physical reference outside raw/S. Refresh SI,
   installation, fact descriptors and real mappings under the existing native
   refresh exclusion. Finish every acquisition that could wait for a new
   conflicting relation tag before registering B. Do not open a selected TOAST
   descriptor yet; its fresh pre-open validation belongs to B.
3. **B — own the payload source.** Make a new coherent six-catalog observation
   with its own registered nonhistoric catalog snapshot. Its publication/private/
   backend/database/context identity must agree with A. End B's raw/gate and
   fact scans, keeping only its snapshot and prepared descriptors alive.
   Build/compare the complete graphs, selected identities, fixed defining facts,
   descriptor profiles and raw carriers outside raw/S. A completed stamp/lifecycle
   change releases this attempt and retries; unexplained same-identity
   disagreement errors. Validate B's pre-open profile, then prepare its admitted
   TOAST descriptors outside raw/S while its snapshot remains registered, using
   only the already-owned native relation tags and initialized prerequisites.
4. **Fetch.** Using only B pointers and admitted descriptors, scan each selected
   actual TOAST heap once outside raw/S, with B's registered snapshot still alive. Assemble and
   normalize selected payloads into owned memory. Check context/snapshot state
   throughout. Close TOAST scans, B's registered snapshot and every B descriptor
   increment when fetching ends. No B snapshot survives a subsequent physical
   lock wait. Unregister the owned copy and invalidate the ephemeral catalog
   snapshot during this completed reader cleanup; do not alter caller snapshots.
5. **C — validate at the metadata point.** Refresh current SI/installation and
   final descriptors outside S, then acquire S and make the final coherent raw
   six-catalog observation. Release raw/gate before graph construction and
   comparison. C's full stamp, graph, identities, defining fixed facts, NULL
   presence, descriptor profiles and exact source carriers must agree with B
   (and A). Never fetch,
   decompress, reload a descriptor or follow a new pointer under S.
6. **Consume and close.** Invoke the source-admitted pure C consumer exactly once
   with C's validated identities/facts and B's owned normalized images, while S
   and the complete physical attempt are owned. No borrowed pointer escapes.
   End S before closing C readers/descriptors and releasing or retaining the
   exact physical attempt. As in the existing private contract, retaining the
   combined attempt retains its catalog references too; record that contention
   and lifetime cost. The images become historical as soon as S ends.

The third observation is intentional. Closing A before physical waits removes
its chunk-lifetime protection; merely re-registering a later snapshot cannot
resurrect A's old chunks. B observes pointers and protects their lifetime
together. C establishes the final metadata point without fetching under S.

This protocol never obtains a transaction data snapshot, changes command IDs,
executes SQL or repairs native snapshot state. `FirstSnapshotSet`, native owning
context and caller snapshots must remain as entered. Existing RR snapshots are
preserved; fresh RR transactions remain unset. Short native buffer/cache work,
hint bits and statistics are not application statement effects. External
metadata IO can wait for native buffer/IO resources outside S.

Only completed pre-statement-effect stamp/lifecycle changes retry, at most
sixteen attempts. A native ERROR propagates with the captured subtransaction
marked abort-required; it cannot become false/retry or partial success. Unwind
owns every scan, snapshot, descriptor, memory context and physical/semantic
token, with native error-safe volatile bookkeeping. Caller-owned, borrowed or
previously retained native references remain untouched.

No semantic/raw reference or first catalog snapshot is held while waiting for
physical relation locks. Native prepared DDL can legitimately block acquisition
or S for its prepared lifetime; it does not require disabling two-phase
transactions. The already reviewed native coverage/publication primitive still
governs PREPARE and prepared completion. This design does not claim new recovery,
interruption or full MySQL lock-equivalence verification.

## Direct builtin TOAST assembly

Use direct `heap_beginscan`/`heap_getnext`/`heap_endscan`, zero scan keys, no
parallel scan and `SO_TYPE_SEQSCAN`. PostgreSQL disables page mode for the
non-MVCC TOAST snapshot. There is no table/index dispatch, comparator or scan-key
function. PostgreSQL 17 requires `init_toast_snapshot` while B's snapshot is
registered; PostgreSQL 18 uses `get_toast_snapshot` under the same lifetime
condition. Do not use SnapshotAny, a transaction snapshot or an MVCC catalog
snapshot in place of native TOAST visibility. TOAST rows are immutable for
the selected live pointer; B's registered snapshot protects them from removal.
The native TOAST visibility rule does not independently recheck the source
catalog row's MVCC visibility, so that source observation is essential.

Group requests by `(actual TOAST OID, value OID)`, rejecting contradictory
pointer sizes/compression facts for a shared value. Read chunk OID/sequence
through validated physically present fixed fields; skip unselected value IDs
without interpreting their data. For selected chunks, all three fields must be
non-NULL. Only plain or short inline bytea chunk data is valid; compressed,
external, indirect or expanded chunk data errors without recursive fetch.
Check the selected chunk's complete byte range before copying.

Chunks can arrive in any heap order. Allocate checked destination/seen-state
sizes first; require each sequence exactly once in `0..chunk_count-1`, the native
full chunk size except for the exact final remainder, and complete coverage at
scan end. Negative/out-of-range/duplicate sequences, inconsistent lengths,
missing chunks or contradictory pointers error. Zero stored bytes are accepted
only for an uncompressed pointer with raw size exactly VARHDRSZ, valid value/
target identity and the same validated source/storage closure. Its plain image
is an empty payload; it needs no chunks. Other zero-size combinations error.
No native index order is assumed.

Reconstruct the native compressed/plain carrier and normalize only outside
raw/S. Select the builtin pglz/LZ4 decoder from the stored compression tag,
independently of the column's current declaration. Check compressed header,
pointer/header size agreement, raw output bound and actual decoded length;
LZ4 returning fewer bytes than its header promised must not become success.
An unavailable required decoder or unknown compression identifier errors.
The final owned plain image is checked against its declared raw length before
publication. No codec, expression parser or provider is part of normalization.

## Cost and implementation gates

Each successful invocation makes three full six-catalog observations: eighteen
catalog scans instead of the current ten scans over five catalogs. Namespace/class/
index maps still scale with catalog size. The pre-open profile copies use those
same scans and add bounded selected class/attribute/presence records, not a fourth
observation. Selected attrdef/type payload copying
scales with selected records/bytes. TOAST scanning costs one full heap scan per
selected actual metadata TOAST heap, independent of selected value count;
lookup/assembly costs scanned chunks plus selected stored/decoded bytes and
seen-state. This is a potentially material regression for small queries against
large catalogs, not an inherited performance pass. Indexed fetching is a later
alternative only with complete provider/index admission, never a silent fallback.

The existing 1MiB counted-name and 4096-root/reference limits remain. Apply the
64MiB per-phase/context and MaxAllocSize checks before raw carrier, destination,
seen-state, array and decoder allocations with checked cumulative arithmetic.
A, B, C, graph and decoded copies can coexist; a per-phase limit is not an
invocation peak or total process-memory limit. Periodic context checks can
overshoot through block overhead. Record per-observation scanned/selected rows,
raw and normalized bytes, TOAST heaps/chunks scanned and selected, allocated
context bytes, exact reference counts, attempts and ordinary elapsed costs.
Do not present context bytes as allocation-event counts or process memory.

Implementation acceptance requires paired actual PostgreSQL 17/18 packages
and the existing required suites, plus bounded ordinary fixtures for:

- absent/inline/short/large external column and direct-type defaults, generation
  expressions (stored on both majors; virtual on 18), and distinct NULL/empty
  fields, without evaluation;
- actual compressed/uncompressed external carriers, multiple selected values
  and shared roots/types, unrelated large values and order-independent assembly;
- DROP/SET DEFAULT between completed invocations, missing-value
  retention independent of the default, renamed/dropped slots and exact identity;
- fresh unset RR and established RR paths, native owner/subtransaction cleanup
  and ordinary prepared commit/rollback outcomes with two-phase enabled;
- ordinary unsupported profile boundaries and source-level checked arithmetic/
  chunk/compression/error cleanup review, without corrupting native catalogs or
  adding forced-error, stress, interruption, recovery or profile-lifecycle tests.

Fixtures must demonstrate the actual stored carrier/decoder they name; string
length or a compression GUC alone is insufficient. New C inputs require current
strict GCC/LLVM product/probe builds and exact source-to-image binding. Existing
packages/results cannot certify changed native code. Independent implementation
review must inspect accessor/descriptor/provider paths, all three observation
lifetimes, deduplication, NULL/generation cases, compression lengths and cleanup.
The independent design readback and all implementation gates remain separate.

Full transitive descriptors/providers/default dependencies/constraints/effects,
neutral Parse/Bind/Describe and reanalysis, immutable statement admission, row
execution, MySQL read/lock equivalence, serving and release gates remain open.
Security work, compiler PR4, hosted CI/account work and release publication are
outside the current work scope.

## Primary source evidence

The finite local manifest `logs/native-variable-catalog-payloads-primary-v3`
records 32 source bodies per major, rehashes the preceding 72-member source
record and adds 16 HTTP-200 bodies for descriptor classification/opening,
bootstrap TOAST construction, AM initialization and options/index machinery.
Across v1/v2/v3 there are 34 cached and 30 newly captured bodies. Each seal is
constructed from its member map before its own file is opened. V3 facts SHA256 is
`c6bf6baf6b4b09c49790f214488b12dd1e2409eebfc7cd29cfe619ce016abc7a`;
its 107-member seal SHA256 is
`89d75367952654a44b252bc39eb8a6fdd7af32826327190a242957afeee0b8f6`.
These are source/provenance records, not Git-blob, installed-binary or runtime
identity. Exact command/source bindings are in the release ledger.

Key paired primary definitions are the
[PG17 attrdef catalog](https://github.com/postgres/postgres/blob/REL_17_11/src/include/catalog/pg_attrdef.h),
[PG18 attrdef catalog](https://github.com/postgres/postgres/blob/REL_18_6/src/include/catalog/pg_attrdef.h),
[PG17 type defaults](https://github.com/postgres/postgres/blob/REL_17_11/src/include/catalog/pg_type.h),
[PG18 type defaults](https://github.com/postgres/postgres/blob/REL_18_6/src/include/catalog/pg_type.h),
[PG17 TOAST snapshot](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/common/toast_internals.c),
[PG18 TOAST snapshot](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/common/toast_internals.c),
[PG17 heap visibility](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/heap/heapam_visibility.c)
and [PG18 heap visibility](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/heap/heapam_visibility.c).
Snapshot registration, heap scan, accessor, attrdef writer, compression and
predicate-lock source bodies are also pinned in that manifest. The selected
mechanism above is Darmok's design derived from those native paths, not a claim
that PostgreSQL supplies this whole invocation as one API.

The pre-open correction uses paired
[PG17 descriptor paths](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/relcache.c),
[PG18 descriptor paths](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/relcache.c),
[PG17 catalog classification](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/catalog/catalog.c),
[PG18 catalog classification](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/catalog/catalog.c),
[PG17 bootstrap TOAST construction](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/catalog/toasting.c)
and [PG18 bootstrap TOAST construction](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/catalog/toasting.c).
Native table/relation/index opens, heap/btree handler initialization and the
options parser's NULL return and callback branches are included in v3.
