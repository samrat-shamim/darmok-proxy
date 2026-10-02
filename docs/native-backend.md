# Exclusive native backend ownership and control scopes

Status: **implemented control lifecycle component; not a statement executor**.
`NativeBackend` connects through the existing PostgreSQL connector, retains the
client and driver privately, and submits only its own fixed initialization and
transaction controls.
It cannot adopt an arbitrary `Client`, implement `GenericClient`, or expose a raw
SQL method. A caller cannot prepare or execute statements through this API.
The [execution contract](native-execution.md) still requires semantic admission,
live catalog validity, and rollback through row decoding and frontend encoding.

## Construction and ownership

A new owner retains both halves returned by the connector and starts one driver
task. Before returning, it submits `ROLLBACK; SET search_path = pg_catalog` and
checks both complete command tags and idle ReadyForQuery state. The
[native lookup context](native-lookup.md) keeps application schema routing
explicit; PostgreSQL's temporary namespace remains implicit. This initialization
is deliberately restricted
to a fresh connection: it cannot roll back another caller's existing work. It
adds one protocol round trip per connection and provides an observed idle state
without changing the connector's startup processing. A missing SET tag or other
initialization failure is an error; the uncertain owner is disposed and is not
returned to its caller.

The owner is neither clonable nor shareable as an independent SQL handle.
Transaction methods require a mutable borrow. Its Debug output contains the
lifecycle state only. The connector argument is forwarded unchanged; this work
adds no authentication, TLS, credential mapping, or authorization behavior.

Explicit `begin(spec)` requires confirmed idle and a complete
[native transaction specification](native-transactions.md). `commit` and
`rollback` require a confirmed transaction or failed transaction. Idle frontend
COMMIT/ROLLBACK semantics, frontend transaction-mode mapping, client savepoints
and session changes belong to the future frontend controller, not guessed
behavior in these internal methods.

## Control lifecycle

| State | Meaning | Next action |
| --- | --- | --- |
| Ready(I/T/E) | The owner's latest complete request confirmed this native state | A permitted explicit control or new scope |
| Controlling | Internal SQL has been started; readiness is not confirmed | Finish the control, or dispose after abandonment |
| Scoped | A borrowed transaction/savepoint scope is open | Finish or recover through that scope |
| Uncertain | A started future/scope was dropped or cleanup was unconfirmed | Dispose only |

`Ready` describes a confirmed historical request. It does not predict future
connectivity. Exclusivity ensures no unrelated command can be queued behind
that request through this owner. A future which has never been polled submits
nothing; after its first backend action a private guard marks the owner busy
before any await. Dropping that guard without complete confirmation marks the
owner uncertain. No asynchronous Drop rollback or optimistic reset is used.

The [control checker](native-controls.md) consumes a fresh stream submitted
inside this boundary and requires exact tags and state. A COMMIT completed as
ROLLBACK remains an error with a confirmed idle backend, never a committed
write receipt. A backend COMMIT error followed by idle, including a deferred
constraint failure, also permits later reuse while retaining the error. An
ordinary BEGIN failure confirmed idle leaves no scope; a failed SAVEPOINT
creation with a confirmed transaction state permits full outer rollback.
Other unexpected completion mismatches, missing readiness, stream errors, and
failed rollback/release/recovery require disposal. An observed ReadyForQuery
alone is insufficient evidence that a failed cleanup removed its owned scope.

## Borrowed scope identities

In idle state, `transaction_scope(spec)` begins an owned transaction with
explicit isolation/access and NOT DEFERRABLE. `savepoint_scope()` requires an
explicit transaction and creates a savepoint named `darmok_statement_<serial>`.
Neither method accepts the other's state; savepoints retain the parent's
characteristics. The generated integer
identity is private, checked for overflow, and never reused on that connection,
including after a failed creation attempt. Future client savepoint identities
must occupy a separate namespace under the owner. No caller-selected name or
SQL participates in an internal control.

The returned scope borrows its owner mutably. This prevents interleaved parent
controls at compile time. Forgetting the scope does not make its parent ready:
the runtime state remains Scoped and rejects another control or scope. Dropping
an unfinished scope marks the parent uncertain.

`finish` commits an owned transaction or releases its owned savepoint.
`recover` requires an explicit `NativeRecovery` choice from the controller:

| Native boundary | Requested recovery | Required control and final state |
| --- | --- | --- |
| Owned transaction | Statement or Transaction | ROLLBACK, idle |
| Savepoint inside an outer transaction | Statement | ROLLBACK TO and RELEASE of the same owned identity, transaction |
| Savepoint inside an outer transaction | Transaction | ROLLBACK, idle |

