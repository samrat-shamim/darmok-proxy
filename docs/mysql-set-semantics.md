# Stock SET execution semantics

Status: finite MySQL reference fixture; no SQL SET controller or PostgreSQL
compatibility claim is implemented by this fixture.

The [borrowed SQL input](session-sql-input.md) retains keyword scopes,
per-name qualifiers and source expressions. Applying each item as soon as it
is parsed would mix expression evaluation with mutation. Assuming the whole
statement is atomic would also discard an observed DEFAULT failure effect.
These are separate correctness obligations for the controller.

## Effective scopes

The [MySQL assignment specification](https://dev.mysql.com/doc/refman/8.4/en/set-variable.html)
describes keyword scope inheritance and the independent per-name @@ scope.
The [transaction scope table](https://dev.mysql.com/doc/refman/8.4/en/set-transaction.html)
distinguishes session defaults from next-transaction choices.

The fixture observes SESSION and LOCAL keywords carrying to later bare names.
An @@ qualifier applies to its own target. Unqualified @@ transaction-variable
assignments target the next transaction even when a SESSION or LOCAL keyword
appears earlier in the statement. Bare transaction variables update session
defaults. LOCAL TRANSACTION updates session defaults; it does not change an
already active transaction. Global and persistence writes are outside this
fixture.

For example, `SET SESSION sql_mode='', @@transaction_isolation='SERIALIZABLE',
transaction_read_only=1` produces one Serializable transaction, leaves the
isolation default at Repeatable Read, and changes the access default to Read
Only. The second transaction uses Repeatable Read and Read Only.

## Evaluation and failure phases

Ordinary system-variable expressions in the selected compound assignments
read values before updates. With a prior PIPES_AS_CONCAT mode,
`SET SESSION sql_mode='ANSI_QUOTES', sql_mode=@@session.sql_mode` leaves
PIPES_AS_CONCAT. A duplicate isolation assignment and an autocommit reference
in a different variable assignment also read the prior value. This evidence
does not establish ordering for user variables, subqueries or functions.

The general error-handling statement in the assignment specification describes
atomic failure. The stock fixture records a narrower result: invalid ordinary
values and checked next-transaction changes inside an active transaction fail
before updates, but DEFAULT is validated during its update. For example,
inside an active transaction,
`SET SESSION sql_mode='ANSI_QUOTES', @@transaction_isolation=DEFAULT` reports
1568 / 25001 and retains ANSI_QUOTES. Earlier transactional work remains
available until rollback, and rollback does not undo the mode change.
The same late failure occurs for @@transaction_read_only=DEFAULT. Reversing
the isolation DEFAULT and mode assignments prevents the later mode update.

This selected behavior agrees with the two-phase ordinary checks and ordered
updates in the pinned
[MySQL 8.4.11 SET implementation](https://github.com/mysql/mysql-server/blob/99960bf74fa919347e4f4e3ca47672f333d6e91f/sql/set_var.cc)
and its
[transaction-variable callbacks](https://github.com/mysql/mysql-server/blob/99960bf74fa919347e4f4e3ca47672f333d6e91f/sql/sys_vars.cc).
External source snapshots stay outside this Apache-2.0 distribution. No GPL
implementation is copied into Darmok.

SESSION DEFAULT copies the corresponding current global values for the four
observed variables. Unqualified @@ transaction DEFAULT targets only the next
transaction and leaves session defaults intact. The fixture also records
quoted ON/OFF, boolean literals, SQL-mode numeric value 4 and isolation ordinal
0. Recognition of their values does not implement coercion, native isolation,
SQL-mode effects or diagnostics in Darmok.

The controller must retain a validation phase and an ordered update phase,
with explicit DEFAULT handling and failure receipts for effects already
established. It cannot update borrowed input facts directly, evaluate all
DEFAULT assignments eagerly, or claim success from a local typed setter.
Native autocommit boundaries, frontend status, warnings and output validation
still require integration and independent evidence.

## Reproduction and limits

The [nineteen-case corpus](../tests/reference/mysql_set_semantics.json) uses the
unchanged observer with the stock MySQL 8.4 fixture. It includes seven ordinary
value or transaction-state errors with exact statement attribution. Each run
uses a fresh assigned disposable database and verifies cleanup.

```sh
python3 tests/reference/observe_mysql_transactions.py \
  --container "$MYSQL_CONTAINER" --image "$MYSQL_IMAGE" \
  --corpus tests/reference/mysql_set_semantics.json \
  --evidence-dir "$SET_EVIDENCE_DIR"
```

The caller supplies MYSQL_PWD for the existing fixture. CI requires this fifth
corpus independently and uploads `mysql-set-semantics` receipts. No missing
fixture is treated as a pass. The four earlier corpora and observer remain
unchanged.

The initial eighteen-case expectation draft completed fourteen cases, then
failed because four expected equality results were integers while MySQL
returned JSON booleans. Its actual failure, unrun cases and successful cleanup
remain preserved. The corrected corpus retains the observed boolean types and
adds the companion read-only DEFAULT failure case.

At revision `f7bf9fa0dc1394226cf019fa48bd88cb8f28722b`, the command above
completed all nineteen cases on the pinned stock MySQL 8.4.11 Linux arm64
fixture. The outer observer exited 0. Its thirty child processes exited 0:
twenty-five SQL clients, two image/container identity inspections and three
metadata commands. All twenty-five SQL completion markers and the seven
declared error attributions were verified, and final database absence was 0.
The observer and four earlier corpora are byte-identical to the parent.
Exact source snapshots, failed/corrected raw receipts, commands, environment
and hashes remain outside the distribution under
`.darmok-work/logs/set-semantics-reference-*`. Schema validation, diff checks
and repository boundary checks also exit 0. Rust source is unchanged, so these
reference checks do not re-certify the historical workspace Rust results.
Independent review identified that the LOCAL case assigned the access value
already established by reset. That case could not distinguish an inherited
session update from a next-only update or no change. The corrected case changes
the later bare value from 0 to 1 and requires both defaults and both successive
transactions to retain Read Only. The full nineteen-case followup run at the
revision above passes, including those distinct LOCAL effects. Independent
followup review confirms that the correction resolves the finding; seven
offline assertion groups pass with no new finding. The original ten-group
review and its finding remain preserved at their earlier revision. Neither
review executes the proxy or runs the native fixture. Actions is disabled
for the personal account, so fresh CI and merging remain pending.

Reference SQL includes session-local assignments, finite transaction controls
and ordinary data operations; it changes no global configuration. These checks
do not test the proxy, implement a SQL handler, or complete M2/M4, catalog,
driver, wire, performance or release gates. Security-related work remains
deferred.
