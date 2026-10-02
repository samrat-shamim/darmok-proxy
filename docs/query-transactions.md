# Original-source transaction controls

The public `execute_query_command` library path connects selected transaction
controls to the exclusively owned native backend. It parses original MySQL
source using current session modes, admits the whole plan before effects, and
holds the session command guard through response encoding, writing and flushing.
This is a library component; the runnable frontend loop and public table SQL
executor remain pending.

## Admitted contract

`START TRANSACTION` accepts optional `READ ONLY` or `READ WRITE`, separated by
commas. Repeating identical access is valid. Contradictory access, START
isolation syntax, `BEGIN TRANSACTION`, completion `TRANSACTION`/`TRAN` keywords,
standalone `END`, reordered clauses and completion clauses on ROLLBACK TO are
MySQL syntax errors. `BEGIN` and `BEGIN WORK` use pending choices/defaults.
The parser retains `WITH CONSISTENT SNAPSHOT`; admission rejects it with 1235
before any implicit commit. Its snapshot and warning behavior is not simulated.

`COMMIT` and `ROLLBACK` retain optional WORK, absent/positive/negative CHAIN,
and absent/positive/negative RELEASE choices in the AST. This matters because
`AND NO CHAIN` differs from an omitted clause. Session `completion_type` is a
canonical `NO_CHAIN`/`CHAIN`/`RELEASE` value. Initial policy comes from the
compatibility profile; GLOBAL reads and SESSION DEFAULT use explicit
`ServerSetValues`, without consulting PostgreSQL defaults. Selected SET forms
accept enum labels, integers 0/1/2, booleans, named defaults and canonical reads.
Explicit clauses override their corresponding policy choice independently.
Implicit commits do not apply completion policy.

Release completion returns explicit 1235 before effects, including a bare
completion under RELEASE policy. Explicit `NO RELEASE` can suppress it.
The controller borrows an output writer and native owner; it does not own the
frontend socket lifecycle. That owning loop must implement and verify RELEASE
before it can be admitted. Client savepoints, prepared controls, batches,
failed-start recovery and general row statements remain outside this component.

## Ordered boundaries

Admission and staging share `TransactionSettingsSnapshot::plan_command`, a
pure fixed-size plan. START precedence is explicit access, pending access,
then default access; isolation uses pending isolation or its default. A
confirmed start consumes both pending choices. START inside an active
transaction first commits the old transaction, then begins the admitted new
pair. Both controls are admitted before that preceding commit can occur.

Completion without chaining clears active and pending choices, even when
already idle. Idle chained completion selects the pending/default pair before
resetting choices. Active chaining carries the preceding active pair despite
changed defaults. A confirmed idle frontend end submits no native control:
exclusive ownership has already established native idle. Active completion
uses checked native COMMIT/ROLLBACK, never readiness alone.

Three named native isolation settings and two access settings produce the six
explicit native configurations. READ UNCOMMITTED is rejected with 1235 rather
than aliased to READ COMMITTED. These configurations and their readbacks do not
certify MySQL/PostgreSQL snapshot, serialization, error or lock equivalence.

`SessionTransactionStage` is nested inside `SessionCommandStage`. Each native
tag and readiness receipt precedes the corresponding model boundary. Finishing
known model effects keeps the outer command pending until the whole response
is sent. A later unknown operation retains the before/last-confirmed history
and remaining boundary; it cannot undo a known preceding commit or certify an
SQL error packet. Native/output failures require disposal under the existing
contract. No recovery or reusable state is invented.

## Finite evidence and remaining gates

`mysql_query_transactions.json` declares sixteen ordinary stock reference
cases, including data effects, active/idle chaining, completion defaults,
negative overrides, enum coercions and exact syntax-error attribution.
`mysql_completion_type_columns.json` declares two CLI field descriptions.
Existing reference corpora and observers retain their bytes.

Eight required native fixture groups invoke the public decoded query path on
both PostgreSQL versions, inspect encoded responses/native settings and check
private fixture data effects. Private setup DML is not an implemented public
write path. Two in-memory guard tests check retained whole-command and partial
transaction histories; they are not native or transport receipts.
Fresh committed verification and independent review evidence are recorded in
the [release plan](release-plan.md); no gate closes from fixture declarations.

Planning copies at most two steps and snapshots without allocation or locks.
Idle nonchained completion adds no native request; ordinary start/active end
uses one, and active replacement/chaining uses two ordered requests. Existing
response buffers and parser allocations remain. No cache or automatic replay
is added. Measured latency, allocations, contention and cache behavior remain
open integrated-workload gates, as do real drivers, both end-to-end examples,
native/created schemas, row execution, catalog validity, snapshots and locks.
M2/M4 and the full release goal remain incomplete. Security work stays deferred.

Primary references: [MySQL transaction controls](https://dev.mysql.com/doc/refman/8.4/en/commit.html)
and [completion policy](https://dev.mysql.com/doc/refman/8.4/en/server-system-variables.html#sysvar_completion_type).
The pinned stock observations supply the finite idle and coercion cases;
same-named native controls alone are not equivalence evidence.
