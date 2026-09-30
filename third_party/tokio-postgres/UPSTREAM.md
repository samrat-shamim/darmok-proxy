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
They do not provide semantic admission, connection ownership or MySQL behavior.
The generated `src/error/sqlstate.rs` map declaration has one trailing space
removed, with no change to its constants.

The [backend completion contract](../../docs/backend-completion.md) defines the
new API and its ordinary functional verification. No other upstream source is
modified.
