# Native prepared statement representations

`darmok-execute::NativeStatementUtc::new` checks the parameter types and result
column types of an already prepared PostgreSQL statement. It uses the same 19
builtin codecs as [native values](native-values.md) and
[native parameters](native-parameters.md). Unsupported types fail by their
actual OID and zero-based parameter/column position, even when the eventual
query would return no rows. Parameters are checked before result columns.

This is a representation check. It does not parse SQL, approve its semantics,
execute statements, infer MySQL metadata, or decide whether a command returns
rows. A description with zero columns and parameters is valid for this check;
it does not mean transaction commands, COPY, DDL or empty SQL are admitted.
SQL admission must separately identify the command and allowed effects.

The wrapper borrows the driver's statement, preserving exact output labels,
types, typmods and optional relation/attribute origins. It adds no database
query, SQL rewrite, catalog lookup, allocation or statement clone. `bind`
checks the dense backend value count and returns borrowed parameter wrappers
through an exact-size iterator. Values are encoded once by the driver using
`NativeParamUtc`; this check does not implement MySQL input coercion.

[Native output](native-results.md) separately binds this checked description to
immutable frontend column definitions and validates/encodes text or binary row
payloads. It does not generate those definitions or supply SQL admission.

`decode_row` compares the row's driver description to the checked statement,
including count, names, types, typmods and origins, before ordinary native
decoding. Incompatible descriptions are rejected; separate statements with
identical description fields are accepted.
Rows sharing the immutable prepared description bypass the per-field comparison;
other descriptions are compared in full. These are **driver metadata**, not a fresh backend portal description:
tokio-postgres attaches cached statement metadata to rows. The comparison does
not establish physical database identity, schema freshness, or a valid catalog
snapshot. Reusable catalog-dependent plans remain disabled pending their own
coherence and execution contract.

Domain results follow PostgreSQL's reported base type and typmod. A supported
base result does not erase declared domain facts in
[the native catalog](native-catalog.md). Domain parameters, arrays, enums and
other unsupported parameter/result representations are rejected. Type names
never select a codec.

Type admission also does not guarantee representability of every value.
Unconstrained NUMERIC, infinities, non-finite floats and temporal values outside
the documented range can still fail decoding. Typmods are retained as native
facts; they are not silently replaced by invented MySQL precision or scale.
A future executor must keep writes rollback-capable through row decoding and
wire encoding, then finish the backend operation before reporting success.
This component has no transaction ownership or recovery API.

## Development evidence

Required PostgreSQL 17/18 fixtures cover all builtin parameter/result types
with an empty result; unsupported empty results and write RETURNING shapes;
domain result flattening and unsupported domain/enum parameters; binding arity,
typed writes and outputs; independent description mismatch cases; and value
errors despite an admitted type. Every fixture requires a real disposable
backend and fails when the dependency is missing.

The backend's [extended-query contract](https://www.postgresql.org/docs/18/protocol-flow.html#PROTOCOL-FLOW-EXT-QUERY)
provides ParameterDescription and RowDescription at statement Describe, before
Bind/Execute. The retained fields are exposed by
[tokio-postgres Column](https://docs.rs/tokio-postgres/0.7.18/tokio_postgres/struct.Column.html).
Neither source nor these fixtures certify MySQL compatibility or full execution.
