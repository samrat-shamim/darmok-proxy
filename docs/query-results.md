# Local SELECT results

The private query controller beneath [FrontendConnection](frontend-connection.md)
admits a single direct SELECT with
no table or other clauses. Exact numeric literals, booleans, NULL and ordinary
quoted strings produce one text row. Parentheses, selected unary plus
expressions, numeric unary minus and explicit aliases retain their MySQL label
semantics.
[Exact decimal and large-integer literals](exact-numeric-literals.md) retain
their value, scale, signedness and declared-width contracts. Approximate
floating-point and general expressions are explicit 1235/42000 errors, pending
their own value, metadata and diagnostic contracts. Queries with WHERE, LIMIT,
ORDER BY, DISTINCT, hints, tables, CTEs or other clauses are not approximated.
Query and Select fields are exhaustively checked at admission.

MySQL executable comments (`/*! ... */`, including version-prefixed forms)
return an explicit 1235/42000 before SELECT or SET effects. They need a separate
server-version and expression-label contract. The upstream tokenizer expands
them unconditionally with coordinates that omit removed delimiters and version
digits; neither expansion nor an offset-only repair provides that contract.
The source API retains executable comment tokens in its single tokenization
and rejects them before parsing. Quoted values/identifiers containing the same
characters, ordinary comments and line comments retain their normal behavior.
See [MySQL's executable-comment semantics](https://dev.mysql.com/doc/refman/8.4/en/comments.html)
and [support issue #32](https://github.com/samrat-shamim/darmok-proxy/issues/32).

The selected variable reads are autocommit, transaction isolation/read-only,
sql_mode, time_zone, client/connection/results character sets, connection
collation, version, version comment and compile OS. Explicit GLOBAL reads of
the four mutable settings use the supplied server authority. The version
variables are global-only: SESSION/LOCAL returns 1238/HY000. Session-only
authorities do not invent GLOBAL values. Other canonical variables, including
diagnostic counts, remain explicit unsupported errors. Unknown registry names
return 1193/HY000 with project wording. These are selected semantics, not a
complete variable registry execution contract or MySQL error precedence proof.

`parse_mysql_source` owns an immutable AST and borrows the original SQL. During
the same parse it captures complete projection ranges, independently of
semantic Expr spans that omit unary operators and parentheses. A SourceSelect
borrow binds its query, projections and spelling to that parse; callers cannot
substitute a constructed AST. Original numeric tokens remain available when
the optional BigDecimal AST representation normalizes zeros or exponent text.
Coordinates count Unicode characters. All byte endpoints are resolved in one
character walk after sorting; numeric lookup uses binary search. Source
contract failures are explicit caller errors, not AST-rendered label fallbacks.

[Binary literals](query-binary-literals.md) additionally support hex/bit forms
and their `_binary` introducers, including exact byte values and separate
declared metadata. Their source, numeric-context and error limits remain distinct
from ordinary quoted strings.

Declared column metadata is independent of encoded row length. Integers use
LONGLONG, binary collation, signed/unsigned flags and literal declared widths;
Exact decimals and promoted integer expressions use NEWDECIMAL with independent
declared width and original fractional scale. NULL uses its own type and null
marker. Strings use the immutable session text authority, character count
times charset maximum width, and string decimals 31.
System-variable strings declare 21845 characters: the MySQL system charset's
65535-byte declaration divided by its three-byte maximum, then converted to
result charset bytes. Empty source identifiers and original names describe
expressions, rather than inventing table provenance.

Generated labels trim leading ASCII nongraphic characters and convert
supplementary Unicode characters to `?` under MySQL's three-byte system name
charset; row text still carries the full utf8mb4 value. Explicit aliases whose
normalization needs warnings, and names needing truncation, are unsupported.
Their warning/truncation outcomes are separate compatibility gates.

The pinned MySQL 8.4.11 source explains these declaration and protocol facts:
[variable declarations](https://github.com/mysql/mysql-server/blob/99960bf74fa919347e4f4e3ca47672f333d6e91f/sql/item_func.cc),
[expression/name metadata](https://github.com/mysql/mysql-server/blob/99960bf74fa919347e4f4e3ca47672f333d6e91f/sql/item.cc),
and [column serialization](https://github.com/mysql/mysql-server/blob/99960bf74fa919347e4f4e3ca47672f333d6e91f/sql/protocol_classic.cc).
Upstream implementation excerpts are kept outside the Apache distribution;
only contracts and independent implementation are included here.

The response contains column count, definitions, a legacy metadata EOF when
required, the text row, then EOF or OK-as-EOF according to CLIENT_DEPRECATE_EOF.
The existing packet encoder owns fragmentation and sequence advancement. The
caller supplies the initial response sequence. The command guard stays pending
through the complete write and flush. Success clears old diagnostics, records
ROW_COUNT as -1 and FOUND_ROWS as 1, and preserves LAST_INSERT_ID. Status derives
from confirmed session settings and active transaction state. Local evaluation
does not start or finish a native transaction or consume next-transaction
choices. These reads make zero PostgreSQL requests.

The opt-in source path adds vectors of spans, endpoints and byte ranges to
SELECT parsing; AST-only APIs retain their existing allocation behavior. SET
without a nested SELECT does not build numeric provenance or scan coordinates.
Executable-comment admission checks the retained token vector once without a
second lexer, source scan or additional per-token allocation.
Result admission allocates column and cell vectors and owned output names/text.
Already normalized names reuse their Bytes allocation; supplementary-character
conversion allocates a new name. Output is buffered for the whole small result,
with one payload buffer per logical packet and one aggregate response buffer.
No cache, catalog lookup or new lock is introduced. End-to-end allocation and
latency measurements remain mandatory release gates.

`mysql_query_results.json` and its stock observer check five ordinary CLI column
declarations with the pinned image, exact server version, original transcripts,
child exits and source identities. CLI flags include its computed NUM flag;
they are deliberately distinct from server packet flags. These observations
are neither a raw stock wire capture nor a proxy differential run. Required
native fixtures decode COM_QUERY and verify output bytes, both EOF forms,
sequence wrapping, diagnostics, current modes, defaults/next choices, and
preservation of an owned active read-only transaction on PostgreSQL 17 and 18.
Trusted fixture starts do not certify public transaction SQL or isolation
equivalence. At `029609697d270b44adf982df6e5df0de767559e2`, four SELECT and
fourteen SET fixture groups pass on each backend; both workspace suites and
strict local checks pass. Independent passive review closes the original
executable-comment provenance defect and test-module placement lint failure,
with no new finding. It audited saved author receipts without rerunning Cargo
or SQL. Exact commands, environment, counts, original failures and evidence
boundaries are recorded in [the release plan](release-plan.md).

Selected transaction starts and the owning command-phase loop are implemented
in separate components. Table execution, catalog coherence, prepared commands,
a runnable proxy, authenticated real MySQL driver exchanges and release
publication remain incomplete. Account CI is unavailable under the recorded Actions condition.
Security-related work and the separate compiler draft remain excluded.
