# Native metadata descriptor bootstrap

Status: paired source investigation and preferred design direction. The new
descriptor bootstrap gate remains OPEN before C changes. This document does
not certify an implementation-ready collector, a backend callback/owner
witness, new runtime behavior or an execution lease. The accepted six-catalog
and direct-payload components retain their original source and profile scope.
Concurrent native PostgreSQL two-phase transactions remain required.

The [transitive type investigation](native-transitive-type-closure.md) adds
`pg_range`, `pg_enum`, `pg_constraint` and constraint TOAST2832 to the intended
metadata graph. Each new native descriptor must have a pre-open admission
proof, including cold and invalidated paths. Opening it and checking afterward
cannot establish that proof.

## Guard alternatives and cost

The pinned native conflict matrices determine these differences. AS means
AccessShare, RX means RowExclusive and SUE means ShareUpdateExclusive. An exact
AS reference is still required when a native reader expects it; another mode
does not replace that reference.

| Candidate | Defining coverage still needed | Native concurrency consequence |
| --- | --- | --- |
| AS + RX, requiring NULL options | Does not exclude SUE option writers; rejected as a durable NULL-options proof | Reader attempts and ordinary RX catalog writers can coexist |
| AS + SUE, requiring NULL options | Must cover every relevant writer and the entered/rebuild paths | SUE conflicts with itself, so attempts on the same metadata relation serialize; maintenance also waits |
| AS + Share, requiring NULL options | Must cover every relevant writer and the entered/rebuild paths | Share permits other Share readers but conflicts with RX catalog writes and SUE maintenance |
| AS + RX, admitting complete supported options paths | Must prove the actual heap/builtin-btree path, carriers, registry/profile and entered/rebuild ownership | Avoids introducing either stronger mode's global coupling |

The last candidate is the preferred direction to investigate. This choice is
based on native concurrency and complete path admission, not a claim that RX
freezes options. No new mode is implemented or approximated by an old mode.
The [PG17 conflict matrix](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/storage/lmgr/lock.c)
and [PG18 conflict matrix](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/storage/lmgr/lock.c)
support the table. None of these modes freezes external provider libraries or
establishes the private callback profile by itself.

All physical waits, including waits for locks retained by a prepared transaction,
must occur before S. No catalog snapshot or reader descriptor increment may
cross a new physical wait. Stronger catalog locks would add real waits for
otherwise unrelated native catalog writes; disabling native two-phase support
is not an acceptable way to remove them. Any adopted guard set needs exact
ownership, ordering, complete-attempt release/retry and retention proof.

## Follow the actual options path

For ordinary catalog heaps/TOAST, `RelationParseRelOptions` reaches
`extractRelOptions`, which selects `heap_reloptions(..., false)` for the admitted
kinds. The path then uses the global registry through `default_reloptions` and
`build_reloptions`. Array deconstruction uses the builtin TEXTOID layout, rather
than consulting a selected type's I/O provider. Already initialized builtin
btree descriptors use `btoptions` and `build_reloptions` with the BTREE kind;
the native `bthandler` sets that options function. These are specific paths,
not permission to invoke an arbitrary access method's options callback.
See paired [PG17 array primitives](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/adt/arrayfuncs.c),
[PG18 array primitives](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/adt/arrayfuncs.c),
[PG17 btree options](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/nbtree/nbtutils.c)
and [PG18 btree options](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/nbtree/nbtutils.c).

A generic call-site inspection is insufficient: `allocateReloptStruct` and
`fillRelOptions` contain fill-callback calls even when validation is false.
The registration path decides reachability. In both pinned majors, global
`add_string_reloption` passes a NULL filler; local string registration accepts
a filler and belongs to a separate local registry. The native builtin global
string table is empty. The false-validation path does not invoke the registered
string validator; registration itself can validate a default and is not part
of descriptor parsing. Local registration/parsing cannot be silently treated
as the global heap/btree path. See paired
[PG17 registration and parsing](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/common/reloptions.c)
and [PG18 registration and parsing](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/common/reloptions.c).

This narrows the callback question; it does not accept every registry history,
provider, allocation or option datum. A concrete profile still needs an
authoritative registry/provider admission boundary, supported native carrier
paths and resource accounting before it calls native parsing. Preserve actual
NULL/empty/nonempty option images independently from parsed `rd_options`.
Native cache defaults or ignored options are not an alternative source of
the collector's defining bytes. Unexpected carriers or histories must fail
before opening the path that depends on them.

## Cold and invalidated descriptors

`RelationBuildDesc` loads class and attribute facts, selects table/index access
information, parses options and conditionally loads other defining state.
Fresh copied profile facts must establish the admitted native kind, namespace,
persistence, handler and physical identity, exact positive attribute layout,
default/missing/generated declarations and the flags that select additional
descriptor paths. Compiled names/types/length/alignment are expectations to
check against actual rows, not fabricated replacement catalog facts.

