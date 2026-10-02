# Observed backend completion

The vendored `tokio-postgres` 0.7.18 library exposes exact command, text
simple-query, prepared query and built-in typed-query events. This is a connector component, not an executor, semantic plan,
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

`Client::simple_query_events(sql)` queues one simple-query request and exposes
each native `RowDescription`, text row, exact `CommandComplete`, `EmptyQuery`,
`BackendError` and final `ReadyForQuery`. Each row shares the description of its
own statement, including a zero-column description. Completing a statement or
receiving a backend error discards the stream's current description; the next
statement cannot reuse it. Multi-statement requests retain their ordered events
and one final request state. A parse error can precede any description; an
execution error can follow rows; a deferred commit error can follow a command
tag. None of those observations alone acknowledges frontend success.

`SimpleColumn` preserves the native label, table/column origin (zero means no
origin), raw type OID, type size, typmod and format. It does not turn OIDs into
connector `Type` objects, query type metadata, or consult a type cache. Values
are PostgreSQL text representations, with NULL distinct from an empty string;
this does not decode MySQL values or certify a PostgreSQL type's semantics.
Unknown native type OIDs are valid descriptions in this text transport.

The event stream requires format 0 for every column. PostgreSQL simple queries
normally use text, but a `FETCH` from a `BINARY` cursor can return binary
columns. Such a description terminates with an explicit format error, before
any row is interpreted as text. COPY and other unsupported response sequences
also terminate explicitly. These are observations after submission, not
pre-execution rejection or cleanup receipts. The owner must supply verified
internal SQL, exclusivity, and an unconfirmed-operation disposition; this API
adds no public SQL entry point to NativeBackend.

This lane permits a row-returning utility such as SHOW without adding an
extended-protocol Parse/Bind cycle. It does not establish that a utility is
snapshot-neutral or that a reported lease is live. Server-module presence,
fresh execution, exact backend identity and complete native guards remain
separate catalog-control requirements tracked in
[issue #46](https://github.com/samrat-shamim/darmok-proxy/issues/46).

`Client::query_events(statement, parameters)` accepts an already prepared
`Statement` and an exact-size parameter iterator. It encodes parameters and
queues Bind/Execute/Sync once. A local arity/type/encoding error returns before
queuing this request. It emits rows, an exact command tag or a distinct
empty/suspended outcome, backend errors, and final ReadyForQuery. Errors before
BindComplete remain observable, including executing inside a failed native
transaction. This API does not implicitly prepare SQL or certify its semantics.
Checked native bindings can supply the iterator, and native result checks can
decode its rows. Unsupported frontend cursors remain a separate capability.

The [described portal lane](native-portals.md) separately queues
Bind/portal Describe/Sync before Execute. `query_portal_events` then supplies
the same explicit query completion events using that portal's actual columns.
It retains the portal and source statement while the stream exists. Its row
limit is native protocol control, not an admitted frontend cursor capability.

`Client::query_typed_builtin_events(sql, typed_parameters)` combines Parse,
Bind, statement Describe, Execute and Sync in one request for fixed internal
queries whose outputs have known built-in types. It retains preparation errors
as well as execution and Sync errors through final ReadyForQuery. Each supplied
parameter has its native type; encoding happens once before submission. A local
encoding error queues no part of this request. The server's result description
is observed after submission, including column labels, types, relation/attribute
origins and typmods, even when no rows are returned. `columns()` returns None
until a complete description is observed; NoData has a present empty description.

This one-shot lane cannot supply frontend pre-execution admission or a native
prepared-check receipt: Execute is already queued when the description arrives.
An unknown result OID causes a terminal error whose source is
`UnsupportedBuiltinResultType`, retaining the exact OID. It does not consult the
connector type cache, issue a hidden lookup, invent a type or convert to text.
This explicit restriction applies even to a zero-row result or a previously
cached custom type. Declared native types can still be returned as OID/name
catalog facts with built-in output representations; those facts remain separate
from the result description. Parameters may use caller-supplied native types;
this lane is restricted by result types, not an invented input-type fallback.

A terminal description error occurs after SQL submission and can follow backend
effects. It provides no ReadyForQuery, rollback or safe-reuse receipt. The owner
must apply its unconfirmed-operation disposition. Only known fixed internal
queries belong here; admitted frontend SQL retains its separate prepare/check
and rollback requirements. NativeBackend exposes neither this raw SQL method
nor a public Client.

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

All these streams are fused after their ReadyForQuery event or terminal `Err`.
`CommandEventStream::has_yielded()` and
the simple and built-in typed-query streams' `has_yielded()` record whether an event or terminal error
has already been returned. A checker requiring the entire request can reject
handoff of a partially consumed stream. Pending-only polling does not set the
flag; it is not an execution, readiness or ownership observation.
Dropping or stopping a stream supplies no confirmation and performs no owned
rollback. Prepared rows retain the statement's cached description; typed and
described-portal rows use their respective observed native descriptions.
Neither supplies full catalog freshness. Output may precede a later
backend error; output decoding/encoding and frontend success remain inside the
future owner's rollback/completion boundary.

## Cost and ordinary functional verification

These APIs reuse the existing request queues, parameter encoder and native Row
construction. They add no SQL query, protocol exchange, per-row value encoding
or whole-result buffer. One owned string retains each command tag. Prepared
rows keep the existing statement reference-count operation; binding keeps the
upstream format/parameter container allocations. The one-shot typed lane
collects the supplied pairs before Parse, then uses the upstream encoder's
format and parameter containers for Bind; it avoids a separate parameter-OID
vector. One local unnamed description owns
the column names/types and is shared by its rows, with no server statement-close
request on Drop. The typed, prepared and described-portal lanes share
row/error/completion logic.
The text simple-query lane shares one column-name/metadata allocation across
each statement's rows and uses the existing `SimpleQueryRow` field ranges.
Retaining raw column facts adds fixed metadata per column, also to upstream
simple-query descriptions; it adds no per-row lookup or value conversion.
These are structural costs, not a measured performance claim.

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

Eight further required typed-query fixtures on each PostgreSQL version cover:

- Typed inputs encoded once, exact native column origins/typmods and zero-row
  descriptions; empty SQL, NoData, zero-column rows and explicit zero counts.
- Parse errors, domain input failure during Bind before a result description,
  a row before execution failure, failed transactions and confirmed recovery
  followed by a valid operation.
- Deferred Sync errors after rows/CommandComplete, with and without RETURNING,
  preserving the error and confirmed state rather than acknowledging a write.
- Local input-type errors without a queued partial request; precise custom
  result-OID rejection, including zero rows and a populated connector type cache.
- Queued typed and prepared requests whose values, descriptions and states
  remain associated with their own boundaries after later unnamed Parse calls.

These fixtures do not implement or certify the executor, recovery after a
stopped consumer, uncertain transport outcomes, pooling, MySQL transaction
policy or wire behavior. Security-related work and adversarial/resource stress
verification remain outside the current user-requested scope.

Ten required text simple-query fixtures additionally cover native SHOW tags
and descriptions; multiple statements with separate labels/origins/typmods;
NULL, empty and Unicode text; empty SQL, zero rows/columns/counts; custom type
OIDs; parse and row-producing execution errors; failed-transaction/savepoint
recovery; deferred implicit commit failures before the last tag; queued simple
and prepared requests; SHOW preceding the first repeatable-read data snapshot;
explicit binary cursor rejection; and local encoding failure without a partial
queued request. Their native observations do not certify a server catalog
control or its nonblocking guard design.
