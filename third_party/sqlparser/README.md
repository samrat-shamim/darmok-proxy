# Vendored SQL parser

This is a self-contained Apache-2.0 derivative of
[Apache DataFusion sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs),
including MySQL SQL modes, construct metadata and PostgreSQL AST emission.

The package is named `darmok-sqlparser`; its Rust library is named `sqlparser`.
Keeping the Rust library name matches its derive macros, examples, doctests
and integration tests. Consumers use the workspace dependency `sqlparser`.
The derive package is also vendored. Neither package is published separately
until registry packaging and dependencies have been verified.

Both packages are workspace members so their tests run in normal CI. Grammar
coverage here does not imply Darmok execution support for any dialect.

The opt-in `source::parse_mysql_source` extension binds an immutable MySQL AST
to original projection and numeric-token source ranges from the same parse.
It rejects executable comments before parsing: their server-version and source
contracts are not implemented. A tokenizer option retains those comment tokens
for explicit admission; ordinary AST parsing keeps its separate upstream path.

The `visitor` feature enables the AST visitor, PostgreSQL emitter and semantic
metadata APIs. The grammar and MySQL mode APIs can also build without it.
Both the core parser and visitor extensions support `no_std` with `alloc`.
CI checks these minimal feature combinations as well as default/all features.

See `LICENSE.TXT` and `NOTICE` for licensing and retained attribution.
