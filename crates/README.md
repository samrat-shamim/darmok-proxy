# Generic libraries

- `darmok-types`: typed values, MySQL/PostgreSQL constants, errors, plans and
  exact PostgreSQL schema identifiers.
- `darmok-protocol`: packet encoding/decoding and authentication primitives.
- `darmok-session`: SQL session state and bounded prepared-statement storage.
- `darmok-execute`: explicit native PostgreSQL scalar decoding, including exact
  decimal scale, native year-one dates and the 24:00:00 time endpoint.

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

Session-variable, charset/collation, engine, temporal and transaction behavior
still requires semantic certification. Inherited metadata must not become an
advertised support claim without that work. The known issues are tracked in
[release blocker #2](https://github.com/duotronic-ai/darmok-proxy/issues/2).
