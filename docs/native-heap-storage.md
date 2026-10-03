# Private catalog-declared heap storage

Status: the original finite private C storage component passed local PostgreSQL 17.11/18.6
verification and independent implementation review at
`2640513033227cfe00ad5be4511b12e85491c09c`. Acceptance is confined to the
continuous builtin private-owner and source-admitted pure consumer profile.
It does not certify application SQL, an executor or complete semantic closure.
The fixed column/type declaration extension also passed local verification and
independent implementation review at `1934034e28d7fda64953c1693104b248c37c42ea`
within that finite profile. See
[native-attribute-type-facts.md](native-attribute-type-facts.md). Issues 46/15
remain open. Concurrent native PostgreSQL two-phase transactions
remain supported; disabling them is not a serving requirement.

The variable-payload implementation passed paired local package/runtime
verification and independent implementation review at
`82c72068716545da8cb6dc964ee6ea8238b5a4c6`, within the finite builtin profile.
The earlier component reviews above do not certify this changed native code.
Its full source/carrier and pre-open descriptor contract
is [native-variable-catalog-payloads.md](native-variable-catalog-payloads.md).

`postgres/darmok_server/heap_storage.h` supplies one private C invocation.
Counted exact UTF8 schema/relation names and native AS/RS/RX root modes are
copied before acquisition. There is no search-path alias, folding, truncation,
frontend handle or returned native token. Input bindings preserve duplicates;
each exact OID/mode reference is acquired once. Ordinary nonpartitioned builtin
heaps and builtin btree live indexes are accepted; other kinds/AMs fail explicitly.
Traditional inheritance hints are visible but do not imply child expansion.

The storage component copies fixed fields and selected raw carriers from six
preopened builtin heaps: pg_namespace, pg_class, pg_index, pg_attribute, pg_type
and pg_attrdef. Exact-root/live-type filtering does not expand the application
column set to include metadata storage nodes. It includes every live index, even invalid/not-ready ones,
and the declared TOAST heap and all its live indexes. Conservative relhasindex
does not suppress edges. Roots, then their index siblings in OID order, then
TOAST heaps and their index siblings produce deterministic parent-first native
declarations. Root index modes follow the root. Declared TOAST receives AS for
reads and separate AS plus RX for write-capable roots; stronger modes do not
replace exact native counts. Catalog graphs require AS and share exact references
with overlapping application roots. Per-root application/catalog use records
preserve provenance; duplicate physical nodes union modes. Missing, cyclic or
contradictory parent edges fail.

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
A middle coherent observation supplies source carriers while retaining its
registered catalog snapshot through external assembly and inline normalization
outside raw/S. Fresh pre-open metadata-TOAST/critical-index descriptor profiles
must agree with the initial observation. All required relation tags are already
owned before this snapshot; selected descriptor opens use NoLock and may not
wait for another conflicting relation tag. Reader cleanup closes this horizon
and descriptor increments, and invalidates the ephemeral catalog snapshot
before final preparation or any later wait. A final coherent observation under
S checks publication/private/context identity, exact literal bindings, carrier
presence/bytes and all defining graph fields. Unexplained same-identity
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

The accepted original component performed two full three-heap observations and
the fixed column/type extension performed two five-heap observations. The current
variable-payload implementation requires three six-heap observations and one
direct scan per selected metadata TOAST heap. It reports all three selected-copy/
context costs and the additional raw, normalized and heap/chunk costs. Original
namespace/class/index maps remain O(catalog-size) copying, O(V+E) graph traversal, sorting for deterministic roots/index siblings
and O(D log D) exact-declaration validation, plus D native increments and SI/
descriptor/owner costs. Unrelated publications can cause false retries. Root
and exact reference counts are bounded to 4096, copied names to 1MiB and copied
observation/graph phases to 64MiB. Raw copying allocates under its short fence;
graph/serialization work is outside raw. Periodic checks can overshoot before
error; these are not peak or total process-memory bounds. Retained counts accrue
until native transaction cleanup. Finite ordinary costs are recorded; allocation,
throughput, contention and full performance acceptance remain open.

For the accepted original component at tested `2640513`, the separate native
test image supplies a pure copying probe; product images
contain no probe or SQL storage entry point. Six new ordinary fixtures pass on
both majors, including exact literal/duplicate bindings and counts, release and
native retention, mapped/shared/own TEMP storage, explicit unsupported kinds,
both normal prepared DDL outcomes and live not-ready/invalid concurrent indexes.
The complete native package passes 143 tests per major, including those six;
three explicitly selected private-owner checks also pass per major. No test is
ignored in these runs. At the accepted PR54 leaf, all 20 product/probe build inputs, including packaged
documentation, were byte-identical to built `aacfe42cab42f6f0b4b08a3e866a9cb3e6485ab7`.
Those historical results do not certify the current extension.
Primary and ordered profiles keep native `max_prepared_transactions=10`.
Type/default/check/rule/trigger/domain/FK/partition/function/operator/collation
and candidate closure, invocation effects, immutable IR/execution, native
Parse/Bind/Describe/reanalysis admission and MySQL row/lock semantics remain
separate open gates. See [release-plan.md](release-plan.md) for immutable reviews,
source receipts, actual failures and verification checkpoints.
