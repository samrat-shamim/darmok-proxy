# Darmok 0.1.0 execution plan

Status: **in progress; no release candidate exists**.

## Decisions

- Name: Darmok; repository: `darmok-proxy`; executable: `darmok`.
- License: Apache-2.0. First release: experimental 0.1.0.
- Equal support goals: MySQL applications and MySQL clients over native
  PostgreSQL schemas.
- MySQL 8.4 is the semantic reference. PostgreSQL 17 and 18 are the initial
  backend test matrix.
- One configured PostgreSQL endpoint; multiple explicitly authorized logical
  databases, each naming one physical database and one schema.
- Separate frontend credentials, backend role credentials, and route grants.
- Explicit per-database initialization in the reserved `darmok` schema.
- No application-specific integrations or external control-plane dependency.

## Milestones and mandatory exit gates

| ID | Work | Exit gate | Status |
| --- | --- | --- | --- |
| M0 | Pin and inventory the starting implementation; establish baseline; write contracts | Reproducible source inventory and test results, documented inherited failures, architecture and scope contracts | Complete; baseline has one recorded failure |
| M1 | Independent workspace, vendored parser, package names, licensing and CI | Fresh checkout builds and tests without another repository or private dependency; upstream notices preserved | In progress |
| M2 | Extract the generic translation and execution engine | All application assumptions and allocation/capture/control-plane dependencies removed; both minimal end-to-end examples pass | Pending |
| M3 | Configuration, initialization, authentication, routing, diagnostics | Idempotent init, explicit mismatch errors, tested TLS/login/authorization/reset behavior, least-privilege runtime | Pending |
| M4 | SQL, types, metadata, transaction/session/prepared/cache correctness | Every advertised construct has executable evidence; external DDL and native schema tests pass | Pending |
| M5 | Drivers, adversarial inputs, concurrency and performance | Required driver and differential matrices, fuzz smoke, resource limits, isolation and cancellation tests, reproducible benchmarks pass | Pending |
| M6 | Documentation, artifacts, independent review and publication | Exact candidate passes all gates; fresh artifact examples pass; review resolved; PR merged; signed/checksummed release artifacts published | Pending |

Dependencies: M0 → M1 → M2 → M3 → M4 → M5 → M6. Work may overlap when it
does not assume an unmet gate, but release certification may not.

## Ordered work

1. Preserve a pinned source snapshot and an inventory outside the distribution.
   Run the original tests without modifying that snapshot. Record failures and
   tests that do not actually execute. Historical compatibility percentages are
   not a baseline.
2. Import the parser and generic libraries with their tests. Replace repository
   paths with local workspace dependencies. Keep the parser and its derive
   macros testable as first-class workspace members. Build in CI from a clean
   checkout with no adjacent repositories.
3. Remove domain-specific crates, state, plan flags, helper SQL, fixtures,
   environment variables, metrics, documentation and build dependencies.
   Preserve the AST compiler, parameterization, wire encoding and streaming
   where their semantics are sound. Resolve findings at their actual boundary.
4. Implement the architecture contract: validated route graph; frontend and
   backend credentials; initialization with explicit version checks; diagnostic
   commands; connection and pool lifecycle. Verify denied paths as thoroughly
   as successful ones.
5. Audit each translation and execution path against the compatibility policy.
   Fix or explicitly reject silent approximations. Make native PostgreSQL
   metadata authoritative. Decide external-DDL cache coherence with a written
   correctness argument, concurrency tests, and measurements before enabling
   reusable catalog-dependent plans.
6. Exercise all layers together with MySQL 8.4 differential tests and native
   PostgreSQL fixtures on both backend versions. Add PHP, Python, Node, Go and
   Java driver cases; malformed protocol and SQL fuzzing; pool/session isolation;
   concurrent DDL/DML; cancelled streams; connection loss; overload and shutdown.
7. Produce a self-contained container, Linux amd64/arm64 executables, checksums,
   dependency/license inventory and examples. Cargo publication is conditional
   on all required packages being independently publishable and verified.
   Review the complete release independently, merge through a PR, verify the
   final commit and publish the experimental tag and artifacts.

## Evidence rules

Every gate record must include:

- Exact Git revision and dependency lockfile.
- Toolchain, operating system/architecture, database and driver versions.
- Exact commands, exit codes, executed/failed/ignored test counts and logs.
- For performance: workload, dataset, concurrency, configuration, warm-up,
  repetitions, throughput, p50/p95/p99 latency, memory and CPU.
- Known limitations and the tests demonstrating their explicit errors.
- Independent review findings and their resolution when required.

Missing infrastructure is a failed required check, not a successful skipped
check. Any source change invalidates release evidence for the old candidate.
Only affected development checks need repetition during implementation; the
complete release gate runs once on the final candidate.

## Current evidence

- The repository has an Apache-2.0 license and a feature branch for foundation
  work. It remains private while the extraction is incomplete.
- The initial source and parser revisions are pinned in an external work log.
- The original workspace baseline completed on Rust 1.96.0/macOS arm64 with
  Docker PostgreSQL tests: **1,425 passed, 1 failed, 2 ignored**. No database skip
  notices were emitted. The failure is in the application-specific change
  capture integration path excluded from Darmok; it does not certify or block
  a Darmok feature. Full source pins, commands, failure details and hashed logs
  are preserved in the external extraction work log.
- The vendored parser's own tests exposed an incorrect lexical round-trip
  assertion for equivalent `SHOW INDEX` spellings. The assertion now verifies
  all three aliases against the canonical AST, including the WHERE filter.
  Feature-combination and strict-toolchain checks are being completed before
  the foundation PR merges.
- No SQL compatibility, authentication, routing, or performance gate has passed.

See [the extraction inventory](extraction-inventory.md) for component decisions
and inherited semantic risks that require verification.

## Release gate checklist

- [ ] Complete source, build, test and documentation independence audit.
- [ ] Both use cases work from released artifacts and documented examples.
- [ ] Fresh init and repeated init are safe; conflicts/mismatches fail clearly.
- [ ] Frontend authentication and backend role/database/schema isolation hold.
- [ ] Supported constructs have current differential/native/wire evidence.
- [ ] Unsupported constructs fail explicitly before unintended effects.
- [ ] Native schemas and native data remain intact.
- [ ] Prepared plans and catalog caches remain correct under external DDL.
- [ ] Driver, fuzz, concurrency, cancellation, load and resource-limit checks pass.
- [ ] Artifact architecture, checksums, provenance and licenses are verified.
- [ ] Independent review is complete with no unresolved release blockers.
- [ ] Final merged commit passes all gates and the experimental release is published.
