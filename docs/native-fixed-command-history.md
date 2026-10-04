# Fixed native commands and catalog entry

Status: paired PostgreSQL 17.11/18.6 source findings and project transition
obligations under the [stock runtime contract](native-runtime-contract.md).
Native services are trusted; local lifetimes and relevant functional callback
admission remain OPEN for new paths. This admits no new C reader or execution
lease. Concurrent native PostgreSQL two-phase transactions remain required.

## The commands are fixed; their native effects still need a proof

The private query owner establishes a fresh connection and submits
`ROLLBACK; SET search_path = pg_catalog`. Its subsequent controls are explicit
BEGIN characteristics, COMMIT, ROLLBACK and generated savepoint controls without
AND CHAIN. Catalog discovery submits one escaped literal SET LOCAL request and
SHOW for `darmok_server.catalog_request_v1`. It exposes no arbitrary SQL, client
adoption or setup execution. The [separate setup owner](native-command-history.md)
prevents initialization DO manifests from entering this query history.

The native simple-query path calls `pg_analyze_and_rewrite_fixedparams` and
`parse_analyze_fixedparams`. The latter invokes `post_parse_analyze_hook` when
present. For a query that remains CMD_UTILITY, `pg_rewrite_query` skips ordinary
query rewriting and `pg_plan_queries` constructs a utility PlannedStmt without
calling `pg_plan_query`. That distinction does not bypass the analysis hook or
prove that its provider leaves the query or native state unchanged. See paired
[PG17 analysis](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/parser/analyze.c),
[PG18 analysis](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/parser/analyze.c),
[PG17 rewrite/planning dispatch](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/tcop/postgres.c)
and [PG18 rewrite/planning dispatch](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/tcop/postgres.c).

`PlannedStmtRequiresSnapshot` returns false for TransactionStmt, VariableSetStmt
and VariableShowStmt. `PortalRunUtility` therefore does not obtain its ordinary
transaction snapshot for these statement kinds. It still calls ProcessUtility,
which dispatches through `ProcessUtility_hook` when present. A hook or its
delegates can perform additional work; the statement kind alone cannot certify
that FirstSnapshotSet or the entered data view stayed unchanged. See paired
[PG17 portal dispatch](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/tcop/pquery.c),
[PG18 portal dispatch](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/tcop/pquery.c),
[PG17 utility dispatch](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/tcop/utility.c)
and [PG18 utility dispatch](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/tcop/utility.c).

The standard transaction branch routes BEGIN options through SetPGVariable,
COMMIT through EndTransactionBlock, ROLLBACK through UserAbortTransactionBlock,
and savepoint controls through DefineSavepoint, ReleaseSavepoint or
RollbackToSavepoint. The block-control functions arrange native state transitions;
the command-completion machinery performs the later subtransaction/transaction
work. The captured transaction dispatch also contains native prepared-transaction
and chained variants. Capturing that span does not add them to this owner's API.

## Configuration callbacks are part of the history

The selected builtin registration rows bind `search_path` to check_search_path
and assign_search_path. Its check validates an identifier list, optionally using
the in-memory search-path cache; it no longer checks schema existence there.
Its assignment marks the base path invalid for lazy recomputation. These selected
paths do not themselves prove that later namespace recomputation is admitted.
The three current transaction-characteristic rows bind their respective checks
and have NULL assignment/show hooks. Their checks inspect transaction,
subtransaction, first-snapshot or recovery state as applicable. They are not
substitutes for an admitted BEGIN boundary. See paired
[PG17 namespace checks](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/catalog/namespace.c),
[PG18 namespace checks](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/catalog/namespace.c),
[PG17 transaction checks](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/commands/variable.c)
and [PG18 transaction checks](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/commands/variable.c).

There are additional call sites beyond those builtin hooks:

