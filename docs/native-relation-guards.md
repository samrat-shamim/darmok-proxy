# Private native relation references

Status: the earlier finite private reference-owner was verified on PostgreSQL
17.11/18.6 at `c0438c7882a197227c147fe4b86e2c67bddbdd5d`.
The native cache-clearing change described below is under verification.
Complete physical dependency closure and application statement admission remain
unaccepted; the reference primitive alone does not establish either.
Issues46/15 remain open. Concurrent native PostgreSQL two-phase transactions
are supported; `max_prepared_transactions=0` is not a requirement.

## Boundary

`postgres/darmok_server/relation_guard.h` defines a private C attempt containing
only a backend-local identity. The module copies an ordered list of relation
OID/mode declarations and records exactly `CurTransactionResourceOwner` and
its subtransaction. The caller must release the attempt within one invocation,
or explicitly retain successful references for native transaction cleanup.
Automatic tokens spanning PG_TRY must be volatile. There is no SQL registration,
frontend handle, plan constructor or executable statement entry point.

The supported modes are AccessShare, RowShare and RowExclusive. One relation
may have multiple modes, but exact OID/mode duplicates, zero OIDs, unsupported
modes, empty requests and more than 4096 declarations fail before acquisition.
Sorted scratch keys provide O(n log n) duplicate validation without changing
the actual acquisition order. A future closure builder must declare every
needed reference and prove native heap-before-index ordering independently.

Native relation tags use database zero for `IsSharedRelation(oid)` and
MyDatabaseId otherwise. This uses native hard-coded OID classification without
a catalog lookup. An OID lock does not prove existence, stable names, candidate
coverage or complete physical protection. Acquisition now delegates each
declaration to native `LockRelationOid` under the recorded transaction owner.
Unless the exact native local mode was already clear, core absorbs shared
invalidations and calls `MarkLockClear` on its own LOCALLOCK before returning.
This occurs outside semantic/raw/publication exclusion. Darmok stores no native
local-lock pointers and does not duplicate core's recursive cache processing.

An already-clear mode may skip dispatch. This marker is per exact local mode;
it does not establish that all backend SI messages or current catalog facts
are globally caught up. Full preparation still needs refreshed catalog facts,
complete physical/name/candidate closure and definition recheck. A compatible
metadata publication, another mode or unrelated provider is not covered merely
because one native mode is clear.

