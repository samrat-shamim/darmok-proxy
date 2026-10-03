# Private native relation references

Status: implementation draft; compilation and PG17/18 verification pending.
The conditional design review accepts the finite reference-owner boundary,
not complete physical dependency closure or application statement admission.
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
coverage or complete physical protection. Direct LockAcquire does not dispatch
invalidations, open descriptors or MarkLockClear. Future preparation must
absorb invalidations and clear the exact local references outside semantic/raw
Share, or prove later preparation cannot dispatch again. Calling
AcceptInvalidationMessages alone does not mark those references clear.

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

Native LockAcquire ERROR is abort-required, not an attempt-retry result.
WaitOnLock can leave an awaited lock and saved owner for native LockErrorCleanup,
including a grant racing with ERROR. The catch detaches the token/list, records
the failing owning subtransaction, frees private state and rethrows. It does
not decrement a guessed partial list. New private operations and discovery,
commit and prepare are refused until matching native child abort or top abort.
A failed child's veto occurs at PRE_COMMIT_SUB while state is INPROGRESS;
successful promotion occurs at COMMIT_SUB before native reassignment.

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
and acquires/releases n native increments. It allocates validation scratch and
one owned list; native owner arrays, fast-path/main-table transitions, partition
contention and waits remain. Retain frees the list but repeated retained scopes
may add native counts until transaction cleanup. No throughput or allocation
claim follows from the design.

The separate test-only probe may retain a token across bounded SET/SHOW requests
to observe native queues/cleanup. Product artifacts contain no probe. Planned
ordinary fixtures cover exact shared/local tags and modes, borrowed same-owner
and parent references, copied input, stale tokens, parent/child promotion and
rollback, scoped ERROR cleanup and native retained commit/rollback/prepare.
Prepared DDL completion must unblock ordered physical acquisition for both
outcomes while observers see neither semantic nor raw/publication references.
Scoped caller ERROR after completed acquisition does not certify the pending
native-wait ERROR branch. No forced-error experiment is claimed or required for
its pinned native-source cleanup rule. Runtime evidence remains pending.

Full transitive/catalog/application/index/TOAST/storage/name/negative/overload
closure, neutral preparation and exact cache clearing, callback/provider
admission, Describe/materialization, immutable execution and MySQL data/lock
semantics remain open. This component alone cannot admit a SQL statement.