The tuple-descriptor implementation differs by major:18 populates compact
attributes and handles virtual generation and native NOT NULL state differently.
Catalog NOT NULL declarations must not be rejected merely because ordinary
table NOT NULL handling takes a different branch. A zero-check profile must
still establish actual default/missing/generation absence before skipping those
loaders. Additional rule/trigger or other descriptor paths need their own
admission; the bootstrap does not execute or interpret them.

Cache invalidation is an entry concern as well as a recheck concern. In17,
`RelationFlushRelation` routes active entries through rebuilding
`RelationClearRelation`;18 separates clear and rebuild functions. Active entries
can reach `RelationBuildDesc`, while inactive pre-existing entries can be
discarded. Locally created/relocated entries and nailed/index descriptors take
other paths. `RelationReloadIndexInfo` parses options again. A new guard acquired
later does not retroactively admit an earlier invalidation callback or an
unowned active reference. Warm-cache state and `criticalRelcachesBuilt` alone
cannot supply the missing witness. See paired
[PG17 relcache paths](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/relcache.c)
and [PG18 relcache paths](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/relcache.c).

The required entered-cache/owner witness and continuous builtin callback profile
remain OPEN. They must govern cache clearing, native snapshot acquisition,
new descriptor opens, invalidation/rebuilds and cleanup. An unsupported active
descriptor cannot be probed by opening/rebuilding it and checking afterward.
No assumption of a private owner or absence of outstanding references is
inferred from a passing direct-type fixture.

## Writer coverage and remaining sequence

The complete paired `AlterTableGetLockLevel` bodies distinguish rewriting,
default/missing/generation/layout changes from trigger changes and SUE
statistics/options/maintenance changes. Their option dispatch is the earlier
NULL-options counterexample. Separate trigger/index/maintenance entry points
are part of the source inventory; an inventory is not proof that every writer
has been challenged. Names, index membership, physical mapping and in-place
hints need their own defining-field classification. See paired
[PG17 ALTER lock selection](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/commands/tablecmds.c)
and [PG18 ALTER lock selection](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/commands/tablecmds.c).

`vac_update_relstats` is not only a tuple-count writer: it updates in-place
class state and can clear relation index/rule/trigger hints.18 also carries its
native all-frozen statistic. A concrete collector must distinguish those
actual hint updates from schema/provider changes, without inventing18 fields
on17 or using a hint as proof of complete live index membership. This source
read does not exclude all hint writers or certify a publication/cache shortcut.

Before C implementation, close the entered-cache/owner and supported options
profile proof; finish writer and defining-field coverage; then select the exact
bootstrap observations and guard sequence. Discovery must use only admitted
readers. Close the discovery horizon before acquisition, collect fresh profile
facts after acquisition, and admit each new descriptor before opening it.
An expanded graph must release the whole attempt before further waits. A/B/C
observations must remain independently coherent, preserve the entered data view
and command identity, and compare all defining carriers and graph identities.
The pure copying consumer runs once; S ends before callback-capable cleanup,
physical release/retention and any row/data/XID wait.

The transitive candidate still needs33 definition passes plus an unselected
bootstrap sequence and TOAST scans. There is no accepted total pass count,
peak memory, throughput or contention result. Complete ordinary paired
fixtures, strict product/probe packages, existing required suites and a fresh
implementation review are required after an adopted implementation. No new
stress, corruption, forced error, interruption, recovery or existing-profile
lifecycle experiment is authorized by this source investigation.

## Finite source evidence

The v1 primary record rehashes26 selected cached bodies and captures eight new
HTTP-200 trigger/index/vacuum/analyze bodies,17 per major. Under
`logs/native-catalog-bootstrap-primary-v1`, facts SHA256 is
`7a5dbd8882cb8c58e27da3b403ffc75a275373ad363d4010eac1953de32fd2b5`
and the47-member nonself seal is
`944c1b1126b165ea07edb61160c1542e83cf8d498a8c063a012fb7949751b3fa`.
V2 rehashes those34 bodies and four selected array/btree bodies, then captures
two HTTP-200 btree utility bodies:20 per major. Its facts/48-member seal are
`a13755f9ec985ccddcb06df32197d784e1e0dc8ef0ebd38858cacf90452e6bc4`
and `78b61856fca1b4cbeb7f9a8b771f666ed522d2898710eea46468b06891e87ea7`.
These records do not recursively recertify prior runtime review graphs.

Three distinct selected-function records preserve38 cold/options/writer bodies,
11 rebuild bodies and16 global/local/array/btree bodies. Five separately saved
paired differences retain native major variations. Source counts and identical
body comparisons are byte/provenance facts; they do not mean all40 source files
or65 selected bodies have complete semantic admission. The new descriptor,
registry/owner, writer and whole statement gates remain OPEN. No C, Rust, SQL,
native build or runtime result is introduced by this checkpoint. Standing
security/compiler/hosted-CI/publication exclusions remain in force.