The acquisition/cache-dispatch phase rejects mutating module reentry through
the common ready precondition, native prepare/completion and child precommit.
Observation of token ownership is harmless. Ordinary native recursion remains
native; this phase is not arbitrary callback/provider admission. SI processing
can rebuild descriptors and call registered callbacks. Snapshot neutrality is
conditional on the continuous builtin backend profile documented in
[catalog-discovery.md](catalog-discovery.md#continuous-backend-profile).
No escaped arbitrary descriptors, custom AMs or extension callbacks are admitted
by this reference primitive, and no final flag comparison supplies that proof.

## Ownership and errors

One attempt may be active per backend. Identities are monotonic through the
signed-int8 range and never reset or wrap. The copied list lives in
TopTransactionContext so a parent's ownership survives child rollback.
Every successful native acquire is an increment, including an already-held
reference. Exact release selects the recorded owner in PG_FINALLY, decrements
in reverse acquisition order and preserves unrelated owner/session references.
Retain clears the module/token and frees the wrapper; the actual increments
remain native transaction-owned until commit, rollback, subabort or prepare.
Native mixed session/transaction restrictions at PREPARE still apply and must
propagate rather than becoming guaranteed prepare success.

Release/retain require a completed owned token at a safe TRANS_INPROGRESS
boundary, after semantic/raw references have been released. Detach precedes
any decrement, so enclosing cleanup cannot release the same counts again.
An unexpected release failure marks the backend broken; native abort remains
responsible for remaining references. Mutating reentry during owner cleanup
is rejected. Successful subcommit promotes the recorded owner before native
lock reassignment. Subabort discards only matching logical state, and callbacks
never acquire/release native references. Commit/prepare reject an active attempt.

Native acquisition/cache-refresh ERROR is abort-required, not an attempt-retry result.
WaitOnLock can leave an awaited lock and saved owner for native LockErrorCleanup,
including a grant racing with ERROR. A successful grant can also precede an
invalidation callback ERROR, before the private completed count advances.
The catch clears the phase, detaches the token/list, records
the failing owning subtransaction, frees private state and rethrows. It does
not decrement a guessed partial list. New private operations and discovery,
commit and prepare are refused until matching native child abort or top abort.
A failed child's veto occurs at PRE_COMMIT_SUB while state is INPROGRESS;
successful promotion occurs at COMMIT_SUB before native reassignment.
If a later semantic acquisition throws, completed physical references may
unwind by exact release of their distinct relation tags, but that ERROR must
still propagate to native abort. Releasing physical references does not clean
the awaited semantic lock or authorize catching the error and continuing.

All physical waits happen before semantic Share. Semantic/raw publication
exclusion must end before tuple, MultiXact or XID waits: an ordinary transaction
can own a row XID, perform unrelated DDL and need semantic RX at precommit.
Retaining reader S while waiting for that row creates the opposite edge.
This is source reasoning, not an executed hanging or interruption experiment.
Physical references plus independently proven immutable bindings/executor state
must bridge preparation to execution; unchecked re-analysis after releasing S
does not establish validity.

## Cost and verification scope

The wrapper copies O(n) declarations, validates duplicate keys in O(n log n),
and acquires/releases n native increments. Each uncleared exact native mode may
dispatch SI; already-clear modes skip that work. Cache rebuilding, callback
work and catalog waits occur before module exclusion. It allocates validation scratch and
one owned list; native owner arrays, fast-path/main-table transitions, partition
contention and waits remain. Retain frees the list but repeated retained scopes
may add native counts until transaction cleanup. No throughput or allocation
claim follows from the design.

The earlier separate test-only probe may retain a token across bounded SET/SHOW requests
to observe native queues/cleanup. Product artifacts contain no probe. Four
ordinary fixtures cover exact shared/local tags and modes, borrowed same-owner
and parent references, copied input, stale tokens, parent/child promotion and
rollback, scoped ERROR cleanup and native retained commit/rollback/prepare.
Prepared DDL completion must unblock ordered physical acquisition for both
outcomes while observers see neither semantic nor raw/publication references.
Scoped caller ERROR after completed acquisition does not certify the pending
native-wait ERROR branch. No forced-error experiment is claimed or required for
its pinned native-source cleanup rule. Native session/transaction overlap
restrictions at PREPARE remain native errors, not an all-configurations success
claim. The four fixtures passed on both majors, and all 135 package tests plus
three private-owner discovery tests passed on each. Current images bind all
16 native inputs; eight profiles are healthy with no prepared/coordination/mock
relation references after the fixtures. Missing required dependencies fail.

The earlier bounded cost check sends 32 warmup and 128 measured scopes, with three
references per scope through one in-container TCP-loopback psql connection.
PG17 p50/p95/max is 15000/37000/46000 ns; PG18 is 10000/23000/90000 ns, with
1000 ns psql display quantization. Each scope uses a volatile automatic token
and releases its native references. Final status has no owned token and false
snapshot flags; it is not a status observation for every sample. Protocol,
utility, wrapper and native-owner allocation costs are included. There is no
baseline comparison, isolated C-call, throughput, contention, descriptor,
preparation, executor, recovery or full-cache acceptance. See the exact command,
environment and immutable evidence checkpoint in [release-plan.md](release-plan.md).

Two additional ordinary fixtures are under verification. An exact-owned native
sampler observes ALREADY_HELD/CLEAR by acquiring and releasing one extra native
increment, without dispatching or marking. It checks multiple modes, parent
counts surviving child rollback and clear reset after the final count. A warm
builtin heap descriptor is closed with AccessShareLock before prepared AX;
prepared commit must consume target invalidations and refresh current shape;
prepared abort does not publish that private definition and may consume no
target messages. Both outcomes must return the expected current shape outside exclusion
with first-unselected and established repeatable-read flags unchanged. A passive
test callback records owner/fence observations during acquisition. OIDs/setup
come from a distinct observer; no reader SELECT selects the unselected view.
These fixtures do not inject native wait errors or certify arbitrary callbacks.
Earlier results above do not certify these changed native inputs.

Full transitive/catalog/application/index/TOAST/storage/name/negative/overload
closure, globally fresh facts, neutral descriptor preparation, callback/provider
admission, Describe/materialization, immutable execution and MySQL data/lock
semantics remain open. This component alone cannot admit a SQL statement.
