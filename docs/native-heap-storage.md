# Private catalog-declared heap storage

Status: the finite C boundary is implemented on the feature branch. Exact
PostgreSQL 17/18 runtime verification and independent implementation acceptance
are pending. The reviewed design is a conditional recommendation within the
continuous builtin private-owner and source-admitted pure consumer profile.
It does not certify application SQL, an executor or complete semantic closure.
Issues46/15 remain open. Concurrent native PostgreSQL two-phase transactions
remain supported; disabling them is not a serving requirement.

`postgres/darmok_server/heap_storage.h` supplies one private C invocation.
Counted exact UTF8 schema/relation names and native AS/RS/RX root modes are
copied before acquisition. There is no search-path alias, folding, truncation,
frontend handle or returned native token. Input bindings preserve duplicates;
each exact OID/mode reference is acquired once. Ordinary nonpartitioned builtin
heaps and builtin btree live indexes are accepted; other kinds/AMs fail explicitly.
Traditional inheritance hints are visible but do not imply child expansion.

The boundary copies fixed fields from preopened builtin pg_namespace, pg_class
and pg_index heaps. It includes every live index, even invalid/not-ready ones,
and the declared TOAST heap and all its live indexes. Conservative relhasindex
does not suppress edges. Roots, then their index siblings in OID order, then
TOAST heaps and their index siblings produce deterministic parent-first native
declarations. Root index modes follow the root. Declared TOAST receives AS for
reads and separate AS plus RX for write-capable roots; stronger modes do not
replace exact native counts. Missing/duplicate/cyclic/inconsistent edges fail.

Actual namespace/temp-toast identities and the native backend are checked;
foreign TEMP is rejected. Native hard-coded shared tags must match copied
catalog facts. Zero tablespace resolves through MyDatabaseTableSpace; global
storage uses database zero. Real mapped file numbers are read outside exclusion
under exact physical references; a missing mapping fails without OID substitution.
Active/uncommitted rewrite and arbitrary callback/descriptor histories are not
admitted by this boundary. This is the catalog-declared graph at the metadata
point. Native future detoast/delete can follow va_toastrelid and rewrite can use
rd_toastoid; a future data consumer must prove that separate closure.

Every observation uses an internal gate-first/raw try-fence. Shared-drop intent
returns false with neither short reference and never CV-sleeps. Sixteen completed
pre-effect lifecycle/generation failures produce serialization failure. Native
LockAcquire/cache waits and ERROR retain their normal deadlock/abort semantics;
they cannot become false retry. Closing this attempt does not remove references
borrowed or retained by earlier native transaction history. The historical SHOW
reader has no new storage-lifetime claim.

Initial scans/snapshot/descriptor increments close before complete physical
acquisition. Globally current SI, installation checks, final fact descriptors
and mapping resolution occur after physical acquisition and before semantic S,
under common native refresh reentry exclusion. S then owns the metadata point.
A second fixed observation checks publication/private/context identity, exact
literal bindings and all defining graph fields. Unexplained same-identity
definition changes fail loudly; statistical/conservative hints are excluded
from definition equality. No descriptor reload or provider lookup occurs under S.

Exactly one source-admitted pure C consumer receives only copied facts with
semantic S and all physical references owned, after raw/gate references end.
It may copy those values into caller memory. No SQL, provider, descriptor,
protocol output, row/tuple/MultiXact/XID wait, transaction mutation or borrowed
fact pointer escape is permitted. This is a caller/source contract, not a
runtime purity detector. Local ownership flags are native token observations,
not a separate lock-table or application-admission certificate.

Cleanup ends raw and reader bookkeeping, releases S, then closes native
scans/snapshots/descriptors and completes physical references. Release uses exact
recorded counts; retain leaves only native transaction/subtransaction counts.
Copied facts become historical as soon as S ends. Volatile tokens and PG_FINALLY
protect the invocation. Entered-invocation ERROR propagates, records the captured
owning subid as abort-required, and never guesses a partial native grant.
Matching native abort is required before reuse/commit/prepare. Native promotion,
child abort and retention remain governed by the reviewed reference primitive.

Uncontended work performs two full three-heap observations, O(catalog-size)
copying, O(V+E) graph traversal, sorting for deterministic roots/index siblings
and O(D log D) exact-declaration validation, plus D native increments and SI/
descriptor/owner costs. Unrelated publications can cause false retries. Root
and exact reference counts are bounded to4096, copied names to1MiB and copied
observation/graph phases to64MiB. Raw copying allocates under its short fence;
graph/serialization work is outside raw. Periodic checks can overshoot before
error; these are not peak or total process-memory bounds. Retained counts accrue
until native transaction cleanup. No performance acceptance is claimed yet.

The separate native test image supplies a pure copying probe; product images
contain no probe or SQL storage entry point. Fresh exact-source native fixtures,
including ordinary prepared outcomes and concurrent index phases, remain required.
Type/default/check/rule/trigger/domain/FK/partition/function/operator/collation
and candidate closure, invocation effects, immutable IR/execution, native
Parse/Bind/Describe/reanalysis admission and MySQL row/lock semantics remain
separate open gates. See [release-plan.md](release-plan.md) for immutable reviews,
source receipts, actual failures and verification checkpoints.