There is no default recovery choice. Whole-transaction recovery discards earlier
outer-transaction work and all client/internal savepoints in one request. It
does not first attempt savepoint recovery and therefore does not depend on that
savepoint still existing. A successful receipt identifies the actual control
and confirmed native state; a request to recover alone is not confirmation.
Both tags and the final state are required for statement-savepoint recovery.
A partial recovery or missing savepoint preserves its native error and leaves
the parent uncertain. Dropping either recovery future also leaves it uncertain,
including an unpolled consuming future which drops its unfinished scope.

The caller keeps any original statement error separately from the cleanup
result. Error classification and frontend state changes remain controller work;
this component does not invent a MySQL recovery policy. Native savepoint rollback
also does not certify [MySQL lock retention](https://dev.mysql.com/doc/refman/8.4/en/savepoint.html):
[PostgreSQL releases locks](https://www.postgresql.org/docs/18/explicit-locking.html)
acquired after a rolled-back savepoint. Data effects and final native state alone
are insufficient evidence for frontend transaction equivalence.

No rows can currently be obtained through this scope. When the admitted row
executor is integrated, it must hold the scope privately through decoding,
encoding and completion; it cannot expose unchecked rows alongside this
control-only finish method. Scope completion by itself is not statement
admission or output validation.

## Disposal

`dispose` consumes the owner, drops its private client, aborts and awaits its
local driver, and returns the preceding state. Driver errors retain their
native or task error source. If the disposal future is stopped, owner Drop
still aborts the driver; it cannot detach a running driver task. Ordinary owner
Drop also aborts the driver without awaiting it or issuing SQL.

Disposal confirms local driver termination when awaited. It does not confirm
server rollback or resolve an uncertain COMMIT. No retry, committed-success
receipt, reset receipt, or pool-return API is derived from it. Pooling, bounded
shutdown and statement/stream disposition remain integration gates.

## Costs and functional evidence

There is one driver task per connection and one initialization control round
trip. BEGIN/COMMIT/ROLLBACK and savepoint creation/release each cost their own
control request. Transaction start selects static SQL with explicit modes,
without an additional SET or readback request. Savepoint recovery combines two dependent commands into one
request, with both outcomes checked. Savepoint SQL allocates a bounded string;
whole-transaction recovery from either boundary uses fixed static SQL and one
control request. It avoids a savepoint-recovery request followed by a separate
outer rollback, without claiming measured latency. Control checking retains its
fixed matched prefix and first error/mismatch, not an unbounded event history. There
is no row allocation, result buffering, cache, pipelining or performance claim
in this component.

Private database unit fixtures can exercise table effects without opening a
public raw SQL escape. They are ignored in database-free workspace jobs and
**required explicitly** by both PostgreSQL CI jobs:

```text
cargo test -p darmok-execute --lib --locked native_backend::tests -- --ignored
```

The command requires `DARMOK_TEST_DATABASE_URL` and fails if it is absent or
unavailable. Fifteen ordinary fixtures on each supported backend cover successful
transaction/scope effects; repeated rollback-and-release preserving earlier
writes and a separate client savepoint; prepare and constraint errors followed
by valid writes; deferred COMMIT and COMMIT-as-ROLLBACK failures; invalid parent
controls; inert unpolled futures and abandoned pending controls; dropped scopes
and finish futures; failed savepoint creation; and failed savepoint cleanup.
The four whole-transaction recovery fixtures additionally cover earlier writes
and all savepoint identities being removed; recovery after a native constraint
error at either boundary followed by a valid scope; recovery after an earlier
client rollback removed the internal savepoint; and abandoned unpolled/pending
full recovery at either boundary.
The two transaction-characteristic fixtures additionally cover all six native
isolation/access combinations through explicit BEGIN and transaction scopes,
unchanged session defaults, inherited savepoint characteristics through recovery
and release, and wrong-boundary errors. They check native settings, not MySQL
snapshot or lock equivalence.
Fixture SQL uses private test access and is not an admitted statement path.
No forced transport interruption, security review, adversarial inputs or
resource stress is included. MySQL behavior and release gates remain pending.

The [command-phase connection owner](frontend-connection.md) now consumes this
native owner together with its TCP transport and session. It confirms ordinary
disconnect rollback separately from local disposal and implements selected
source controls; it adds no raw SQL escape or row execution.
