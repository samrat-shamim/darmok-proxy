# Private native statement interlock

Status: **C component under current verification; no application statement
entry point**. This extends the [publication fences](server-catalog-lease.md)
with an owned semantic reference. Full physical and name-candidate closure,
snapshot-neutral native preparation, callback admission, Describe/materialization
and rollback-capable execution remain [issue46](https://github.com/samrat-shamim/darmok-proxy/issues/46)
gates. Holding this reference cannot construct an admitted executable plan.

## Modes and publication order

The cluster-wide default-method object tag is database0 / pg_extension3079 /
object0 / subid17487 (0x444f). Share readers coexist; RowExclusive publishers
coexist; those two modes conflict. Compatible AccessShare is an independent
prepare-coverage witness. The existing raw observation and publication tags
remain separate and never transfer into native two-phase records.

At PRE_COMMIT, surviving native transactional SI/init-file messages take
semantic RX before publication RX, then briefly drain raw readers and advance
the epoch. Actual native lock cleanup releases both retained references after
invalidation publication. Supported late object-access publication follows the
same order. Neither path waits for semantic while retaining raw global.

Every native PRE_PREPARE takes coverage AS. A surviving metadata witness also
takes semantic RX. Native PostgreSQL serializes, transfers, recovers and
releases these modes. Both COMMIT PREPARED and ROLLBACK PREPARED retain only the
existing short raw completion wrapper: target invalidations/storage work and
native target lock release complete before the wrapper releases its owned raw
reference at utility return/error. The caller's later cleanup runs separately.
`max_prepared_transactions=0` is not required.

Native deferred triggers/portal work precede PRE_PREPARE. Later ON COMMIT temp
effects cannot successfully prepare, and native relation-map prepare rejects
pending changes. Arbitrary extension/C callbacks or direct private native
Prepare/Finish calls require their own admission proof; coverage AS does not
prove universal metadata RX coverage.

## Private C ownership

`statement_guard.h` defines a C token containing only a backend-local identity.
The module separately retains the exact ResourceOwner/subtransaction/reference;
it never borrows the token address or trusts it to select an owner. Identities
never wrap. Mismatched/duplicate release fails. No SQL function, SQL lease,
frontend handle or executable-plan constructor exposes this component.

Automatic tokens that span native error handling must be volatile:

```c
volatile DarmokStatementGuard guard = {0};
PG_TRY();
{
    if (darmok_statement_guard_acquire(&guard))
    {
        /* Only a separately admitted caller may perform guarded work. */
        perform_admitted_work();
    }
    else
    {
        /* Explicit lifecycle retry; release owned physical attempt refs
         * before any wait. This is not unguarded execution success. */
        release_physical_attempt();
        report_retry();
    }
}
PG_FINALLY();
{
    if (darmok_statement_guard_owned(&guard))
        darmok_statement_guard_release(&guard);
}
PG_END_TRY();
```

Product C callers must release within one invocation. Acquisition/release and
native coverage verification require TRANS_INPROGRESS and reject unsafe
discovery/writer/completion/preparation/exit/historic/parallel boundaries.
Lock operations temporarily select the recorded owner and restore
CurrentResourceOwner in PG_FINALLY. Subcommit records the parent owner before
native lock reassignment; mutating reentry during that cleanup is rejected.
Subabort clears only the child's reference. A parent reference survives child
rollback. PRE_COMMIT/PRE_PREPARE reject a retained reader.

## Shared drop and readiness

Database/tablespace DROP drains existing semantic readers with invocation RX
before announcing intent. It releases that exact RX reference before native
backend-retirement/storage-barrier waits. New S acquisitions immediately
postcheck intent: nonzero releases S and returns an explicit retry-needed
result, with no CV sleep or descriptor/output work. Discovery rejects unexpected
intent under an owned semantic reference. Successful shared-drop commit
unconditionally reacquires retained semantic RX before publication RX and
intent clear. Earlier irreversible native marker/storage effects still require
their actual physical owners; abort is not a promise to undo them.

Before first reader admission in each server incarnation, native coverage
verification takes TwoPhaseStateLock SH then every lock-hash partition SH and
checks each actual prepared dummy independently. A dummy with granted locks
must own coverage AS on the exact semantic tag. Inspection allocates nothing
and calls no catalog/receiver/user routine under these LW locks. Volatile
acquisition counts unwind in reverse before error or ready-state publication.
Uncovered targets fail loudly; native Finish remains available to drain them.
Partial new native transfers may conservatively refuse until they complete.

The pinned [PG17 startup](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/transam/xlog.c)
and [PG18 startup](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/transam/xlog.c)
complete RecoverPreparedTransactions before RECOVERY_STATE_DONE. Primary-only
admission therefore cannot inspect an incompletely replayed startup target.
This source ordering proof is not an executed recovery fixture. The private
verifier can be invoked independently without resetting/promoting shared
readiness; ordinary acquisition runs it only before the one-time ready flag.

## Field boundary and cost

Ordinary native procedure/operator/cast/type/domain/enum/collation/namespace
definitions and relation/attribute/index definitions use MVCC catalog tuple
writers and transactional SI. Defaults/constraints/rules/triggers/inheritance
also require their actual native relcache invalidation paths. A selected-OID
hook or unchanged row shape is not complete candidate coverage.

Native in-place statistics and conservative has-index/rule/trigger hints may
change while S is held. They are not the actual definition set. Physical
relfilenumber/tablespace/rewrite/index/mapped storage requires exact native
physical guards before S. External library/provider state and custom callbacks
need explicit admission. The finite core field/late-boundary matrix does not
freeze every PostgreSQL-visible field or replace those remaining gates.

Readers add one semantic acquisition/release plus their separate physical and
discovery costs. Publishers add retained RX. Each native prepare adds coverage
AS and metadata targets additionally add RX. Initial readiness scans configured
prepared slots/locks under TwoPhase plus all hash partitions once per
incarnation. No per-statement lock-table copy or allocation is added afterward.

Prepared metadata can delay new readers across all databases for its full
prepared lifetime. Streams/backpressure lengthen ordinary reader exclusion;
native queue ordering can delay newly arriving publishers behind readers.
Writer coexistence does not remove those costs. Per-database narrowing requires
a separate shared-dependency proof. No inherited throughput/overhead result
certifies this component.

## Verification scope

The separate test-only probe intentionally retains a C token across bounded
native SET/SHOW requests to observe queues and owner cleanup. This lifetime
exception never enters the product image or application protocol. It also
exercises one-invocation volatile-token PG_FINALLY cleanup on ordinary ERROR.
An explicitly ordered test callback can remove prepare coverage AS solely to
test uncovered dummy representation through the actual native verifier. That
synthetic case is not pre-install or recovery execution, and never claims the
once-ready fast path is valid under its intentionally broken hook premise.

Current PG17/18 evidence, full meaningful fixtures and independent code/evidence
review are required before component acceptance. No security work, stress,
forced interruption, native-profile restart/stop/signal, hosted CI or release
publication is included. Full execution and issue46 remain open.
