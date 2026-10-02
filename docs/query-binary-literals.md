# Binary literals in local SELECT

The original-source COM_QUERY controller evaluates hexadecimal and bit-value
literals as binary strings in its existing local SELECT lane. No application
tables, PostgreSQL functions, catalog lookup or native statement are used.
Public table execution and the complete catalog-validity gate remain pending.

Supported spellings are `X'hex'`, `x'hex'`, `0xhex`, `B'bits'`, `b'bits'` and
`0bbits`. Quoted hex requires an even number of ASCII hex digits; unquoted hex
accepts an odd number with a leading zero nibble. Bit digits are ASCII zero/one
and a partial leading byte is zero-padded. Leading zero groups remain part of
the value. Empty quoted literals produce an empty byte string, distinct from
NULL. Values are not converted through a 64-bit integer or UTF8 text.

Parentheses, unary plus and explicit aliases retain their existing result/name
contracts. A `_binary` introducer on these hex/bit forms preserves the bytes
while selecting its separate metadata contract. Other introducers, `_binary`
on ordinary quoted strings, numeric conversions, unary minus, arithmetic,
bitwise operations and explicit COLLATE remain unimplemented and fail before
result output. Existing unsupported query clauses still fail before effects.

The MySQL lexer gives bit literals their own `BitStringLiteral` token and AST
value. Foreign byte-string AST behavior is retained. Quoted digit forms do not
use ordinary string backslash decoding; invalid quoted digits are syntax
errors under both backslash modes. Unquoted prefixes require lowercase `x` or
`b` and at least one digit. A following identifier character keeps the whole
token a name: `0x41g` and `0b012` cannot become a literal with an implicit alias.
The current local lane reports those unimplemented names as 1235/42000;
stock MySQL reports unknown-column errors. General name/error precedence is
still a separate gate. These rules follow the MySQL 8.4
[hex](https://dev.mysql.com/doc/refman/8.4/en/hexadecimal-literals.html) and
[bit](https://dev.mysql.com/doc/refman/8.4/en/bit-value-literals.html) contracts.

All selected binary literals use VAR_STRING and binary collation 63. Declared
width is the byte count, independent of session text collation. Unintroduced
hex carries NOT_NULL, BINARY and UNSIGNED flags, with decimals 0; bit carries
NOT_NULL and BINARY, with decimals 0. Introduced hex/bit carries NOT_NULL and
BINARY, with decimals 31. Source labels preserve their complete original
spelling rather than rendering the AST. Existing name normalization and
unsupported warning/truncation conditions still apply.

`mysql-binary-literals.json` records stock MySQL 8.4.11 observations under four
combinations of ANSI_QUOTES/NO_BACKSLASH_ESCAPES. It contains 44 positive cases,
28 quoted-syntax cases and 44 unimplemented contexts/names. Positive cases
cover 216 field declarations and cells, empty/NULL/zero distinctions, arbitrary
bytes, leading groups, 64/65-bit values, aliases, parentheses, plus and comments.
The syntax/unsupported groups retain stock outcomes separately from the
proxy's narrower error contract.

Introduced empty hex/bit values, a parenthesized introduced zero value and an
introduced unary-plus bit value have their own successful query in every mode.
These cells are checked in the positive pure and both-EOF TCP loops rather than
being witnessed only inside a rejected mixed query.

The repository observer uses pinned PyMySQL 1.1.2 for stock fields and binary
values, checks Docker image identity/content digest and verifies the connected
server UUID against that fixture. It uses the existing test login; it implements
no handshake or authentication. It preserves subprocess streams, each actual
stock outcome and the complete observation receipt in a new external directory.
PyMySQL's distribution version is 1.1.2; its module reports 1.4.6. Non-binary
values in unimplemented reference contexts are client-decoded values, not raw
server row payloads. No authenticated proxy driver exchange is claimed.

```text
python -m pip install -r tests/reference/requirements-mysql-binary.txt
python -B tests/reference/observe_mysql_binary_literals.py --container <reference-container> --image mysql:8.4@sha256:6ea90827b1100f8f2ae306a539f86d2c264a26ed435a2a9f75551dd5c3aeb242 --port <mapped-port> --corpus crates/execute/fixtures/mysql-binary-literals.json --evidence-dir <new-external-directory>
```

`MYSQL_PWD` is the existing stock fixture environment input. Future CI creates
a private Python virtual environment from the pinned requirement, runs this
observer as a required check, and always uploads its receipt directory. Hosted
CI remains pending under the recorded account blocker.

Pure tests compare all captured positive declarations and bytes. Two required
native-owner groups use real ordinary command-phase TCP exchanges with both
EOF formats, current modes, diagnostic replacement, pending-setting preservation,
and earlier outer writes surviving rejected input followed by valid results
and commit. Setup DML is private fixture SQL. PostgreSQL 17/18 and BigDecimal
runs are required separately; declarations alone do not certify those runs.
Exact committed evidence belongs in the release plan after verification.

The lexer validates digit payloads in O(input bytes) without a second SQL scan.
Decoding allocates one exact-capacity byte vector per binary cell and transfers
it to Bytes without copying. No intermediate numeric string, whole-input UTF8
conversion, cache, lock or PostgreSQL round trip is added. Existing response
buffering and label allocations remain. Measured allocation counts, latency,
resource bounds and complete M2/M4/driver/release gates remain pending.
Security work and compiler PR4 remain excluded.

Verified code/fixture revision `e6ccbf39b835be3d44620464eb7d1c61652147a2`,
tree `0d4589614348cef92ca95326e022a3af694ae849`, passes the 14-command local
matrix: PostgreSQL 17.11/18.6 ordinary owners and BigDecimal frontend loops,
workspace all/default features, strict Clippy, minimal/std/visitor parser
builds, formatting, repository boundaries, diff and 116 fresh stock cases.
The independent source/correction/expanded-fixture reviews found no functional
defect; the positive introducer coverage concern is closed. Exact commands,
counts, environment and immutable evidence hashes are recorded in
[the release plan](release-plan.md). These results certify that code revision;
later documentation leaves retain a separate tree/evidence proof.
