# Contributing

Darmok is preparing its first experimental release. Read the
[architecture contract](docs/architecture.md),
[compatibility policy](docs/compatibility.md), and
[release plan](docs/release-plan.md) before extending behavior.

Install the Rust toolchain pinned in `rust-toolchain.toml`, then run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
python3 scripts/check_repository.py
```

The workspace includes the parser's integration tests and doctests. Database,
driver and artifact checks will be added as their implementations arrive; no
test suite may claim those gates before exercising them.

Use a feature branch and a pull request. Explain the observable behavior,
failure modes and verification. Include regression cases for NULLs, metadata,
authorization and transactional effects when they are relevant. Nontrivial
changes need an independent second review.

Contributions are licensed under Apache-2.0. Preserve upstream notices and
identify changes to vendored files.
