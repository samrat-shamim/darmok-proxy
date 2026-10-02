# Command-phase connection ownership

`darmok-execute::FrontendConnection` owns one TCP transport, the frontend
`SessionState`, an exclusive `NativeBackend` and the explicit `ServerSetValues`.
Its consuming `run` method handles the command phase until normal QUIT, EOF,
successful transaction RELEASE or a terminal execution/transport error. It
returns historical state and independent cleanup results, never a reusable
socket or PostgreSQL handle.

This is the execution boundary after a separately completed handshake. It has
no listener, handshake bypass, authentication, credential configuration, route
selection or pool. The standalone `darmok` executable and both end-to-end
examples still require their remaining implementation. TLS transport ownership
belongs to the deferred security work; this finite component owns `TcpStream`.

## Commands and output

The owning loop dispatches the existing original-source selected SET, local
SELECT and START/BEGIN/COMMIT/ROLLBACK controllers. The former borrowed public
`execute_query_command` entry point is private, with no compatibility wrapper.
Public SQL execution cannot acknowledge RELEASE through a borrowed writer and
then leave a frontend open. The internal controller remains independently
testable with library buffers; those tests do not certify a socket exchange.

Existing framing reassembles logical packets and preserves prefetched command
bytes supplied from the preceding phase. Each command starts at sequence zero;
the response starts after its last frame. Reads and execution are serial, and
the next command cannot execute until output writing/flushing completes. Local
SELECT retains both legacy EOF and CLIENT_DEPRECATE_EOF output. Compressed
streams, query attributes, session tracking and optional metadata are explicit
unsupported entry contracts rather than guessed framing modes. A complete
logical packet at EOF is still processed; incomplete framing is a terminal
error rather than a normal disconnect.

PING sends OK with current session status and zero affected rows/insert id/
warnings. It records ROW_COUNT as zero, preserving the SQL condition list,
LAST_INSERT_ID, FOUND_ROWS, transaction modes and next choices. The declared
behavior follows the pinned ordinary command dispatch; complete stock driver
diagnostics/wire equivalence remains a separate gate.

Other decoded commands requiring a response receive an explicit 1235/42000
error before effects. They are not implemented by this component. Prepared
close and long-data commands have no response in the MySQL protocol. Since
prepared execution is not implemented, receiving one ends the connection with
an explicit `FrontendError::NoResponseCommand`; it sends no invented packet and
does not silently accept the command. Ordinary decode/framing errors are
terminal, rather than successful or recoverable SQL results.

## Release and termination

Explicit RELEASE and inherited completion_type=RELEASE resolve independently
of chaining. MySQL forbids both explicit positive clauses together, but a
default policy and an explicit clause can resolve both choices to true. The
native ordered controls and corresponding frontend boundaries complete first;
the complete OK is written and flushed before the owner stops reading commands
and shuts down the frontend. Chaining retains its existing characteristic
contract and status flags, even if the resulting empty transaction is then
rolled back during disconnect cleanup. Explicit NO RELEASE suppresses closure.
Unsupported READ UNCOMMITTED starts or other unadmitted controls still fail
before prior effects, with no successful release receipt.

QUIT sends no packet. Normal QUIT/EOF and successful RELEASE close the frontend;
when the exclusive native owner has a confirmed active state, cleanup performs
one checked ROLLBACK before disposal. This can discard earlier writes on a
normal disconnect or close an empty chained transaction after RELEASE. On a
terminal error, the owner disposes without inventing transaction recovery or
confirmed rollback. Dropping the runner drops the socket and the backend owner,
whose existing Drop aborts the driver; asynchronous rollback is not invented.

`FrontendReport` keeps four independent facts: command-loop end/error, frontend
shutdown result, optional checked rollback result and local driver disposal
result. Any failed cleanup remains visible. Local disposal does not prove a
server rollback or resolve an uncertain commit. The returned SessionState
describes the last confirmed command and may still show a transaction later
rolled back during disconnect; it is not reusable execution authority.

## Costs and evidence scope

The connection has one input buffer and one existing packet assembler. It
reuses the existing per-command output buffers and native driver. No reader
task, shared lock, cache or extra query lookup is added. Local SELECT, PING and
local SET need no native request; existing active SET commits and transaction
requests retain their own documented costs. Normal active termination adds one
ROLLBACK. Measured latency, memory, allocations, concurrent workloads and bounded
shutdown remain open gates; no result is inferred from this request count.

`mysql_frontend_release.json` declares sixteen stock CLI lifecycle cases. The
separate observer disables client reconnect, requires the completion's Query
OK, checks either exact trailing-query disconnect or successful NO RELEASE
continuation, reads committed table effects through another connection, and
confirms disposable database absence. Expected CLI exits1 remain actual exits1
in receipts. CLI observations do not expose raw server flags or certify proxy
equivalence; the resolved chain status is corroborated by the pinned server
dispatch, separately from observable closure/data effects.

Seven required native fixture groups use real ordinary TCP command-phase
exchanges on PostgreSQL17/18. They check serial selected commands/current SQL
modes, both EOF formats, status and error/continuation output, all sixteen
release choices, discarded prefetched input, persistent commit/rollback effects,
normal coalesced/split frames, RELEASE received directly over TCP, normal
QUIT/EOF rollback and verified fixture removal, and explicit termination
without invented prepared close/long-data responses. Setup DML is private fixture
SQL; public table execution and authenticated real-driver exchanges remain
pending. Existing selected controller regressions remain required separately.
Fresh committed results and independent review are recorded in the release
plan; fixture declarations alone close no gate.

Primary references: [transaction completion](https://dev.mysql.com/doc/refman/8.4/en/commit.html),
[pinned command dispatch](https://github.com/mysql/mysql-server/blob/99960bf74fa919347e4f4e3ca47672f333d6e91f/sql/sql_parse.cc),
[PING](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_com_ping.html),
[QUIT](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_com_quit.html),
[prepared close](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_com_stmt_close.html)
and [long data](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_com_stmt_send_long_data.html).

No M2/M4, runnable artifact, catalog-validity, native/created-schema workload,
snapshot/lock equivalence, real-driver, performance, CI, merge or release gate
is closed by this component. Security work and the separate compiler draft
remain excluded.
