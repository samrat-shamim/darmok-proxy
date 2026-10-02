# Native internal-control completion

`darmok-execute::check_native_control` checks a request's observed native control
outcome. It consumes the connector's [command events](backend-completion.md);
it does not submit SQL, own a connection or admit a frontend command. The
[execution contract](native-execution.md) still requires those boundaries.

## Expected outcomes

The caller chooses one fixed expectation for its known internal control:

| `NativeControl` | Exact command tags, in order | Final ReadyForQuery state |
| --- | --- | --- |
| `Initialize` | `ROLLBACK`, `SET` | Idle |
| `Begin` | `BEGIN` | Transaction |
| `Commit` | `COMMIT` | Idle |
| `Rollback` | `ROLLBACK` | Idle |
| `Savepoint` | `SAVEPOINT` | Transaction |
| `RollbackTo` | `ROLLBACK` | Transaction |
| `Release` | `RELEASE` | Transaction |
| `RecoverSavepoint` | `ROLLBACK`, `RELEASE` | Transaction |

Success requires every expected tag, no additional or empty command, no backend
or stream error, and the expected final state. A `NativeControlCompletion` has
private fields and is constructed only after those checks. A command tag alone
cannot construct it. These expectations cover the owner's ordinary internal
controls, without `AND CHAIN`, transaction-prepared operations or arbitrary
batches.

`Initialize` describes the owner's fixed fresh-connection request
`ROLLBACK; SET search_path = pg_catalog`. A tag cannot identify which setting a
caller changed. The [owner](native-backend.md) submits that exact SQL privately;
an arbitrary SET with matching tags is not a certified lookup context. Neither
this expectation nor the checker can adopt another caller's connection.

The connector records whether the stream has yielded any event or terminal
error. Passing a partially or completely consumed stream fails with
`AlreadyConsumed`; a matching tail cannot certify the full request. The checker
still drains any remaining events. Earlier consumed items are unavailable to
it and cannot be reconstructed. Polling Pending alone consumes no item and
does not prevent checking the complete request.

COMMIT completing as ROLLBACK is a tag mismatch, even when the backend confirms
idle state. A deferred constraint failure at COMMIT is a backend error, also
potentially followed by idle state. Neither is committed success. A matched
COMMIT tag followed by an unconfirmed request boundary remains a failure with
that tag observation retained; it does not establish a reusable connection or
justify automatically retrying a write.

PostgreSQL [COMMIT AND CHAIN](https://www.postgresql.org/docs/18/sql-commit.html)
and [ROLLBACK AND CHAIN](https://www.postgresql.org/docs/18/sql-rollback.html)
start a new transaction. Their tags can match an ordinary finish, but their
final Transaction state must fail its Idle expectation. This failure does not
undo or erase the control already observed.

## Failure information and draining

`NativeControlFailure` retains the control expectation, the number of matching
prefix tags, the first stream-handoff/tag/empty/completion mismatch, the backend
SQL error, the terminal stream error and the optional ReadyForQuery state
observed by the checker. A previously consumed stream has no trusted matched
prefix; errors or states consumed before the handoff are unavailable.
Database and terminal errors have separate accessors; a transport failure must
not erase a preceding SQL error. `Error::source` prefers the backend error when
present, while the complete failure remains available for disposition.

A tag mismatch, empty command or backend error does not stop draining. Continue
to the final ReadyForQuery or terminal stream error so the caller receives the
actual observed state. No observed state means unconfirmed completion. Even
when a failure has a state, it is not a success receipt or permission to reuse
the connection. The future owner decides recovery or disposal.

For `RecoverSavepoint`, one ROLLBACK tag is not complete cleanup. PostgreSQL
[ROLLBACK TO](https://www.postgresql.org/docs/18/sql-rollback-to.html) keeps the
savepoint. The checker requires the following
[RELEASE](https://www.postgresql.org/docs/18/sql-release-savepoint.html) tag and
usable transaction state. If release fails, the matching rollback prefix,
precise SQL error and final state remain observable; there is no optimistic
successful reset.

## What the observation cannot prove

The expectation describes an already queued request. It cannot validate the
SQL before execution or associate the events with an exclusive owner. A
BEGIN issued in an existing transaction can have the same tag and final state
as a new BEGIN. COMMIT/ROLLBACK outside a transaction also have ordinary tags
and idle state. Establishing initial state remains the owner's responsibility.

Savepoint names are absent from command tags. The checker cannot prove that
ROLLBACK TO and RELEASE used the same or intended savepoint, or distinguish
internal and client savepoints. The owner must generate those controls from
its private scope identity; callers cannot supply raw SQL to that owner.

ReadyForQuery belongs to this request's boundary, not a live global idle check
in the presence of other queued requests. Stopping the check future supplies
no result and performs no owned recovery. No connection lifecycle, pooling,
semantic admission, live catalog validity, MySQL recovery policy or frontend
acknowledgement follows from this API.

## Cost and verification scope

The checker adds no SQL, protocol exchange, result buffering or parameter
encoding. Success tracks a fixed expectation and at most two matching tags
without retaining their strings. Failure keeps the first unexpected tag and
the native errors; it does not accumulate arbitrary command history. Draining
has one constant-size update per event. This is an allocation/round-trip
analysis, not a benchmark.

Required ordinary PostgreSQL 17/18 fixtures cover successful controls and table
effects, aborted and deferred-error commits, repeated savepoint recovery,
partial recovery failure, missing/extra/empty outcomes, chained finishes and
errors before a control tag. A ninth fixture checks untouched, partially and
completely consumed streams and Pending-only polling. These fixtures do not
verify transport interruption,
connection ownership or MySQL semantics. Security-related work and
adversarial/resource stress verification remain deferred at the user's request.
