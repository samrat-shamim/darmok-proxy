# Owned relation option images

Status: implemented in the working branch; current paired native builds,
required ordinary fixtures and independent implementation review are pending.
Historical results do not certify this change. This extends the private
[heap-storage invocation](native-heap-storage.md) with defining raw carriers,
not an options parser, new descriptor admission or a table executor.
Concurrent native PostgreSQL two-phase transactions remain required.

## Meaning and source identity

Every distinct node in the selected application/catalog storage graph has one
`pg_class.reloptions` payload record. It identifies the actual source catalog,
class row OID and the pinned major's actual field ordinal. Class records use
zero for the extra relation/column fields reserved for column-expression
records. Duplicate roots and different native modes share this one source
record; equal images from different class rows remain different records.

Copy actual NULL presence and stored bytes. A present empty array is different
from NULL, and native parsed defaults or `rd_options` never replace this
defining carrier. The middle observation supplies the owned flat native
varlena image. The array remains opaque, local to the PostgreSQL major and
architecture: no element decoder, array transformation, options parser,
provider or expression evaluation runs in this component.

This covers ordinary application heap, live btree index and declared TOAST
nodes as well as the selected metadata storage graph. It does not interpret
which option affects planning or admit opening an application descriptor.
The four opened bootstrap TOAST/critical-index descriptors retain their
existing physically present NULL-options checks and continuous builtin profile.
Capturing other class options does not relax those prerequisites.

## One coherent raw observation

Prepare the existing six builtin fact descriptors outside raw/S under owned
seed or complete graph references. Their checked class target layout includes
the actual `reloptions` text-array OID, varlena/by-reference representation and
native integer alignment. Register one nonhistoric catalog snapshot and retain
the existing raw publication span while collecting fixed facts.

Construct the graph from those copied facts in that same span. Its work is
bounded hashing, native allocation, sorting, fixed membership checks and the
native current-temp namespace predicate; it opens no descriptor, parses no
option, resolves no provider and selects no data snapshot. The selected native
temp predicate reads only the current backend's namespace identities.

Then begin one further sequential class scan through the already admitted
descriptor and the same registered snapshot. Use the existing graph node map
to select sources; do not copy unrelated option arrays or duplicate graph
selection. Each selected OID must appear exactly once at its graph position.
Track capture separately from NULL presence so an uncopied source cannot
masquerade as a captured NULL. Missing, duplicate or inconsistent sources fail
with an error. The bootstrap descriptor NULL bit must agree with the carrier.

The selected native `pg_class` declaration has no TOAST relation. Require its
fresh copied no-TOAST identity before this additional pass and reject external
option carriers before following a pointer. The existing bounded inline
carrier copier physically checks the ordinal and value extent before copying;
it does not enter `getmissingattr` or a detoaster.

Construct the ordered generic payload sources only after capture completes.
End raw bookkeeping before cleanup. A closes every scan/snapshot/descriptor
and releases its complete seed before the full physical graph wait. B closes
its scans and retains its source snapshot through normalization outside raw/S,
then closes that horizon before later preparation/waits. C repeats fixed
selection and raw option capture under S, compares every source identity,
presence and exact stored carrier with A/B, and uses B's already owned images
for the single pure copying consumer. Generation/lifecycle retry still drops
the whole attempt; native errors keep normal abort requirements.

The extra scan has its own returned reference pointer. Cleanup detaches it
before native `heap_endscan`, after raw and semantic exclusion have ended.
Invocation backing survives an uncertain native error until the actual
transaction/subtransaction cleanup, as in the reference-scope contract.

## Choice and cost

Copying all class option arrays in the first pass would avoid another scan,
but unrelated definitions would add allocations and consume the phase budget.
Separately reacquiring a raw fence after graph selection would introduce a
new wait while this observation's reader resources are alive. The selected
construction instead keeps one coherent raw span and scans selected carriers
after graph selection within it.

There are seven sequential catalog scans per observation, twenty-one per
successful A/B/C attempt, plus the existing selected metadata TOAST scans.
Six descriptors are opened per observation. The extra scan adds one returned
heap-scan reference per observation; physical seed/full-graph modes and counts
are unchanged. No SQL/protocol round trip or options callback is added.

Graph selection/validation and payload source construction now also occupy
the raw span. Their existing hashing, sorting and allocation work therefore
holds publication exclusion longer. That contention cost needs measurement;
this change makes no latency or throughput claim. Option scanning is linear
in class rows, and copied carriers/arrays are proportional to selected graph
nodes and their option bytes. The carrier array and capture bitmap obey the
4096-node graph bound and the cumulative 64 MiB phase budget. Normalization
adds its owned image bytes in B. These limits are not process/peak-memory
certificates.

Report all extra class rows as `options_rows`, independently from the first
class pass's `relation_rows`. Include actual selected stored carrier bytes in
payload copy accounting and all image/source allocations in the existing
cumulative requested-byte budget. Unrelated option bytes are not copied or
reported as selected payload bytes.

## Writer and verification scope

Normal ALTER TABLE/INDEX SET/RESET routes are metadata utilities in the
existing native publication boundary. The selected complete native
`ATExecSetRelOptions` bodies update actual class option arrays, issue
post-alter hooks and propagate parent TOAST options in both pinned majors.
PostgreSQL 18 no longer admits the direct TOAST-kind entry branch; the parent
TOAST propagation branch remains. Native prepared completion uses the existing
global completion fence and real two-phase target. AS does not freeze SUE
option writers, so the observed carrier and publication identity remain
required facts. A prepared metadata writer retains semantic RX even when its
physical SUE modes coexist with AS. The consumer must wait for S outside raw
and publication exclusion; native prepared completion can then release that
writer and the attempt rechecks current definitions. This is specific writer
coverage, not a complete DDL census.

The ordinary fixtures independently obtain stored carrier shape and flat
images through test-only native varlena oracles with a text-array signature.
They verify the real graph's class sources, NULL/nonempty carriers, distinct
heap/index/TOAST changes, reset, duplicate roots, unrelated-source filtering,
and copy accounting. Prepared option changes retain an earlier owned result,
then an independent observer verifies the real ungranted S wait, absence of
raw/publication locks and preservation of the caller's snapshot horizon. The
observer completes both normal outcomes before capture returns and its current
images are checked with first-unselected and established table read views.
Prior native relation modes must remain unchanged by completed capture;
an established SELECT's locks cannot be mistaken for leaked invocation counts.
No new stress, corruption, injection, interruption or recovery fixture is added.

The finite primary record is `logs/native-options-path-primary-v1`, outside
the Apache distribution. It binds six named files to the two official
archives, four complete selected writer/temp-predicate bodies and two complete
public class declarations. Facts SHA256:
`d195a6778de42d284c93be0d2d172e9e55804845f1d54df6289b0f7510199c39`.
Its 16-member nonself seal is
`4a07aa82dfd362fe024686f30de00c3d51b4a5ce129c6dbdeab575235bd780d7`.
The ordinary writer bodies differ by major; the temp predicate bodies agree.
This does not certify complete native files/call graphs, registry/provider
construction or callback/error behavior outside the selected interfaces.

Broader descriptor/options admission, writer coverage, transitive type and
statement closure, binding/planning/execution, serving, full performance and
release gates remain open. Issues 46/15 and the overall goal remain open.
