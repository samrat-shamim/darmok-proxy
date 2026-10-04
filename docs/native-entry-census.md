# Native entry mutation census

Status: paired archive-bound source census and targeted reference findings.
The startup/provider/registry/reference construction remains OPEN before new
descriptor admission. This extends the [startup trace](native-startup-references.md)
and [registry trace](native-reloptions-registry.md). Concurrent PostgreSQL native
two-phase transactions remain required.

## Corpus and what the search establishes

The official PostgreSQL archives bind these source identities:

| Tag | Observed tag commit | Archived regular files | C/header files under src and contrib |
| --- | --- | ---: | ---: |
| REL_17_11 | 083ac033419f690758508e08c1736089384bbee8 | 7,085 | 2,387 |
| REL_18_6 | 724edf9bde9d356724ad384a2e196edc3c9f80f7 | 7,284 | 2,463 |

Tag observations before and after each archive agree. Six selected earlier
source identities per major match the archive bytes. Every searched file is
listed and rehashed against its archive inventory before and after the census.
The raw ripgrep commands, version, streams and actual exits are retained.
See the official [PG17 source tree](https://github.com/postgres/postgres/tree/REL_17_11)
and [PG18 source tree](https://github.com/postgres/postgres/tree/REL_18_6).

These are matched source lines, including declarations, definitions and comments,
not typed call counts or runtime events:

| Literal query | PG17 | PG18 |
| --- | ---: | ---: |
| Selected global reloptions registration names | 35 | 35 |
| rd_refcnt | 19 | 20 |
| Selected relation-reference APIs | 25 | 25 |
| Selected transaction/subtransaction/resource callback registration names | 18 | 18 |
| RelationData | 16 | 16 |

For the selected global registration names, backend-file hits are confined to
reloptions.c. Other hits include public declarations and contrib/test module
sources. For the selected callback registration names, backend-file hits are
confined to xact.c and resowner.c; other source areas have registrations too.
This narrows the construction work without proving the actual loaded footprint.
Other-module and incidental names in the raw search are outside semantic
admission.

The corpus excludes generated/non-C source, external modules and compiled
objects. Literal absence cannot exclude token construction, indirect calls,
aliased writes or whole-object copies. This is not a complete program mutation
proof or a certificate for the selected server binary.
The [release/header/executable observations](native-build-observations.md) bind
the published release corpus and selected file artifacts. Matching source
inventories and one compiled builtin row do not close that binary or live-state
proof.

## Counts, owned items and whole-object movement

The selected native bodies distinguish these paths:

| Path | Reference fact | Required distinction |
| --- | --- | --- |
| RelationBuildDesc | Initializes a newly built non-nailed descriptor's count to zero | Construction does not itself establish an owned reader item |
| RelationBuildLocalRelation | Initializes count to one or zero according to nailit, then increments for the returned reader | Its initial pin and returned owned reference are separate |
| RelationIncrementReferenceCount | Enlarges the current owner, increments the count and remembers an item outside bootstrap processing | Bind the actual processing mode and owner |
| RelationDecrementReferenceCount | Decrements and forgets the current owner's item outside bootstrap processing | Equal counts do not establish the same owners |
| ResOwnerReleaseRelation | Decrements after the owner item has already been removed, then runs close cleanup | Do not forget the item twice or treat cleanup as a passive observation |

The [earlier startup constructors](native-startup-references.md) still supply
the intrinsic pins and restored-file count resets. The literal count matches
are in relcache.c, a table-command precondition and the public relation header.
They do not enumerate live references or provide a production owner census.

Rebuild uses whole RelationData copies and then re-swaps rd_refcnt to preserve
the old count. This does not transfer or re-enumerate ResourceOwner items.
PostgreSQL 17 places this path in RelationClearRelation; 18 splits it into
RelationRebuildRelation, while its clear path removes a reference-free entry.
The count, object address, defining fields and ownership therefore have separate
preservation obligations. No byte-copy or equal-count shortcut is selected.
See paired [PG17 relcache](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/relcache.c)
and [PG18 relcache](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/relcache.c).

Heap and index scan setup retain additional references through the same native
increment API and scan completion decrements them. PartitionDirectoryLookup
retains a reference when creating its entry; DestroyPartitionDirectory releases
those references. A table open alone is not the complete count model for these
carriers. Their provider, owner, callback and cleanup paths remain separate
admission obligations. See paired
[PG17 scan references](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/heap/heapam.c),
[PG18 scan references](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/heap/heapam.c),
[PG17 index references](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/index/indexam.c),
[PG18 index references](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/index/indexam.c),
[PG17 partition directory](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/partitioning/partdesc.c)
and [PG18 partition directory](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/partitioning/partdesc.c).

The RelationData query also finds the fake low-level storage carrier in
xlogutils.c. Its constructor zero-allocates the carrier and sets a physical
locator; its free routine releases the allocation. It is used for low-level
WAL/storage work, including syncing WAL-skipped files. It is not a catalog
descriptor/reference witness. Reading these source bodies introduces no
recovery, synchronization or interruption experiment.

## Registration and construction still required

The selected core transaction, subtransaction and resource callback lists have
static NULL initial declarations. Their registration functions allocate callback,
argument and next-pointer records in TopMemoryContext and prepend them; their
unregistration functions unlink the first matching callback/argument pair.
The lists' initial declarations do not establish their state after preload,
session startup or later commands. Darmok's own required transaction callbacks
remain part of the admitted footprint. See paired
[PG17 transaction registration](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/transam/xact.c),
[PG18 transaction registration](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/transam/xact.c),
[PG17 resource registration](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/resowner/resowner.c)
and [PG18 resource registration](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/resowner/resowner.c).

The next construction must cover generated/compiled mutation paths and actual
module initialization, bind the base's owned references and preserve it through
each permitted command, invalidation/rebuild and release phase. The entered
data view, exact AS references and physical-wait frontier remain independent
requirements. This census selects no private-layout reader and admits no new
descriptor. Registry/provider, writer, sequence and full table execution remain
OPEN.

The census and selected reads add no runtime cost. Native owner enlargement,
scan/read-stream allocation, callback storage and rebuild copies retain separate
resource costs. PG18's selected heap-scan bodies add bitmap-specific allocation
and batching paths and change buffer cleanup. Their presence is not measured
latency, peak-memory or universal scan-path admission. Setup and callback-capable
cleanup remain outside S.

## Finite evidence

Primary facts at `logs/native-entry-census-primary-v1/facts.json` have SHA256
`a76e2da7b03f8fd90d0ebda6435abcffadbad16d0dda81e08df526f2d5342edd`;
the 19-member nonself seal is
`a6cd6136b0050491e5430a579ceedac0f4b13321e2d04e992e01b90bb302960e`.
Census v1's actual exit 1 remains preserved: ripgrep's directory traversal order was
not the reader's flat lexicographic order. The file sets matched with no missing,
extra or duplicate path. Corrected v2 compares the exact membership independently
of order, rehashes every file before/after and actually exits 0. Its facts/seal
SHA256 values are `f9ae56f41c702f4d267c6c7c9f73cca0ce2c36d92a05f6ba5b30cc2beb47b09a`
and `d1f3f070a89c9508cf0646a954b3b02eddb583f4778a7fc4fb855f6508a35dac`.

Selected v1 retains 44 complete function bodies across 22 function pairs, the
separate PG18 rebuild body and six initial declarations. Four of 25 pairs differ;
21 have equal selected
bytes. Facts/seal SHA256 values are
`c8be611cb6ba4ae3aa6b326200c4f199d69eb5a43e68e4b39c00ed55ca7edcf9`
and `5f9f4dc02cf03e9b40c53852e053e83af9de4ed4167f2567ad26a1fde1cc779a`.
Counts and equality do not certify whole source files or every captured function.
Evidence remains outside the Apache distribution. No C/Rust/SQL, native build,
runtime/profile lifecycle, security or compiler work, new stress/recovery
experiment, hosted CI or release publication is introduced. All implementation,
serving, performance and release gates remain OPEN.
