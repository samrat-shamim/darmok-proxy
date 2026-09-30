# Statement execution and rollback contract

Status: **statement execution design; not an implemented row executor**. The
native value and parameter codecs, [prepared representation checks](native-statements.md),
and [exclusive control owner and borrowed scopes](native-backend.md) are
implemented components. Admitted statement execution, rollback through output
validation and a runnable proxy remain pending.

## Why the executor needs its own boundary

Preparing a statement describes types, not every possible value. A native
NUMERIC result can be outside the decoder's range; an otherwise valid value can
fail encoding against incorrect frontend metadata. If INSERT RETURNING commits
before those checks, returning an error to the client leaves a successful write
behind. The transaction must therefore remain rollback-capable through both
native decoding and frontend encoding.

Buffering an entire result does not establish rollback or connection ownership,
and makes memory proportional to result size. The chosen design owns a
transactional statement scope while streaming, validates each output inside
that scope, and finishes the backend operation before sending final success.
It avoids a second value-encoding pass and does not require whole-result
materialization. An explicit outer transaction and an autocommit command use
different scope boundaries.

## Admission and owned inputs

The execution entry point accepts an immutable, semantically admitted plan,
not arbitrary SQL plus a caller-supplied category. Parsing, emission, native
type checks and a guessed first keyword cannot construct this plan. Admission
must establish the supported AST forms and functions, parameter identity and
coercions, result metadata/encoding, command effects and catalog conditions.
Unverified constructs fail before preparing or executing backend SQL.

Only the semantic admission component can construct this plan. Its constructor
is private, and its SQL, category, binding layout and conditions are opaque and
immutable to callers. A public struct with writable SQL/category fields, or a
public constructor accepting them, would bypass admission even if named
"admitted". Parsing and native statement checks retain their separate input
types and cannot promote themselves into this executable plan.

The ordinary statement lane initially admits only supported transactional
query/INSERT/UPDATE/DELETE operations. A query may contain write effects through
CTEs or functions; a SELECT root does not prove a read-only operation. Session,
transaction, DDL, COPY and multi-statement commands have separate controllers
and cannot enter the statement lane. In particular, COMMIT must never execute
inside a scope intended to recover one failed statement.

An admitted plan carries an explicit dense backend binding layout and the full
frontend arity. Frontend arity is checked before eliminating unused bindings;
native binding arity is checked against the backend description. SQL text is
never scanned again to discover parameters.

Catalog-dependent admission requires an execution validity contract for the
bound objects. Fresh reads alone do not close the gap between metadata lookup,
translation, preparation and execution. That contract must cover the complete
dependency set, snapshots, locking or validation, temporary objects and
external DDL. Its exact algorithm remains an unresolved M2/M4 design gate.
Reusable catalog-dependent plans stay disabled until it is implemented and
verified. Bypassing a cache does not by itself satisfy this live execution gate.

The connection owner is the only backend SQL boundary. It tracks the current
outer transaction and active statement/stream; callers cannot interleave a
raw COMMIT or another command on the same connection. A newly created owner
must know the backend is idle. Wrapping an arbitrary Client and issuing BEGIN
could join an existing transaction and later commit somebody else's work, so
the constructor must establish ownership from the connector lifecycle rather
than assume idle state. Returning this owner to a pool is a later integration
step with its own verified reset contract.

## Scope lifecycle

The owner maintains these distinct functional states:

| State | Meaning | Permitted next action |
| --- | --- | --- |
| Ready | Known outer transaction state, no statement in progress | Start an admitted statement or an explicit transaction control |
| Starting | Scope creation/preparation in progress | Finish setup or recover; no second command |
| Streaming | Rows and completion are being consumed inside the scope | Validate/encode the next row, finish, or recover |
| Finishing | Completion and output are valid; release/commit is pending | Confirm completion or recover/discard on uncertainty |
| Recovering | A statement failed or its consumer stopped early | Confirm rollback and scope removal |
| Uncertain | Cleanup or completion was not confirmed | Dispose of the backend connection; no SQL reuse or success |

The guard marks the owner busy before the first asynchronous backend action.
Stopping a future or dropping a stream cannot turn it back into Ready. Drop
cannot await rollback; an async owner must complete recovery, or dispose of the
connection. A cleanup failure must preserve both the original error and the
cleanup outcome. It cannot be swallowed, changed into success, or followed by
an optimistic pool return.

For an autocommit operation, begin an explicit backend transaction before
preparation/catalog-dependent work. Keep it open through output validation.
Commit only after confirmed statement completion and successful output
validation. On a recoverable statement error, roll back the transaction and
confirm idle state. No nested statement savepoint is needed in this case.

