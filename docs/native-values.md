# Native backend value decoding

`darmok-execute::decode_native_row_utc` converts binary PostgreSQL rows into
typed proxy values. It is an execution component, not a runnable proxy or an
end-to-end SQL compatibility claim. PostgreSQL 17 and 18 are its test matrix.

## Explicit scalar representations

| PostgreSQL type | Proxy value | Conditions |
| --- | --- | --- |
| boolean | Bool | Exact boolean |
| smallint, integer, bigint | Int | Exact signed width/range |
| oid | UInt | Exact unsigned 32-bit value |
| real, double precision | Float | Finite values; real expands exactly to f64 |
| numeric | Decimal | Finite, at most 65 decimal digits and 30 fractional digits |
| text, varchar, bpchar, name | String | UTF-8 payload, including native private-use characters and padding |
| bytea | Bytes | Exact bytes, including NUL and empty values |
| json | String | Native JSON text, preserving its whitespace and duplicate keys |
| jsonb | String | PostgreSQL's JSONB version-one textual payload |
| date | Date | Years 1 through 9999; year one remains year one |
| time | Time | 00:00:00 through 24:00:00, with exact microseconds |
| timestamp | DateTime | Years 1 through 9999, with exact microseconds |
| timestamptz | DateTime | UTC representation, years 1 through 9999 |

Unknown types, domains, arrays, enums, intervals and timetz have no implicit
text conversion. An unsupported type fails even when its value is NULL. Finite
native values outside the supported representation also fail. Infinity, NaN
and dates outside the year range are not silently clamped or wrapped.

NUMERIC is decoded directly from PostgreSQL's binary base-10000 digits. It
never passes through floating point. The decoder retains the backend display
scale and removes only the final group's zero padding; it does not round or
truncate meaningful digits. The representation limits follow the initial
[MySQL decimal range](https://dev.mysql.com/doc/refman/8.4/en/fixed-point-types.html).
PostgreSQL allows a larger range and additional special values, which require
explicit future representation decisions.

Native text has no private-use escape convention. Native temporal values have
no zero-date sentinel convention. The inherited global year-one conversion and
fallback-to-text coercion variants have been removed. Any future MySQL zero-date
support needs a separate, column-specific representation and executable evidence.

PostgreSQL TIME permits the 24:00:00 endpoint. The decoder represents it as a
one-day duration rather than wrapping it to midnight. TIMESTAMPTZ decoding is
explicitly UTC, independent of the PostgreSQL connection's display timezone.
Session timezone behavior must be integrated and verified before execution
support is advertised. See PostgreSQL's [date/time types](https://www.postgresql.org/docs/18/datatype-datetime.html).

## Cost and evidence

Decoding performs one linear pass over the columns. Numeric, text and binary
payloads allocate one owned value each; scalar and temporal values require no
per-cell allocation. Numeric output is bounded by the declared precision/scale.
No compatibility SQL, catalog query or additional protocol round trip is needed
to decode a row. Streaming and metadata integration will need their own
measurements before release.

Required native tests compare exact decimal output against PostgreSQL's own
text representation, retain native year-one dates/timestamps and private-use
text, distinguish empty values from NULL, preserve the 24-hour endpoint and
check binary MySQL encoding using independently declared fixture metadata.
They also exercise ordinary unsupported native type/range errors.

These tests do not certify result-column metadata, MySQL coercion rules,
session timezones, parameter encoding or full statement execution. Those remain
separate release gates. Security-related work remains deferred at the user's
request; this component's tests cover ordinary data conversion.
