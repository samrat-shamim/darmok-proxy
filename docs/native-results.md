# Native output representation

Status: **implemented output component; no admitted native row executor**.
`NativeResultUtc` borrows checked native columns and the exact immutable
`ColumnDefinition` slice intended for the frontend. `new` uses a checked
[prepared statement](native-statements.md); `from_portal` uses the checked
[bound portal](native-portals.md) before Execute. Both check encoding
compatibility and validate/encode
each native row as a text or binary row payload. It does not prepare or execute
SQL, send metadata, frame packets, or finish a transaction.

## Description and value checks

A result needs at least one column and matching native/frontend column counts.
The portal constructor distinguishes NoData from an empty RowDescription and
rejects each explicitly. Neither is silently treated as an ordinary result set.
Unsupported encoding metadata fails at construction, including for empty or
NULL-only results. Names, aliases, relation origins, key flags and projected
nullability are semantic inputs: this component neither invents them nor proves
them from a native RowDescription. In particular, a catalog column's NOT NULL
constraint is not a proof that a projected value is non-NULL. A NULL value
contradicting supplied NOT NULL metadata fails at row encoding.

The [native scalar metadata mapper](native-metadata.md) can derive type,
charset, width, decimals and representation flags from reported types/modifiers.
It does not supply those semantic identities or establish a dependency lease.

All nineteen supported [native scalar types](native-values.md) have an explicit
output representation:

| Native reported type | Accepted frontend representation | Per-value conditions |
| --- | --- | --- |
| BOOL | TINY | 0 or 1 |
| INT2, INT4, INT8, OID | TINY, SHORT, INT24, LONG, LONGLONG, YEAR | Exact advertised signed/unsigned range; YEAR is 0 or 1901..2155 |
| FLOAT4, FLOAT8 | FLOAT, DOUBLE | Finite; FLOAT narrowing must preserve all value bits, including negative zero |
| NUMERIC | DECIMAL, NEWDECIMAL | Fixed declared scale, precision 1..65, scale 0..30; no rounding, padding or truncation |
| BYTEA | STRING, VAR_STRING, VARCHAR, BLOB | Binary charset; advertised maximum byte length |
| TEXT, VARCHAR, BPCHAR, NAME | STRING, VAR_STRING, VARCHAR, BLOB | Declared UTF-8 or binary charset; advertised maximum byte length |
| JSON, JSONB | JSON or the string/blob forms above | Native UTF-8 JSON bytes; JSON field metadata uses binary charset |
| DATE | DATE | Native valid date in years 1..9999 |
| TIME | TIME | Native 00:00:00..24:00:00; exact declared fractional precision |
| TIMESTAMP, TIMESTAMPTZ | DATETIME | Native valid date/time in years 1..9999; exact declared fractional precision; TIMESTAMPTZ components are UTC |

A NULL field representation accepts only NULL values from a supported native
type and cannot declare NOT NULL. BIT, ENUM, SET, ZEROFILL, fixed-scale floating
output and MySQL TIMESTAMP output are explicitly unsupported here. TIMESTAMP's
frontend range and session timezone policy must be established before enabling
that representation; DATETIME does not claim to implement those semantics.
UNSIGNED is accepted only for integers and decimals. Other supplied origin/key
flags remain admission's responsibility.

Numeric and temporal field metadata uses binary charset63. Integers, DATE and
NULL have decimals0; FLOAT/DOUBLE require decimals31; TIME/DATETIME use 0..6.
String/blob decimals must be 0 or31. Native text accepts only profile-declared
utf8mb4, utf8mb3 and binary collations. utf8mb3 rejects supplementary codepoints;
binary is an explicit byte-preserving representation, not charset conversion.
Other charsets fail before execution. BYTEA never becomes arbitrary UTF-8 text.

For decimal metadata, precision is `column_length` minus the decimal point
(when scale is nonzero) and minus the signed representation's sign allowance.
This follows MySQL's [decimal metadata convention](https://dev.mysql.com/doc/dev/mysql-server/8.4.7/my__decimal_8h.html).
The value's significant integer digits must fit precision minus scale; zero
integer digits do not consume precision. Numeric and temporal display widths
are not treated as maximum payload byte lengths. An unbounded native NUMERIC
description does not supply fixed frontend precision/scale automatically.
Ordinary stock MySQL 8.4.11 metadata observations confirm signed DECIMAL(2,2)
length 4 and DECIMAL(65,30) length 67. The valid value -0.12 has five payload bytes
despite length 4; a leading integer zero must not be mistaken for an overflow.

## Payloads and failure boundary

The result verifies the row's native description before decoding, including
names, types, typmods and optional relation/attribute origins. Prepared rows
carry cached statement facts; `query_portal_events` rows share the portal's
observed description. Only the portal guard compares that observed description
to the prepared facts before Execute. Neither establishes full dependency
validity. All native values are decoded and checked before any payload bytes are
written. Text output uses MySQL's [length-encoded values and NULL marker](https://dev.mysql.com/doc/dev/mysql-server/8.4.11/page_protocol_com_query_response_text_resultset_row.html).
Binary output uses the shared [binary row layout](https://dev.mysql.com/doc/dev/mysql-server/8.4.11/page_protocol_binary_resultset.html),
with the result NULL bitmap's two-bit offset and metadata-selected widths.
The shared writer reads the borrowed definitions directly, without cloning a
second metadata vector. INT24's four-byte binary container still requires a
24-bit value range.

Text integers and decimals preserve exact values. Floating text uses Rust's
shortest round-tripping display for the advertised FLOAT/DOUBLE precision; its
spelling is not yet certified against stock MySQL. Temporal text uses padded
date/time components and exactly the declared fractional digits. 24:00:00
remains 24:00:00, not midnight; YEAR zero is 0000. Native JSON preserves its
backend spelling, while JSONB retains the backend's normalized spelling.
No private-use escapes or zero-date sentinels are introduced.

Any description, decoder, representation or encoder error leaves the original
destination bytes and length unchanged. This component does not issue a success
terminator or commit. [Statement execution](native-execution.md) must keep its
owned rollback scope open through output validation and native completion. A
later failing row must cause recovery and an explicit result error, even when
earlier valid row payloads have already been streamed.

## Cost and verification limits

Construction performs one metadata pass and borrows both descriptions. Each row
uses the existing native decoder's owned value vector, one representation pass,
and one encoding pass. Text/string/decimal/byte payloads copy directly to the
destination; scalar text shares one bounded stack buffer with checked writes.
Binary output allocates no per-row metadata or NULL bitmap vector. There are no
extra PostgreSQL requests from the output wrapper, catalog reads, locks or cache
accesses, and no
whole-result buffering. Actual throughput and allocation measurements remain
release gates; these structural costs are not benchmark results.

Required PostgreSQL fixtures exercise all supported scalar codecs through the
described portal path, exact wire
payloads, empty/NULL metadata rejection, range/scale/charset mismatches,
temporal endpoints, and output buffer preservation. Private owner fixtures
test ordinary DML RETURNING through checked portals, with failures followed by
explicit recovery and success followed by confirmed finish. They observe the
Execute request's completion and readiness before owner finish/recovery. They
prove native data effects only, not MySQL
lock retention or an admitted public executor. Catalog execution validity,
generated result metadata, session timezone semantics, stock differential
formatting, table SQL, driver workloads and executable integration remain
pending. Security work remains outside the user-requested scope.
