# Catalog reader reference scopes

Status: source design for the existing finite builtin catalog profile;
implementation drafted; current runtime verification pending. This is the local
reference and acquisition argument under the
[stock runtime contract](native-runtime-contract.md). It admits no new catalog
descriptor, options parser, arbitrary callback configuration or table executor.
The NULL-options generalization and transitive collector remain open.

## Acquire before opening readers

The existing collector opens its initial catalog heaps with AccessShare one at
a time. Each completed open leaves a Darmok-owned Relation increment alive while
the next open can acquire a new physical tag. The final physical graph is already
acquired after the initial readers close; the initial preparation needs the same
separation of explicit physical acquisition from reader ownership.

Use a transient catalog seed attempt before opening the initial readers. Its
compiled heap OIDs receive exact AccessShare references, together with the
existing critical class-OID and attribute-ordinal indexes. The six-heap storage
collector needs eight references; the four-heap SHOW reader needs six. Each list
contains unique OIDs. The existing native reference primitive acquires the whole
list before returning, records its real transaction owner and exact grants, and
does not create a frontend token.

Only after successful acquisition may the reader use table_open(..., NoLock).
NoLock skips relation_open's explicit LockRelationOid call. It still obtains a
Relation reference and can build or revalidate a descriptor. The supported
builtin descriptor paths and admitted callback configuration remain prerequisites
before that call; a later pointer, kind or layout postcheck cannot supply them.
The two critical indexes are existing supported native services, not newly
admitted arbitrary index descriptors. Additional catalog roots still need their
own cold/rebuild path and dependency argument.

The seed covers explicit project opens and the existing class/attribute critical
lookup paths. It does not enumerate all native cache or owner entries, guarantee
all kernel operations are nonblocking, or certify a whole callback call graph.
Correct supported native services and the finite private history are explicit
dependencies. Native SI and descriptor work remain outside semantic Share.

## Reader and physical lifetimes

For the storage collector, the initial sequence is:

1. Observe the native publication/private stamp without retaining its short
   reader references.
2. Acquire all eight seed AccessShare references with no Darmok catalog reader,
   scan or registered snapshot alive.
3. Prepare the six readers using NoLock; capture initial fixed facts and carriers.
4. End all scans, unregister the catalog snapshot, invalidate the ephemeral
   catalog snapshot and close all six reader increments using NoLock.
5. Release every seed reference before acquiring the copied complete physical
   graph. The seed is never retained as earlier transaction history.
6. Prepare source and final readers using the complete graph's exact AccessShare
   references. Source fetches keep their existing closed-before-later-wait
   boundary; final descriptor preparation still precedes semantic Share.

The historical SHOW reader has no complete application physical graph or
execution lease. It first observes its stamp with no seed/readers alive, ends
that reader scope, then acquires its six seed references. Preparation occurs
outside reader bookkeeping under the existing native refresh exclusion. Its
second raw acquisition must use the no-CV try operation. Shared-drop intent or
a generation change closes the whole attempt before the next initial lifecycle
wait. On success, it ends raw bookkeeping, closes readers/snapshot and releases
the seed before selecting copied types or building protocol output. Its original
sixteen-attempt generation bound remains; the initial lifecycle wait remains
outside every project reader and seed scope.

Installation verification can own a syscache tuple while checking its version.
Copy the fixed namespace OID and release that tuple before a separate namespace
lookup. The native syscache and value-copy services retain their supported API
contracts; this ordering avoids carrying that project pin into the next lookup.

These counts are separate from intrinsic nailed pins and native internal
references. NoLock does not supply a lock, and a Relation increment does not
substitute for an exact AccessShare count. The seed and complete-graph attempts
own their native counts through CurTransactionResourceOwner. Returned readers
and scans use CurrentResourceOwner. The admitted fixed history preserves both;
native owner bookkeeping and phased cleanup remain trusted services.

