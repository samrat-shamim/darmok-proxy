# Selected COM_QUERY execution

`darmok-execute::execute_query_command` takes a decoded `Command::Query` and
parses its original SQL with the settled session's current `sql_mode`. It
dispatches one SET statement to the [selected evaluator](set-controller.md) or
one local SELECT to the [result controller](query-results.md), or selected
controls to the [transaction controller](query-transactions.md).
There is no public execution function accepting a constructed SET AST. The
pure input classifiers remain syntax views, rather than execution entry points.

The parser recognizes both MySQL SET assignment operators, `=` and `:=`.
They normalize to the same AST because they have the same assignment semantics.
The shared `TO` alternative belongs to other dialects. MySQL direct SET
TRANSACTION uses one characteristic, optionally the other kind after a comma;
the shared PostgreSQL list grammar still serves other dialects. MySQL START
and completion clauses have their own grammar. No source string rewriting or secondary token scan is used.
These rules follow the pinned MySQL 8.4.11 grammar, revision
`99960bf74fa919347e4f4e3ca47672f333d6e91f`; upstream implementation source is
kept outside the Apache distribution. The parser is not a complete MySQL
grammar validator, and support remains limited to the selected SET, local SELECT and transaction shapes.

The complete command is parsed before any effect. A successfully parsed batch or any
other unimplemented parsed statement returns 1235/42000 before an earlier SET can update settings or
commit a transaction. Batch execution is unimplemented regardless of client
capabilities; empty/comment-only input is also explicitly unimplemented.
A parser failure returns 1064/42000 with project wording. General MySQL error
precedence and exact error message text are separate gates. A non-query Command
is a caller error and emits no SQL response; the caller must dispatch it to its
own controller. This is not a complete frontend command loop.

`SessionCommandStage` replaces the SET-only name: it holds setting effects and
SQL diagnostics until the complete OK, ERR or SELECT packet sequence is written and flushed.
Known SQL errors settle the command without losing confirmed effects. Native
or output uncertainty retains the pending guard and requires disposal. This
component does not implement cancellation recovery or a transport server.
The response sequence is supplied by the caller's wire phase. Protocol 4.1
without session tracking or optional result metadata remains the required output
contract. Both ordinary EOF and CLIENT_DEPRECATE_EOF result terminators are
implemented for the local SELECT path.

All ten existing SET native groups now decode ordinary COM_QUERY payloads and
use this public source path. Four additional required native groups cover `:=`
and mixed-operator setting effects, both characteristic orders, current-mode
parsing across commands, and rejection of a valid unsupported statement/batch
with native rollback data proving the batch did not commit. Frontend starts and
private DML are fixture setup, not a transaction start controller or an
isolation mapping. In-memory output verifies frame bytes and flush completion;
it does not establish a network exchange with a MySQL driver.

`tests/reference/mysql_set_source.json` declares four ordinary stock MySQL
observations for the assignment operators, pre-update reads and both direct
characteristic orders. Its unchanged observer and required CI step preserve
version/image identity, exact output and completion/cleanup receipts. These
observations certify the stock expectation, not a full differential run through
a proxy. At `042b374a89121b025c701d30fdd94d967edfba5a`, all fourteen native groups
pass on PostgreSQL 17.11 and 18.6, both workspace suites and strict local checks
pass, and independent passive source review has no unresolved finding. The
review audited saved execution receipts without independently running tests.
Stock observations remain bound to clean `29ab912`; the one-file correction
changes only a foreign-dialect classifier test. Exact commands, environments,
counts and evidence boundaries are recorded in the release plan.

The command decodes one owned SQL string using the existing protocol decoder
and parses it once. Parsing creates tokens and the statement AST; admission
builds a vector of fixed-size actions for SET commands. The local SELECT path's
source/row allocations are documented in [query-results.md](query-results.md).
There is no new catalog lookup, cache invalidation mechanism or lock. Local
setting/result operations add no native request; an active autocommit commit
sends one. Transaction-control requests are specified in their own contract. End-to-end performance
measurements remain pending.

Table row execution, complete transaction behavior, prepared execution, catalog coherence, a
runnable proxy and real-driver/release gates remain incomplete. Required account
CI is unavailable under the recorded Actions condition. Security-related work
and the separate compiler draft remain excluded.

Local SELECT implementation and its current validation boundary are documented
in [query-results.md](query-results.md). The prior SET results above remain
revision-bound evidence; they do not certify subsequent code.

Selected SET values now retain original numeric token provenance through a
private immutable source witness. Decimal/exponent forms fail before any effect
regardless of the optional parser numeric representation. SELECT continues to
use its own projection witness from that same parse; no secondary lexer or AST
formatting supplies either admission path.

The [source transaction controller](query-transactions.md) additionally admits
selected original START/BEGIN/COMMIT/ROLLBACK inputs. Its whole ordered native
plan is admitted before effects, and nested transaction receipts remain under
this same command output guard. RELEASE, consistent snapshots, row execution
and the owning frontend loop remain pending.
