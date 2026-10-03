# Owned local prepared commands

Status: implementation candidate; expanded corpus checks and independent review
are pending. The preceding corrected revision passed the required suites.
Table execution, native reusable plans and release certification remain open.

`FrontendConnection` exclusively owns its typed statement registry. PREPARE
parses the original source once with the current SQL mode and admits a direct
local SELECT. It stores literal values, selected variable references and direct
`?` slots. Later mode changes cannot reinterpret literals; variable values are
read from current session/server authorities at execution. No arbitrary backend
SQL capability, registry adoption or public prepared execution API is exposed.

The registry carries opaque admitted payloads, original source and binding
types. The connection constructor supplies explicit count and SQL-byte limits.
CLOSE releases the source accounting and template; IDs are not reused during
the connection. These limits bound retained source, not total template,
transport or parameter memory. Full resource accounting remains a separate gate.

PREPARE sends ID and separate parameter/result definitions. EXECUTE sends fresh
definitions and a binary row through existing packet/row encoders. Both EOF
negotiation forms are handled. Query attributes, optional metadata, cursors and
long data remain explicitly unsupported under the connection contract.

Direct slots initially declare nullable VAR_STRING in the immutable session
text charset. Selected bindings are NULL, signed/unsigned TINY, SHORT, LONG and
LONGLONG, and VAR_STRING with valid UTF-8 bytes. INT24 is explicitly unsupported:
the recorded stock non-NULL inputs fail instead of producing integer rows.
Non-NULL integers derive LONGLONG with width 21
and explicit signedness. NULL preserves the derived description, even before
the first non-NULL binding. Cached input types and derived result types are
independent; RESET preserves both. A string input after numeric derivation
requires unimplemented MySQL coercion/warnings and returns1235 before result
output, without resetting the derived type. Other parameter types, parameter
expressions, tables, clauses, SET/transaction preparation and batches fail
explicitly. The remaining coercion/native prepared gates are not satisfied.

PREPARE clears diagnostics and preserves ROW_COUNT, FOUND_ROWS and insert ID;
it does not execute SELECT. EXECUTE records a one-row SELECT. RESET clears
conditions and records ROW_COUNT0. CLOSE sends no packet, including unknown
IDs, and preserves diagnostics. Unknown EXECUTE/RESET IDs return1243.
Output guards settle only after packets are written and flushed. Transport or
internal-contract errors terminate the owner and dispose the native connection.

Preparation parses once and retains source/template allocations. Execution
resolves cells and constructs a row/result-description vector. Literal strings
share byte buffers; decimal values currently clone stored text for encoding.
This lane adds no PostgreSQL request, catalog cache, lock or module load.
Measured latency, memory and allocation gates remain open.

The stock corpus captures ordinary MySQL 8.4.11/PyMySQL 1.1.2 packets for metadata,
repeated bindings, reset, changed session values, frozen quoted literals and
close, plus signed/unsigned bounds for each admitted integer input type and
stock INT24 rejection. Its ROW_COUNT/FOUND_ROWS observation invokes deprecated FOUND_ROWS: the
warning count includes that query's own warning, not the preceding count.
Separate PING packets establish prepare/reset clear and close preserve behavior.
TCP fixtures replay supported packets and reject the observed unimplemented
numeric-string conversions and INT24 bindings with explicit 1235 errors; those
are unsupported cases, not equivalent stock error packets. Stock legacy EOF observations and documented EOF
negotiation remain separate evidence.

Primary contracts: [PREPARE](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_com_stmt_prepare.html),
[EXECUTE](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_com_stmt_execute.html),
[RESET](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_com_stmt_reset.html),
[CLOSE](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_com_stmt_close.html),
and [pinned MySQL8.4.11 handlers](https://github.com/mysql/mysql-server/blob/99960bf74fa919347e4f4e3ca47672f333d6e91f/sql/sql_prepare.cc).
Upstream implementation bytes stay outside the Apache distribution. No security,
native C/build, existing-profile lifecycle or hosted CI work is included.
Concurrent native PostgreSQL2PC remains required.
