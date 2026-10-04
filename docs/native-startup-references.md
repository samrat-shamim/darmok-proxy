# Native startup reference provenance

Status: paired PostgreSQL 17.11/18.6 source findings, interpreted under the
[supported stock runtime contract](native-runtime-contract.md). Ordinary native
startup/cache construction is trusted. Project reference lifetimes and supported
pre-open paths remain OPEN for new descriptors. This adds source context to the
[fixed-command proof](native-fixed-command-history.md), not a native executor.
Concurrent PostgreSQL two-phase transactions remain required.

## Baseline pins differ from owned reader references

The constructors establish specific initial counts:

| Path | Count and ownership observation | Consequence for catalog entry |
| --- | --- | --- |
| formrdesc | Sets rd_isnailed and rd_refcnt to one without remembering an owned reader item; starts with incomplete class facts | The intrinsic pin is not fresh catalog metadata |
| load_relcache_init_file | Restores one for nailed entries and zero for others | A serialized count is replaced, not transferred from the producing backend |
| load_critical_index | Builds the descriptor, sets its intrinsic pin, then releases its matching AccessShare acquisitions | The positive pin does not supply a retained AccessShare lock |

The shared and local cache phases can construct descriptors or restore init-file
entries. Phase3 temporarily increments each visited relation while repairing it,
then decrements it and may restart the scan after catalog access. Its critical
index flags are set before that repair loop finishes. Neither the flags nor a
nailed count alone establishes completed entry provenance.

The restored file contains parsed rd_options as well as descriptor, attribute
and index data. During reading, index/table handler setup can run before the
entire file and nailed-item counts have been checked and before cache insertion.
A later false return therefore cannot prove that the attempted load had no
provider effects. The producer writes a temporary file, receives invalidations
under RelCacheInitLock and conditionally renames it. That publication protocol
does not certify provider/registry provenance for the query owner. See paired
[PG17 cache construction](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/relcache.c)
and [PG18 cache construction](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/relcache.c).

## Completion is not a passive reference census

AtEOXact_RelationCache normally visits its end-of-transaction work list; an
overflow selects the full hash scan. AtEOXact_cleanup's expected zero-or-one
count check exists only with USE_ASSERT_CHECKING. It neither scans every entry
unconditionally nor supplies a production entry witness. The public
RelationHasReferenceCountZero macro tests zero directly; it does not subtract
the nailed pin or report owner provenance. See paired
[PG17 descriptor macros](https://github.com/postgres/postgres/blob/REL_17_11/src/include/utils/rel.h)
and [PG18 descriptor macros](https://github.com/postgres/postgres/blob/REL_18_6/src/include/utils/rel.h).

The selected ordinary startup calls establish cache tables, shared-cache
initialization, the startup transaction, local-cache initialization and final
commit in that order. PG17's selected transaction block explicitly obtains
GetTransactionSnapshot; PG18's does not. This says nothing about other startup
snapshot paths. AtStart_ResourceOwner constructs the transaction owner. Native
startup completion must still cover its callbacks and later session-module
loads; this selection does not admit the intervening startup functions. See
paired [PG17 startup](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/init/postinit.c)
and [PG18 startup](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/init/postinit.c).

When Phase3 needs new init files, InitCatalogCachePhase2 visits the declared
syscaches. CatalogCacheInitializeCache opens a relation, copies its tuple
descriptor into cache memory and closes the relation before setting up key
functions. InitCatCachePhase2 can separately open/close its index under the
underlying heap lock. Persistent copied tuple descriptors therefore must not be
counted as retained Relation reader items. Key-function/provider work remains a
separate obligation. See paired
[PG17 syscache preload](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/syscache.c),
[PG18 syscache preload](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/syscache.c),
[PG17 catcache setup](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/catcache.c)
and [PG18 catcache setup](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/catcache.c).

## Required construction and cost

The project must distinguish intrinsic pins from its own Relation increments,
copied TupleDesc storage and exact held locks. Under the selected contract,
native constructor/restored-file histories are runtime services. Darmok does
not need a producer integrity certificate, and must not use parsed options as
its defining catalog facts. Fresh actual carriers and supported dependent paths
remain necessary before a new open or rebuild. No post-open inspection or
readiness shortcut supplies project admission.
The [registry trace](native-reloptions-registry.md) adds native kind-mask,
referenced-data and major-specific presence obligations. Binding the consumer
registry does not retroactively certify an init file's producing backend; that
certification is no longer a functional entry requirement.
The [archive-bound mutation census](native-entry-census.md) adds explicit count
initialization, owned scan/directory references and whole-object rebuild copies.
Its literal source matches are not an actual owner census or complete program
mutation proof.

Core startup includes file reads, descriptor allocations, provider setup and
physical waits. Phase3's restart scan can be quadratic; needing new init files
also preloads syscaches. These costs differ by cache state and belong outside S.
This source work adds no runtime operation or measured performance acceptance.
No profile files were deleted or profiles restarted to exercise either path.

## Finite evidence

Primary capture `logs/native-startup-references-primary-v1` binds twelve source
files across both majors: six rehashed bodies and six new HTTP-200 bodies. The
fresh relcache bytes match their earlier named hashes. Selected v3 preserves
34 complete function bodies, two macros, six bounded startup spans and four
startup calls. Four of23 selected pairs differ: the cleanup clear signature,
startup snapshot call, compact-attribute construction and the formrdesc frozen
statistic/assignment placement. Equal bytes on19 pairs do not admit whole files
or every captured function. Primary and selected facts SHA256 values are:

```text
861cdcba973269013ffb1b7b7e3c4d04880f299d78e403d5aaf8e22b408071c7
685e5d212ac78710cc0c1ebe6e4a70ae5946063cf0964556e6feba9f9e1bca86
```

The selected65-member nonself seal is
`cd619659b504da46c6557854a8f0cf65aa7ac8e6385364943c77c05389068438`.
Root04/05 extraction failures remain preserved: an early startup exit repeats
the commit comment, and the PG17 snapshot marker is absent on18. Root06 uses
the exact bounded transaction block and actually exits0. These are source-reader
corrections, not native product or runtime failures. Evidence remains outside
the Apache distribution. Startup/provider/registry/reference/writer/sequence,
new descriptor/table execution, serving, performance and release gates stay OPEN.
