# Contributing

Darmok is preparing its first experimental release. Read the
[architecture contract](docs/architecture.md),
[compatibility policy](docs/compatibility.md), and
[release plan](docs/release-plan.md) before extending behavior.

Install the Rust toolchain pinned in `rust-toolchain.toml`, then run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --exclude darmok-postgres-tests --all-features --locked
python3 scripts/check_repository.py
```

The workspace includes the parser's integration tests and doctests. Database
tests run separately against a disposable PostgreSQL database with schema and
function creation privileges:

```sh
export DARMOK_TEST_DATABASE_URL='postgres://postgres:darmok-test@localhost:5432/darmok_test'
cargo test -p darmok-postgres-tests --locked
```

CI requires this suite on PostgreSQL 17 and 18. It fails if the database is
missing or unavailable. Test objects are created inside a rolled-back
transaction. Driver and artifact checks will be added as their implementations
arrive; no test suite may claim those gates before exercising them.

Use a feature branch and a pull request. Explain the observable behavior,
failure modes and verification. Include regression cases for NULLs, metadata,
authorization and transactional effects when they are relevant. Nontrivial
changes need an independent second review.

Contributions are licensed under Apache-2.0. Preserve upstream notices and
identify changes to vendored files.
