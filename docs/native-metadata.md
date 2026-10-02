# Native scalar field metadata

`darmok-execute::NativeTypeMetadataUtc` derives an immutable MySQL scalar
representation from a reported PostgreSQL type, its reported type modifier and
an explicit frontend text collation. `from_column` reads those two native facts
from a result description. It neither runs SQL nor creates a complete result
definition. Names, logical database aliases, original table/column identities,
projected nullability and key/default/identity flags still require semantic
admission and valid catalog dependencies.

The mapper supplies five representation fields: type, charset, length, decimals
and representation flags. It never derives NOT_NULL, key or AUTO_INCREMENT
flags from a RowDescription. It is used once per described output column, then
the existing [output encoder](native-results.md) validates actual values against
those fields. It does not accept a sample value as a substitute for metadata.

## Representations

| Reported native type | MySQL representation | Length and decimals |
| --- | --- | --- |
| BOOL | TINY | 1; decimals0 |
| INT2 / INT4 / INT8 | SHORT / LONG / LONGLONG | 6 / 11 / 20; decimals0 |
| OID | unsigned LONG | 10; decimals0 |
| FLOAT4 / FLOAT8 | FLOAT / DOUBLE | 12 / 22; decimals31 |
| NUMERIC(p,s) | signed NEWDECIMAL | p + sign + optional decimal point; decimals s |
| BYTEA | binary BLOB | u32::MAX byte bound; decimals0 |
| TEXT | text BLOB | u32::MAX byte bound; decimals0 |
| VARCHAR / BPCHAR | VAR_STRING | declared character bound × encoding width, or u32::MAX when unspecified; decimals0 |
| NAME | VAR_STRING | u32::MAX byte bound; decimals0 |
| JSON / JSONB | binary-charset JSON | u32::MAX byte bound; decimals0 |
| DATE | DATE | 10; decimals0 |
| TIME | TIME | 10 plus fractional suffix; decimals0..6 |
| TIMESTAMP / TIMESTAMPTZ | DATETIME | 19 plus fractional suffix; decimals0..6 |

Nontext representations use charset63 and BINARY. BYTEA, JSON and JSONB also
carry BLOB; TEXT carries BLOB and gains BINARY only for a binary text encoding.
No representation derives NUM, unsigned floating/decimal, ZEROFILL, ENUM, SET
or a MySQL TIMESTAMP policy. These are native representations, not inferred
MySQL declarations. BPCHAR keeps native padding through VAR_STRING; it does not
claim MySQL CHAR trimming semantics. TIMESTAMPTZ remains the encoder's explicit
UTC DATETIME representation, independent of native display timezone.

The explicit text collation must be a profile-declared utf8mb4, utf8mb3 or
binary encoding. Bounded utf8mb4 fields reserve four bytes per native character;
utf8mb3 reserves three and the encoder rejects supplementary codepoints. Binary
text output preserves native UTF-8 bytes, so its bound remains four bytes per
native character. A binary session's one-byte charset width must not truncate
that bound. No comparison/sorting equivalence is inferred from the chosen ID.
The mapper does not guess NAMEDATALEN or a server allocation limit: unbounded
strings use the largest representable four-byte MySQL field bound. This bound
is not a claim about the native object's declared length or a result allocation.

## Explicit failures

Only the nineteen existing scalar codecs have mappings. Unknown, array,
composite, interval and other unsupported types fail with their reported OID.
Malformed or unsupported modifiers fail with the OID and modifier. Fixed
scalars require modifier -1; bounded character modifiers include the native
four-byte header and a positive character count, with checked byte arithmetic.

NUMERIC requires precision1..65 and scale0..min(precision,30). Unconstrained
NUMERIC, negative scale, scale greater than precision and larger declarations
fail explicitly. The PostgreSQL modifier has a signed eleven-bit scale and five
reserved bits; treating its entire low half as unsigned scale is incorrect.
No precision is guessed, rounded or chosen from returned values. See
[PostgreSQL numeric declarations](https://www.postgresql.org/docs/18/datatype-numeric.html)
and [its modifier layout](https://github.com/postgres/postgres/blob/REL_18_STABLE/src/backend/utils/adt/numeric.c).
Signed DECIMAL length includes a sign and, for nonzero scale, a decimal point;
DECIMAL(2,2)'s width4 still admits the five-byte payload `-0.12`. Display widths
are not string byte limits. The shared encoder checks precision and scale.

Unspecified native temporal precision means six fractional digits. Explicit
precision0..6 is retained; other modifiers fail. A fractional suffix has one
decimal point plus the declared digits. Native infinity, out-of-range values
and values inconsistent with the representation remain per-row encoder errors.
These fields follow the
[MySQL column definition protocol](https://dev.mysql.com/doc/dev/mysql-server/8.4.11/page_protocol_com_query_response_text_resultset_column_definition.html).

## Execution boundary and cost

Use actual checked bound descriptions before Execute. A cached prepared
description, or a successful mapping, does not establish object identity or a
live dependency lease. Native domains can report base result types/modifiers;
the mapper uses those facts and does not erase domain policy in the catalog.
Expressions can lose a NUMERIC modifier even when a source column is bounded;
such an expression fails rather than borrowing its source column's precision.
Ordinary later DDL needs a new valid description; this component adds no cache
or invalidation algorithm and does not enable the pending table executor.

The mapping has no heap allocation, SQL request, lock, row scan or result
buffer. It looks up one static encoding profile and checks one scalar/modifier
per column. Admission retains identities separately, and output encoding reuses
the completed definitions without remapping each row. Measured throughput and
allocation counts remain release gates.

Required PostgreSQL17/18 fixtures exercise real prepared and bound descriptions,
all nineteen codecs through both output formats, NULLs, numeric rejection
before Bind/Execute, decimal endpoints, all temporal precisions, UTF-8/binary
widths, BPCHAR padding, supplementary-character errors without buffer mutation,
reported domain facts, expression typmod loss and new descriptions after later
DDL. They test the representation component, not MySQL SQL admission, catalog
coherence during execution, complete origin/key metadata, real drivers or release
compatibility. Those remain mandatory M2/M4/M5/M6 work.