For an operation inside an explicit transaction, create an owned statement
savepoint before backend preparation. On success, release it after output and
completion checks. On statement-local failure, roll back to it **and release
it**, preserving earlier work in the outer transaction. PostgreSQL
[ROLLBACK TO](https://www.postgresql.org/docs/18/sql-rollback-to.html) keeps the
named savepoint; [RELEASE](https://www.postgresql.org/docs/18/sql-release-savepoint.html)
removes it. Reusing the same name without releasing it creates hidden nested
scopes rather than cleanup. Internal savepoints and client savepoints need
separate identities under the owner, without changing client-visible semantics.

The recovery policy is semantic input. Some frontend errors require an entire
transaction rollback; they must not be converted into statement-local recovery
merely because PostgreSQL can recover a savepoint. The MySQL 8.4 differential
transaction matrix must establish that policy before it is advertised.

The tokio-postgres 0.7.18 nested transaction helper rolls back to a savepoint
without releasing it. Its transaction commit/rollback methods also mark their
object finished before awaiting backend confirmation. Using these helpers
alone cannot establish the owner states above. The owner must track completion
and uncertainty itself; it cannot infer a clean connection from helper Drop.
Confirmation includes the actual transaction state and control-command outcome.
A successful generic command call alone is insufficient: COMMIT in an aborted
PostgreSQL transaction can complete as ROLLBACK. The connector integration must
expose and verify the required completion and
[ReadyForQuery state](https://www.postgresql.org/docs/18/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-READYFORQUERY),
rather than turn that outcome into a committed-write acknowledgement.

The [backend completion component](backend-completion.md) exposes those native
events. The [native control checker](native-controls.md) verifies fixed internal
control tag/state expectations and retains failure observations. Neither
component alone supplies connection ownership or a rollback scope. The
[exclusive owner](native-backend.md) now retains the connector lifecycle and
checks its own controls under borrowed scopes. Admitted statement/stream
integration and the verification above remain pending.

## Rows, encoding and completion

Prepare inside the scope, perform native parameter/result representation checks,
and establish the admitted frontend metadata before Bind/Execute. Borrow values
in their validated backend order. A checked native type can still produce a
range error; a raw column NOT NULL flag cannot establish projected nullability.
Domains and native typmods must retain their separate catalog/result meanings.

The row path validates native values and encodes them against the already
established frontend description inside the scope. Encoder failure aborts the
statement just like decoder failure. Returning unchecked rows to a caller that
can finish the transaction without completing encoding is not this contract.
Any plan with several backend steps or internal RETURNING data owns all of them
under the same rollback boundary.

A normal result may stream rows before its final outcome is known. If a later
row fails, terminate the result with the explicit protocol error after backend
recovery; never append a success terminator. Internal RETURNING data used for
insert IDs or affected-row emulation is not a frontend result and must not leak
onto that stream. Apply staged frontend session updates only at the successful
statement boundary specified by the compatibility policy.

Drain backend completion before finishing the scope. tokio-postgres attaches
its prepared description to rows; comparing those descriptions is not portal
re-description or catalog freshness. `RowStream::rows_affected()` is optional
until completion. A missing count is not zero: admitted command contracts must
check their required completion, including an explicit zero count. Empty SQL,
suspended portals and unsupported command outcomes cannot become successful
zero-row results through a default value.

Report final success only after confirmed commit in autocommit mode, or
confirmed statement-savepoint release inside an outer transaction. A connection
loss or stopped future during that finish may leave an unknown outcome; do not
retry the write or claim that rollback succeeded. If frontend transport fails
after a confirmed commit, the operation is committed even though the client
missed its final response. The owner records the actual known outcome.

Rollback covers transactional backend effects. Native
[sequence advancement](https://www.postgresql.org/docs/18/functions-sequence.html)
is not undone by rollback; the executor must not rewind sequences or promise
gapless identifiers. Other nontransactional native effects require explicit
semantic decisions rather than an invented blanket rollback guarantee.

## Cost and required functional evidence

Explicit autocommit adds BEGIN and COMMIT around the prepare/execute path.
An outer transaction adds SAVEPOINT and RELEASE; statement failure requires
ROLLBACK TO followed by RELEASE. These are real protocol and locking costs,
not free wrappers. Recovery commands may share a batch only when dependent
failure behavior is correct. Pipelining, proven read-only shortcuts and reusable
plans require separate correctness and performance evidence before enabling
them. The initial implementation favors a correct ownership boundary.

Ordinary PostgreSQL 17/18 fixtures must demonstrate:

- Successful zero-row, non-row and multi-row operations with explicit completion.
- Unsupported descriptions and wrong binding arity before execution.
- Decoder and frontend encoder errors after DML RETURNING, with table writes
  rolled back and native sequence behavior preserved.
- Backend constraint and prepare errors, preserving earlier outer-transaction
  writes where statement-local recovery is required, followed by a valid write.
- A deferred constraint failing at autocommit COMMIT after a successful
  statement. Record the confirmed rollback separately from an unknown finish
  outcome, return the commit error rather than success, and allow the next
  operation only after confirmed idle state.
- Several-step emulation failure and repeated statement failures with released
  internal savepoints; explicit client savepoints retain their own behavior.
- A consumer ending normally or stopping early, with the owner's next command
  allowed only after confirmed recovery. Uncertain completion is a disposal
  outcome, never a successful reset.

Native backend execution evidence does not certify MySQL errors, warnings,
affected rows, insert IDs, transaction effects or wire behavior. Those require
the corresponding differential, protocol and driver matrices. Security-related
work and adversarial/resource stress verification remain outside the current
user-requested scope. This document does not change those deferred gates.
