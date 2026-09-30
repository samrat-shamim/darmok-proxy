# Observed backend completion

The vendored `tokio-postgres` 0.7.18 library exposes exact command and prepared
query events. This is a connector component, not an executor, semantic plan,
connection owner, catalog validity lease or MySQL support claim. The
[execution contract](native-execution.md) still requires those boundaries.

## Why expose events

Upstream `batch_execute` discards command tags and ReadyForQuery states.
`RowStream` exposes an optional affected-row count, without the command tag or
transaction state. A generic successful command call cannot prove a COMMIT
actually committed: an aborted transaction can complete COMMIT as ROLLBACK.
A deferred constraint can instead produce a commit error and return to idle.
The executor must distinguish those outcomes from losing backend confirmation.

A narrow addition to the existing connector preserves these messages rather
than implementing another PostgreSQL transport. Its source pin, licenses and
modified files are recorded in [the vendored dependency](../third_party/tokio-postgres/UPSTREAM.md).
Existing connector helpers keep their upstream behavior. All Darmok crates
use the same local dependency; no separate registry copy is linked.

## API and message meaning

`Client::command_events(sql)` queues simple-query commands that do not return
row descriptions. It emits each exact `CommandComplete` tag, `EmptyQuery`,
`BackendError` and final `ReadyForQuery`. It can observe several command tags
in one request. The caller must validate the complete expected sequence; a
later error does not erase earlier tags or prove earlier explicit commits were
undone. Unexpected row descriptions/data terminate this stream with an error.
That observation occurs after SQL submission, so this primitive cannot reject
unverified SQL before its effects. The future owner supplies validated internal
controls; frontend SQL must not enter through this method directly.

`Client::query_events(statement, parameters)` accepts an already prepared
`Statement` and an exact-size parameter iterator. It encodes parameters and
queues Bind/Execute/Sync once. A local arity/type/encoding error returns before
queuing this request. It emits rows, an exact command tag or a distinct
empty/suspended outcome, backend errors, and final ReadyForQuery. Errors before
BindComplete remain observable, including executing inside a failed native
transaction. This API does not implicitly prepare SQL or certify its semantics.
Checked native bindings can supply the iterator, and native result checks can
decode its rows. Unsupported frontend cursors remain a separate capability.

Backend SQL errors are `BackendError` events containing a database error, not
successful command completions. Continue consuming to ReadyForQuery to observe
the transaction state after that error. A stream `Err` instead means parsing,
transport or unexpected-response failure and terminates without confirmation.
Neither an error event alone nor a command tag alone proves readiness.

In native autocommit, Execute can emit rows and CommandComplete before Sync
finishes the implicit transaction. A deferred constraint can fail during that
finish. The stream retains the earlier tag and rows, then the precise backend
error and final state; the command tag cannot become a write-success
acknowledgement. It must not reject that late SQL error as an unexpected message.

ReadyForQuery reports exactly Idle (`I`), Transaction (`T`) or FailedTransaction
(`E`); unknown codes fail explicitly. The backend
[command tag](https://www.postgresql.org/docs/18/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-COMMANDCOMPLETE)
is retained as supplied, with no missing/unknown count converted to zero.
The [transaction state](https://www.postgresql.org/docs/18/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-READYFORQUERY)
belongs to this request's completion boundary. It is not a live global state
lookup: other requests can already be queued or executing on the same Client.
The future owner must supply exclusivity and validate expected control outcomes.

Both streams are fused after their ReadyForQuery event or terminal `Err`.
`CommandEventStream::has_yielded()` records whether an event or terminal error
has already been returned. A checker requiring the entire request can reject
handoff of a partially consumed stream. Pending-only polling does not set the
flag; it is not an execution, readiness or ownership observation.
Dropping or stopping a stream supplies no confirmation and performs no owned
rollback. Rows retain the prepared statement's cached description, not a new
portal description or catalog freshness proof. Output may precede a later
backend error; output decoding/encoding and frontend success remain inside the
future owner's rollback/completion boundary.

## Cost and ordinary functional verification

These APIs reuse the existing request queues, parameter encoder and native Row
construction. They add no SQL query, protocol exchange, per-row value encoding
or whole-result buffer. One owned string retains each command tag. Prepared
rows keep the existing statement reference-count operation; binding keeps the
upstream format/parameter container allocations. There is no benchmark claim.

Eight required fixtures on PostgreSQL 17/18 exercise:

- Exact control tags, multi-command completion, explicit zero counts and empty
  query outcomes.
- A statement error with failed-transaction state, followed by COMMIT completing
  as ROLLBACK and a valid later write.
- A successful insert followed by a deferred constraint error at COMMIT, with
  confirmed idle state and rolled-back table rows.
- An autocommit prepared INSERT, with and without RETURNING, whose deferred
  constraint fails at Sync after CommandComplete; retain its exact SQL error
  and idle state, with no table rows left behind.
- Prepared rows, native bindings/decoding, empty/zero-row/zero-column results,
  exact labels and one parameter encoding pass.
- A row before an execution error, a subsequent error before BindComplete,
  savepoint rollback/release, preserved earlier work and a valid later write.
- Local parameter encoding/arity errors without a partial queued execution.
- Queued requests whose command tags and reported states remain associated
  with their own completion boundaries.

These fixtures do not implement or certify the executor, recovery after a
stopped consumer, uncertain transport outcomes, pooling, MySQL transaction
policy or wire behavior. Security-related work and adversarial/resource stress
verification remain outside the current user-requested scope.