| Native path | Observed dispatch | Entry-proof obligation |
| --- | --- | --- |
| ExecSetVariableStmt | Invokes the string object post-alter hook after its setting operation, including the owner's search-path SET | Bind `object_access_hook_str` and its actual delegate chain; a pure variable assignment is insufficient |
| set_config_option / set_config_option_ext | Delegate to set_config_with_handle, whose typed assignment branches invoke assignment hooks when installing an accepted value | Bind the actual selected GUC record and reachable hook implementation, rather than its name alone |
| AtEOXact_GUC | Restores stacked values and invokes assignment hooks when required | Include local-value restoration, subtransaction completion and abort cleanup in the reference/provider history |
| Generic SHOW | GetConfigOptionByName reaches ShowGUCOption, whose typed branches can invoke show hooks | Bind the actual record, hook and descriptor/output path; SHOW syntax alone is not a passive observation proof |

The string object hook is a distinct slot from the OID object hook installed by
the current Darmok module. Its macro checks the string slot before dispatching
RunObjectPostAlterHookStr. These are callback-reachability findings only. See
paired [PG17 SET/SHOW](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/misc/guc_funcs.c),
[PG18 SET/SHOW](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/misc/guc_funcs.c),
[PG17 GUC cleanup](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/misc/guc.c),
[PG18 GUC cleanup](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/misc/guc.c),
[PG17 string-hook macro](https://github.com/postgres/postgres/blob/REL_17_11/src/include/catalog/objectaccess.h)
and [PG18 string-hook macro](https://github.com/postgres/postgres/blob/REL_18_6/src/include/catalog/objectaccess.h).

The existing module is an essential provider, not an absent-hook assumption.
Its `_PG_init` installs its ProcessUtility hook and transaction/subtransaction
callbacks. It intercepts the exact catalog SHOW before its saved utility delegate
or standard_ProcessUtility and calls darmok_catalog_show. Its catalog request
registration has NULL check, assignment and show hooks. The specialized SHOW's
descriptor/output work runs after its catalog fence release. This describes
the current source routing, not complete admission of that reader or every
callback in the module. A profile that requires all hooks to be NULL would
reject the mechanism it is supposed to establish.

## Cleanup retains separate reference and lock obligations

The native relation reference increment both increases rd_refcnt and remembers
the reference in CurrentResourceOwner during normal processing. Explicit
decrement forgets that owner item. Bulk release has already removed the item
before ResOwnerReleaseRelation decrements the counter and runs close cleanup.
Descriptor references and locks therefore have different release mechanisms.

ResourceOwnerReleaseInternal first recurses into descendants. In each release
phase it temporarily sets CurrentResourceOwner to the owner being released,
performs the native phase work and invokes the registered ResourceRelease_callbacks.
On normal return it restores the previous owner. Non-top-level successful lock cleanup
reassigns locks to the parent; it does not transfer relation references in place
of their BEFORE_LOCKS release. Callback identity, invocation owner and phase must
all be part of the proof. An equal owner pointer observed later cannot establish
the callback history. See paired
[PG17 reference lifecycle](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/relcache.c),
[PG18 reference lifecycle](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/relcache.c),
[PG17 resource release](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/resowner/resowner.c)
and [PG18 resource release](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/resowner/resowner.c).

CallXactCallbacks and CallSubXactCallbacks dispatch registered callback/argument
pairs and permit self-unregistration by saving the next item before invocation.
StartSubTransaction calls start callbacks after initializing its resource owner.
CommitSubTransaction invokes pre-commit and commit callbacks, releases native
resources in phases, restores GUC state and later deletes its owner. Abort and
cleanup have distinct paths. A savepoint completion therefore needs its own
transition proof, not a top-level ROLLBACK analogy. The same applies to retained
parent locks after successful RELEASE. See paired
[PG17 transaction lifecycle](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/transam/xact.c)
and [PG18 transaction lifecycle](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/transam/xact.c).

## Project transitions required before new native descriptor opens

The intended project argument uses this private history and the selected native
services, with three obligations. None is implemented by this source checkpoint.

1. Establish fresh exclusive query ownership and confirmed native state after
   supported native startup. State the admitted extension configuration and
   relevant analysis/utility/GUC paths. A fresh Rust connection or lookup SET
   alone does not establish their neutrality or project reference lifetimes.
2. Prove every allowed transition preserves that base or changes explicitly
   modeled state. Include parse/utility/object hooks, catalog discovery, SI/cache
   callbacks, all release phases, GUC restoration and subtransaction owner
   changes. Bind actual native reference increments/decrements separately from
   lock acquisition, reassignment and retention. Unsupported project transitions
   cannot supply admission merely because their wire outcome matches. Native
   internal reference accounting is an API dependency, not an all-owner census.
3. At each new descriptor entry, require the established history and exact
   pre-open profile/reference facts before the reader increment or rebuild.
   Perform physical waits before S with readers/snapshots closed. End S before
   callback-capable cleanup. If the graph expands, release the complete attempt
   before another acquisition; concurrent prepared transactions remain supported.

The public APIs identified in the [bootstrap investigation](native-catalog-bootstrap.md)
do not passively enumerate all native cache references or opaque owner items.
The selected architecture does not require that census. A module-path list,
hook pointer, critical-cache flag or readiness receipt still cannot replace
project lifetime and supported-path arguments. The
[startup trace](native-startup-references.md) explains native intrinsic pins and
restoration separately from Darmok-owned reader increments. Functional options/
provider admission and defining-writer coverage must close before transitive
descriptors are opened. No fallback collector or stronger-lock shortcut is
admitted by these findings.

## Major differences, cost and evidence scope

The selected PG18 search-path row adds GUC_REPORT. A changed value can therefore
be queued for ParameterStatus reporting; identical command tags are not proof of
an identical wire transcript. PG18 also adds AIO cleanup to resource release and
subtransaction abort, type-cache cleanup to subtransaction completion, parallel
worker restoration exceptions in two transaction checks, and a different debug
rewrite-test boundary. The string object-hook call changes only its explicit
void-pointer cast. These differences are retained rather than flattened into
one major's model. See paired
[PG17 registration rows](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/misc/guc_tables.c)
and [PG18 registration rows](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/misc/guc_tables.c).

The findings add no runtime SQL, protocol round trip, allocation, cache or lock
implementation. The existing native dispatch walks callback lists, GUC stacks
and resource-owner descendants; their sizes and provider work belong in any
future resource bound. Passive module capture and reference-history construction
keep their separate costs. No measured latency, peak memory or contention
acceptance follows from a source read.

The primary v2 record binds 28 source files across the two pinned majors. It
rehashes 20 selected bodies and records eight new actual HTTP-200 bodies. The
selected-functions v2 record retains 92 complete function bodies, eight complete
GUC rows, two complete macros, 12 bounded callback call sites and six bounded
utility spans. Eight of the 60 paired selections differ; their exact differences
are saved. The other 52 pairs have equal selected bytes. These counts do not
certify complete semantics of all files, functions or unrelated variants.

Evidence is outside the Apache source distribution under
`logs/native-fixed-history-primary-v2` and `logs/native-fixed-history-functions-v2`.
Their facts SHA256 values are respectively:

```text
8948e9836c6fb9aaa7059a6704cbd66ffe893ea4287f7a04d1aa0126ae2f7990
4f7d0c9a48a9f67f757568fce117cf996ff4b9dbc9d7845c9a251e6f1fd16dd1
```

The selected record's 159-member nonself seal is
`f9a976c28d5c876f32009ad69703ae5ff11ddb105a4c253996ae21137fbcedc9`.
Reader v1's actual exit 1 is preserved: it looked for assignment calls in the
delegating set_config_option_ext wrapper. V2 follows set_config_with_handle and
actually exits 0. This is an extraction correction, not a native product failure.
No native code, build, runtime, profile lifecycle, stress or recovery experiment
is introduced. Entry/bootstrap, complete table execution, serving, performance
and release gates remain OPEN.
