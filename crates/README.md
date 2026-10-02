# Generic libraries

- `darmok-types`: typed values, MySQL/PostgreSQL constants, errors, plans and
  exact PostgreSQL schema identifiers.
- `darmok-protocol`: packet encoding/decoding and authentication primitives.
- `darmok-session`: SQL session state and bounded prepared-statement storage.
- `darmok-execute`: explicit native PostgreSQL scalar decoding and typed
  parameter encoding, including exact decimal scale, native year-one dates and
  the 24:00:00 time endpoint, with prepared description/binding arity checks and
  exclusive native connection ownership for transaction controls.
- `darmok-catalog`: fresh native relation and declared-type facts by
  database-local OID, including domain identities and raw column attributes.

These libraries are an extraction foundation. They do not form a runnable
proxy yet. The [release plan](../docs/release-plan.md) remains authoritative for
end-to-end verification and release gates.

Schema selection is explicit, preserves case and quotes a complete identifier.
`PgSchema` assumes PostgreSQL's standard 63-byte identifier limit; startup must
verify the server limit and validate reserved schemas and role grants before
accepting connections. Literal `$user` and `pg_temp` names are explicitly
rejected because PostgreSQL interprets them specially in `search_path` even
when quoted.

Prepared-statement registries require explicit limits for statement count and
total SQL bytes. Invalid registrations and parameter-type updates preserve
existing state. IDs never wrap onto a live statement; exhausting the ID space
requires connection reset. Debug output excludes SQL text. Database/schema
binding and authorization at prepare/execute are part of the server integration
gate, not a property supplied by storage alone.

`COM_STMT_EXECUTE` keeps its binding payload opaque until the session supplies
the registered parameter count and previously bound types. The decoder does
linear work, rejects malformed/trailing bytes, and leaves text bytes for the
session's negotiated charset. Query attributes and prepared cursors are
explicitly unsupported. Binary rows use column metadata for numeric widths
and signedness, and restore the output buffer on encoding failure. Debug
output for commands, bindings and handshake responses excludes credentials,
query text, parameters and client attributes.

Required PostgreSQL 17/18 tests verify schema quoting, the target of unqualified
object creation, built-in function precedence and temporary-table shadowing.
Missing database configuration fails the test rather than skipping it.

The native value decoder preserves PostgreSQL text and bytes as stored, with no
global sentinel or escape interpretation. Its supported scalar representations
are documented in [native values](../docs/native-values.md). Result metadata,
session timezone integration and statement execution remain separate work.

Native parameters borrow their values and use the backend's expected types.
They reject implicit string/number/date conversions and lossy real-number
conversion. The [parameter contract](../docs/native-parameters.md) records the
accepted representations and native PostgreSQL input semantics. MySQL coercion
rules and frontend prepared-statement metadata require separate integration.

The [native statement check](../docs/native-statements.md) rejects unsupported
parameter/result types before execution, including empty result sets. Borrowed
bindings retain backend arity; decoding checks driver description consistency.
This does not provide semantic admission, catalog freshness or rollback.

The [native backend owner](../docs/native-backend.md) keeps the client and driver
private and owns explicit controls and borrowed transaction/savepoint scopes.
Recovery takes an explicit statement-local or whole-transaction choice and
requires the corresponding native completion. It exposes no arbitrary SQL
method. Frontend recovery policy and lock semantics remain separate work.
Admitted row execution through decoding and encoding is still pending; a control
receipt does not certify a statement.

The [native catalog reader](../docs/native-catalog.md) batches relation IDs and
preserves quoted names, domain declarations and type modifiers. It does not
infer projected nullability, MySQL collations or logical database aliases, and
it does not supply a reusable catalog generation or execution validity lease.

Session-variable, charset/collation, engine, temporal and transaction behavior
still requires semantic certification. Inherited metadata must not become an
advertised support claim without that work. The known issues are tracked in
[release blocker #2](https://github.com/samrat-shamim/darmok-proxy/issues/2).