| Darmok acquisition | Matching normal completion |
| --- | --- |
| One returned table/relation open | One close with NoLock, which drops the reader increment only |
| One returned heap scan | One heap_endscan, including its additional Relation increment |
| One registered catalog snapshot | One UnregisterSnapshot on the same admitted owner |
| One successful seed attempt | Exact native reference release after all seed readers close |
| One complete physical attempt | Existing exact release or explicit native retention after metadata ends |

Detach each returned scan, snapshot and Relation pointer from the project's
cleanup ledger before calling native cleanup. A cleanup ERROR must not cause a
second decrement of an operation whose effect is uncertain. Any unreturned
native acquisition, or resource left by an interrupted cleanup, belongs to
native abort handling. It is not an invented successful local release.

Entered invocation failure records the captured owning subtransaction as
abort-required. Partial physical grants retain the reference primitive's native
abort disposition. Matching native abort is required before reuse, commit or
prepare. Nested FINALLY must preserve this disposition even if cleanup itself
fails. Invocation storage lives under the native current transaction context;
normal completion deletes it, while failure leaves it alive until the owning
native transaction/subtransaction releases resources and reclaims that storage.
It cannot be deleted early while an uncertain native resource may still point
into it. No ERROR becomes a completed false retry.

The SHOW entry requires an unused physical/semantic invocation boundary. It is
not callable through another live statement guard. Ordinary fixtures perform
nonempty catalog discovery before acquiring their Share guard and still check
unchanged first-data-snapshot state, queued publishers and prepared completion.

## Concurrency and costs

Seed acquisition holds no semantic Share, raw observation fence, reader
descriptor or catalog snapshot. A prepared native DDL transaction can therefore
finish through the existing completion route while the seed waits for a native
physical tag. Ordinary native deadlock and ERROR behavior remains intact.
Readers use AccessShare, which adds no Share/SUE serialization against ordinary
RowExclusive catalog writes. Concurrent native two-phase transactions remain
required. The seed supplies neither durable NULL options nor writer coverage;
that options/path work remains separate.

The storage collector replaces eighteen outer catalog LockRelationOid calls
across its three six-heap preparations with eight seed acquisitions. Its copied
complete physical graph is unchanged. The SHOW reader replaces four outer
catalog lock acquisitions with six seed acquisitions. Native nested lookups,
SI processing and owner work keep their own costs; these counts are source-level
outer operations, not measured latency. The mechanism adds a bounded temporary
request array and native attempt state outside raw/Share, and changes no protocol
round trips. Current callers use six or eight requests; the existing 4096 native
reference limit remains. No peak-memory, throughput or contention result is
claimed before current verification.

## Selected native source

The source record at logs/native-bootstrap-references-primary-v1 binds ten
named files directly to two previously captured official PostgreSQL source
archives, twelve complete selected functions per major and twelve paired
differences. Nine pairs have equal bytes; heap scan creation/end and relation
cache lookup differ. These are selected interface/resource findings, not
semantic acceptance of every function in the ten full files.

Facts SHA256: b926ccae45afa73284b6594ab7fb86bd0cbc8bddd63750eb93c7651c8affb544.
The 51-member nonself seal is
a4293ce10797821cee887ed5864232a028ddc67ad8dd82f7ea35432a381d62f9.

See paired [PG17 relation open/close](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/common/relation.c),
[PG18 relation open/close](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/common/relation.c),
[PG17 table wrappers](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/table/table.c),
[PG18 table wrappers](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/table/table.c),
[PG17 scan ownership](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/heap/heapam.c),
[PG18 scan ownership](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/heap/heapam.c),
[PG17 relation references](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/relcache.c),
[PG18 relation references](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/relcache.c),
[PG17 snapshot wrappers](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/time/snapmgr.c)
and [PG18 snapshot wrappers](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/time/snapmgr.c).

Current packages, affected required ordinary suites and independent fresh review
are still required after implementation. Full supported-path, writer, immutable
binding/planning, execution, serving, performance and release gates, issues 46/15
and the goal remain open. Standing exclusions remain in force.
