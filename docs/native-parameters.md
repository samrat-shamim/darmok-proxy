# Native backend parameters

`darmok-execute::NativeParamUtc` borrows a typed proxy value and encodes it
against the parameter type reported by the prepared PostgreSQL statement.
It is an execution component. It does not implement MySQL assignment, string
coercion, SQL modes or frontend prepared-statement metadata.

## Declared conversions

| PostgreSQL target | Accepted proxy values | Conditions |
| --- | --- | --- |
| boolean | Bool | Exact boolean |
| smallint, integer, bigint | Int, UInt | Must fit the target signed range |
| oid | Int, UInt | Must fit the unsigned 32-bit range |
| real | Float | Finite and exactly representable as f32 |
| double precision | Float | Finite f64 |
| numeric | Decimal, Int, UInt | Plain finite decimal; at most 65 significant integer plus fractional digits and 30 fractional digits |
| text, varchar, bpchar, name | String | Exact UTF-8; native PostgreSQL text cannot contain NUL |
| bytea | Bytes | Exact bytes |
| json, jsonb | String | Native PostgreSQL JSON input semantics; the backend validates the JSON and normalizes JSONB |
| date | Date | Valid Gregorian date, years 1 through 9999 |
| time | Time | Nonnegative 00:00:00 through 24:00:00 with exact microseconds |
| timestamp | DateTime | Valid date/time, years 1 through 9999 and microseconds below 1,000,000 |
| timestamptz | DateTime | Same validity conditions, explicitly interpreted as UTC |

NULL is supported only for the listed target types. Unsupported types and
type/value mismatches return errors. Integers do not silently become booleans,
strings do not silently become numbers or dates, and zero dates have no sentinel
conversion. Float-to-real rounding requires an explicit future coercion policy;
this component accepts only exact conversion.

The connector's checked encoder rejects unsupported target types with its
`WrongType` error before invoking the component encoder. Supported-type
conversion failures carry `NativeParamError`; callers must also handle
connector and backend errors.

Decimal text has an optional sign, integer digits and an optional fractional
part. Exponents, whitespace and special values are outside this representation.
Leading integer zeros and a plus sign are normalized without changing value or
fractional scale. PostgreSQL handles native numeric typmod rounding and errors
in the SQL operation, just as it does for other native parameters.

## Format and cost

Numeric and JSON parameters use the PostgreSQL extended protocol's text
parameter format. Decimal input is already exact text, so this avoids a second
proxy-side base-10000 conversion while preserving its scale. The backend's
native input functions remain authoritative. Other parameters use their typed
binary representations. Mixed parameter formats require no extra SQL or
protocol round trip.

The connector's [ToSql contract](https://docs.rs/tokio-postgres/0.7.18/tokio_postgres/types/trait.ToSql.html)
defines per-parameter text/binary format selection. PostgreSQL's
[native numeric rules](https://www.postgresql.org/docs/18/datatype-numeric.html)
remain authoritative for typmod assignment, and its
[character types](https://www.postgresql.org/docs/18/datatype-character.html)
define the native text representation.

The wrapper borrows the value; it does not clone it. Text, bytes and decimal
payloads are copied directly into the connector's message buffer. Integer
formatting and temporal conversion do not allocate owned intermediate values.
Performance measurements remain a separate gate; these are structural cost
properties, not benchmark results.

Required PostgreSQL 17/18 fixtures compare typed parameters with independent
native SQL values, retain decimal scale and temporal endpoints, test supported
NULL types, and confirm ordinary type/range failures do not execute a statement.
Frontend session timezone handling, SQL coercion and full proxy execution remain
separate integration gates. Security-related work is deferred at the user's
request.
