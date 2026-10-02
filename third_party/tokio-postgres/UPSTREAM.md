# Vendored tokio-postgres

The library payload comes from the crates.io `tokio-postgres` 0.7.18 archive,
SHA-256 `a528f7d280f6d5b9cd149635c8705b0dd049754bc67d81d31fa25169a93809d3`.
The archive identifies Rust-Postgres revision
`f1cb6ec0d5766b136cbd68f3010d64142a5daa66`. Upstream MIT and Apache-2.0 license
terms and attribution remain unchanged; one redundant final blank line in
`LICENSE-MIT` is removed. `Cargo.toml.orig` records the upstream manifest; builds use
the registry-normalized `Cargo.toml` with unused upstream test/benchmark targets
and development dependencies removed. Upstream tests/benches and its separate
lockfile are omitted; Darmok's required PostgreSQL fixtures exercise this copy.

This dependency is excluded from Darmok workspace membership. Only the
dependency features requested by the consuming crates are enabled, rather than
every optional upstream value conversion. All consumers use the same vendored
package; the root lockfile pins its registry dependencies.

Local modifications add completion events in `src/completion.rs`, expose those
events from `src/client.rs` and `src/lib.rs`, and separate raw backend responses
from upstream error conversion in `src/client.rs`. Existing query helpers keep
their upstream behavior. The added event APIs preserve command tags, backend
errors and ReadyForQuery transaction states without adding protocol exchanges.
The command and built-in typed-query streams also record whether any item has
been yielded, so a complete-request consumer can reject a consumed stream.
`query_typed_builtin_events` adds one-shot typed internal-query observation
without extra protocol exchanges or hidden result-type lookups. It observes
only built-in result OIDs; unknown result OIDs terminate explicitly after SQL
submission. Prepared and typed streams share their row/completion state machine.
`src/query.rs` adds a modified-file notice and makes its existing typed parameter
encoder crate-visible for reuse, with no change to that encoder's behavior.
They do not provide semantic admission, connection ownership or MySQL behavior.
The generated `src/error/sqlstate.rs` map declaration has one trailing space
removed, with no change to its constants.

The [backend completion contract](../../docs/backend-completion.md) defines the
new APIs and their ordinary functional verification.

`src/portal_completion.rs` additionally observes Bind, portal Describe and Sync
before queuing any Execute. Its built-in descriptions use fresh native labels,
origins and typmods, with no result-type lookup SQL. `src/client.rs` and
`src/lib.rs` expose the described portal and failure receipts; `src/statement.rs`
adds private prepared-handle identity comparison. `src/completion.rs` executes
described portals with their actual metadata, retains their cleanup handle,
and validates result formats and NoData/row distinctions. Existing prepared
query helpers retain their cached-description behavior. These additions do not
provide semantic admission, dependency validity or connection ownership.
