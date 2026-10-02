# SET execution

The internal SET evaluator integrates a catalog-independent SET subset
with session state, exclusive native ownership, native commit confirmation and
an encoded response sent through `AsyncWrite`. It is a library component; there
is no runnable proxy or complete transaction controller. The
[selected query controller](query-controller.md) admits original COM_QUERY SQL
with the current session mode before calling this evaluator.

The supported variables are `sql_mode`, `autocommit`, `transaction_isolation`
`transaction_read_only` and `completion_type`. Bare names inherit SESSION/LOCAL keyword scope;
per-name qualifiers apply only to that name. Unqualified `@@` transaction
assignments target next choices independently of keyword context. Direct
`SET [SESSION|LOCAL] TRANSACTION` updates defaults; unqualified direct settings
update next choices. These distinctions follow the reviewed
[stock observations](mysql-set-semantics.md).

Values are selected string/boolean/integer literals, ON/OFF and reads of these
five variables with ordinary session or global scope. Server global values are
an explicitly supplied `ServerSetValues` authority. DEFAULT reads that authority
from the immutable values supplied for this command; its transaction-state
checks run at its ordered update position. It does not use ambient PostgreSQL
defaults. Global writes are not implemented. Numeric mode assignments
support zero and the observed four/ANSI_QUOTES value; other numeric mode masks
fail explicitly. Warning-producing SQL modes, general coercion, functions,
arithmetic, user variables, subqueries, prepared forms and other settings are
unimplemented and return an explicit 1235/42000 error before ordinary updates.

Support for every expression/value, including DEFAULT, is checked before effects.
This prevents an unsupported DEFAULT from following an irreversible native
commit. All ordinary expressions read pre-update values, and all ordinary
semantic checks precede updates. An ordinary invalid value or active next-choice failure prevents every
update. DEFAULT checks instead run in update order: a later 1568/25001 leaves an
earlier confirmed mode change in place, while a first DEFAULT failure prevents
later updates. A known SQL error is encoded, sent, recorded as an error diagnostic
and settled without erasing those effects. Error message wording is project
wording; exact stock text is not certified.

One `SessionCommandStage` retains the statement outcome across all assignments. It
keeps authoritative setting reads and translation identity unavailable until
the complete success/error response is encoded, written and flushed. Abandoning
that stage retains before/last-confirmed settings and prevents reuse. Its model
effect/output methods assert a caller contract, just like existing transaction
stages. The execution function supplies the actual native and output operations;
calling model methods alone does not prove either operation.

An active autocommit-zero to one change first confirms native COMMIT and idle
ReadyForQuery, then records the frontend end and setting change. A native idle
state alone cannot stand in for that receipt. Unknown native/output outcomes
remain errors requiring disposal, without invented recovery or DML replay.
Completion-type labels, integers 0/1/2 and booleans derive from one typed
transaction setting. The [transaction controller](query-transactions.md) resolves
explicit completion clauses against it; release completion remains an explicit
unsupported result.

Changes while a frontend transaction is active with autocommit already enabled
remain explicitly unimplemented under the existing transaction contract.

Output requires protocol 4.1 without session tracking. Session tracking needs
its own declared setting/change-block controller and is rejected here. Success
packets report active transaction, read-only active choice, autocommit and
NO_BACKSLASH_ESCAPES flags with zero affected rows/insert ID/warnings. The OK
encoder now takes capabilities explicitly: ordinary info is EOF-delimited;
tracked info/state are length-encoded. See the
[MySQL OK packet specification](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_basic_ok_packet.html).
Full capability negotiation, query response metadata and driver behavior remain
separate gates.

## Verification boundary

Two ordinary guard tests verify retained partial outcomes and unavailable
authorities. Ten new required native-owner tests exercise parsed SET inputs,
scope/next choices, pre-update reads, ordinary validation, DEFAULT error order,
retained PostgreSQL data and rollback, global DEFAULT authority, selected
coercions, response bytes, a native autocommit commit with lasting data, and
unsupported DEFAULT admission before changes or commit.
They are selected controller postconditions derived from the reference corpus,
not execution of all nineteen complete MySQL scripts through a proxy.

The active frontend start/rollback boundaries are trusted fixture setup.
PostgreSQL BEGIN is explicitly configured for the test and is not a certified
MySQL isolation mapping. The native table DML is private fixture setup, never
a public arbitrary-SQL API. The test does not establish a start controller,
catalog leases, row admission, isolation/locking equivalence or real MySQL driver
compatibility. At `d5626740bd583af7f284d236485241ac1cfc8391`, all ten groups
pass on PostgreSQL 17.11 and 18.6. Independent passive review found an unsupported
DEFAULT admission gap in the initial implementation, then confirmed its
correction at this revision with no new concrete finding. Source review does
not constitute independent dynamic database evidence. Required CI remains blocked by the
recorded personal-account Actions condition; no merge/release is justified by
local results alone.

Local setting updates add no native request, native cache lookup or lock. A
required active autocommit commit adds one native control request. The plan uses
one vector of fixed-size actions; string literals borrow the AST, canonical
variable string reads allocate, and response payload/framing use byte buffers.
There is no new duplicate mutable mode/transaction authority. End-to-end cache
hit rate, allocation/latency/throughput measurements and controller performance
remain unmeasured. M2/M4 and the full runnable-proxy/release goal remain incomplete.
Security-related implementation and testing remain outside this step.
