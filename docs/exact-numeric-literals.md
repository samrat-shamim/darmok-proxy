# Exact numeric literals

The local SELECT result controller is being extended from integers to exact
MySQL number tokens. Scientific-notation literals remain approximate numbers
and are explicitly unsupported until their formatting, metadata and diagnostic
contract is implemented. The full SQL/type/release objective remains open.

Exact tokens contain decimal digits, with an optional decimal point. Their
original lexical spelling belongs to `SourceSelect`, including when the
optional BigDecimal AST representation normalizes it. Integer values through
the signed/unsigned 64-bit ranges keep LONGLONG declarations. Larger integer
literals and tokens containing a decimal point use NEWDECIMAL. The admitted
decimal range has at most 65 significant integer digits plus fractional digits;
fractional zeros count toward scale. Literal scale may exceed 30: the MySQL
DECIMAL column and native NUMERIC decoder limit is a separate contract.

The plan keeps borrowed integer/fractional digit slices, numeric family,
signedness, display scale and declared width separately. Row text removes
redundant integer zeros, supplies the zero before a leading decimal point,
preserves fractional zeros and suppresses negative zero. A trailing point with
scale zero yields integer-looking text while retaining NEWDECIMAL metadata.
No floating-point conversion, rounding or native request is used.

Decimal literal width follows the independently observed MySQL declaration:
one retained leading integer zero when present, remaining integer digits,
fractional digits, a separator when scale is nonzero, and a sign position.
Integer literal width retains its original digit count and its signed or
unsigned declaration. Values and declared widths are never inferred from one
another. CLI's computed NUM flag is not a server packet flag.

Nested unary plus/minus expressions over exact numbers are planned in order.
Negating an unsigned integer may produce a signed integer or DECIMAL; negating
the signed minimum again produces DECIMAL. Ordinary negation retains width,
adding a sign position for unsigned operands. Integer-to-DECIMAL promotion uses
the operand's declared precision, distinguishing literal and derived values.
Plain literals and unary plus retain the original numeric-token name; a
negation uses complete projection spelling unless an explicit alias overrides
it. Unsupported numeric forms fail before effects, without approximate values
or rendered-AST names.

This extends the existing immutable source parse, private admission plan and
command/output ownership. Digits remain borrowed until one text-value buffer
is allocated; column names keep their existing allocation path. Numeric planning
walks each expression once and scans its token digits a constant number of
times; each subsequent sign operation uses its stored zero classification.
There is no new runtime dependency, cache, lock or PostgreSQL round trip.
Actual end-to-end allocation/latency measurements remain release gates.

The required reference corpus declares three cases and twenty-four columns.
Its observer compares pinned MySQL CLI metadata and exact row strings, with
zero warnings required in each case. Metadata and row commands use distinct
stock connections and preserve separate transcripts. The public command
fixtures consume the same corpus, decode actual result packets in both EOF
modes, and check unchanged pending/active transaction state. The existing
workspace `serde_json` package is used only as a development dependency to read
the fixture, with no new runtime package. These are component checks, not a
live stock/proxy driver differential or full numeric-expression coverage.

References are MySQL's [numeric literals](https://dev.mysql.com/doc/refman/8.4/en/number-literals.html),
[DECIMAL characteristics](https://dev.mysql.com/doc/refman/8.4/en/precision-math-decimal-characteristics.html)
and pinned MySQL 8.4.11 source at
`99960bf74fa919347e4f4e3ca47672f333d6e91f`. Source excerpts and exploratory
stock transcripts remain outside the Apache distribution. Implementation,
required current-revision tests and independent review are not yet certified.
