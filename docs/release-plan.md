# Darmok 0.1.0 execution plan

Status: **in progress; no release candidate exists**.

Current scope adjustment: security-related work is deferred at the user's
request. Authentication, TLS, authorization testing, security review and
adversarial/resource stress verification remain pending. Continue generic
extraction, SQL functionality, ordinary correctness tests, builds and
documentation. Deferred gates cannot count as release certification.

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
| M1 | Independent workspace, vendored parser, package names, licensing and CI | Fresh checkout builds and tests without another repository or private dependency; upstream notices preserved | Complete |
| M2 | Extract the generic translation and execution engine | All application assumptions and allocation/capture/control-plane dependencies removed; both minimal end-to-end examples pass | In progress; component extraction only |
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
- [Foundation PR #1](https://github.com/duotronic-ai/darmok-proxy/pull/1) passed
  fresh Linux/macOS CI and independent review: 1,499 all-feature tests/doctests,
  1,492 default-feature tests/doctests, seven explicitly ignored upstream
  formatting cases, strict Clippy, formatting, repository boundaries, and
  minimal/std-only/visitor-only feature builds. It is merged.
- [Generic libraries PR #3](https://github.com/duotronic-ai/darmok-proxy/pull/3)
  is merged. The types, wire protocol and session libraries passed independent
  review, Linux/macOS CI, 1,595 all-feature tests/doctests, 1,588 default-feature
  tests/doctests, seven ignored upstream formatting cases, and one mandatory
  schema fixture on each PostgreSQL version. Domain allocation/trust state is
  removed and schemas preserve their exact names. This is component evidence,
  not certification of authentication or session semantics.
- The [compiler draft](https://github.com/duotronic-ai/darmok-proxy/pull/4) carries
  explicit AST binding layouts. It is not merged or certified for execution.
- [Native values PR #5](https://github.com/duotronic-ai/darmok-proxy/pull/5) is
  merged. Linux/macOS CI passed 1,595 all-feature tests/doctests and 1,588 default
  tests/doctests with seven ignored upstream formatting cases. Six required
  fixtures passed on each PostgreSQL version. Ordinary correctness review found
  no remaining decoder defect. Global temporal sentinels and implicit
  fallback-to-text coercion variants are removed. The [value contract](native-values.md)
  records the supported representations.
- [Native parameters PR #6](https://github.com/duotronic-ai/darmok-proxy/pull/6)
  is merged. Linux/macOS CI passed 1,595 all-feature tests/doctests and 1,588
  default tests/doctests with seven ignored upstream formatting cases. Twelve
  required fixtures passed on each PostgreSQL version: one schema, six value
  and five parameter fixtures. Ordinary functional review is complete. Input
  parameters use backend types with explicit UTC temporal semantics; conditions
  are recorded in [the parameter contract](native-parameters.md). MySQL coercion
  and session integration remain separate gates.
- Row decoding follows the types reported by PostgreSQL. A native domain
  fixture documents that the backend reports domain base types/typmods; declared
  domain policy requires catalog metadata and is not supplied by row decoding.
- Fresh native relation/type reads are implemented in `darmok-catalog`, with
  their boundaries in [the catalog contract](native-catalog.md). The required
  suite adds seven ordinary catalog fixtures per backend. Reusable catalog plans,
  execution validity and MySQL wire metadata remain pending; reading native
  definitions does not certify these gates.
- Native prepared descriptions are checked before execution by
  `darmok-execute::NativeStatementUtc`.
  [PR #8](https://github.com/duotronic-ai/darmok-proxy/pull/8) is merged as
  `479f9e82a07f2736bf64ce95fefb3bea8ca06c0e`. Linux/macOS CI passed 1,595
  all-feature and 1,588 default-feature tests/doctests with seven ignored
  upstream cases; 25 required fixtures passed on each PostgreSQL version.
  Independent ordinary functional review is complete. The
  [statement contract](native-statements.md)
  separates type/arity checks from semantic admission, value ranges and rollback.
  Six required ordinary fixtures per backend exercise empty results, unsupported
  write outputs, domain types, borrowed bindings and description/value errors.
  This component does not supply a runnable executor or catalog coherence.
- Inherited runtime semantic risks are tracked in
  [release blocker #2](https://github.com/duotronic-ai/darmok-proxy/issues/2).
- No SQL compatibility, authentication, routing, or performance gate has passed.
- The [statement execution contract](native-execution.md) defines the next
  functional engine boundary: owned rollback scopes through decoding/encoding,
  explicit completion, and uncertainty disposition. It is a design, not an
  implemented executor; semantic admission and live catalog validity remain
  prerequisites. This does not complete M2 or any release gate.
- A vendored connector addition exposes [backend completion events](backend-completion.md)
  with exact command tags, backend errors and request-boundary transaction states.
  Eight required ordinary fixtures per backend cover control/commit outcomes,
  prepared rows/errors, local binding failures and queued request association.
  Admitted rollback through output validation, semantic admission and catalog
  validity remain pending; this is connector component evidence, not an executor gate.
- The [native control checker](native-controls.md) requires fixed internal
  control tags and final states, distinguishing COMMIT-as-ROLLBACK, backend
  commit errors and incomplete savepoint recovery from successful completion.
  Nine required ordinary fixtures per backend exercise outcomes and table
  effects. It consumes existing requests and owns no connection or rollback
  scope; M2 and the execution/admission/catalog gates remain incomplete.
- The [exclusive native backend owner](native-backend.md) retains a fresh client
  and its driver privately, observes idle before returning, and owns explicit
  controls plus borrowed transaction/savepoint scopes. Dropped pending controls
  or scopes cannot restore Ready; unconfirmed cleanup requires disposal.
  Nine private database unit fixtures are required separately on PostgreSQL
  17/18, without exposing a public raw SQL execution escape. This implements
  the control ownership boundary, not semantic admission, catalog validity,
  rollback through output validation, wire execution or the M2 end-to-end gate.
- Scope recovery requires an explicit Statement or Transaction choice. The
  whole-transaction choice confirms one ROLLBACK/idle even inside a statement
  savepoint, removing earlier writes and all savepoints. Four additional required
  private fixtures per PostgreSQL version cover complete discard, native errors
  followed by valid work, an already removed internal savepoint, and abandoned
  recovery futures. The owner suite now has thirteen fixtures. Frontend error
  classification, state changes and MySQL lock-retention equivalence remain
  pending; this is a native control mechanism, not a transaction compatibility gate.
- Built-in typed-query completion is implemented for fixed internal queries.
  Parse/Bind/Describe/Execute/Sync share one request, retaining SQL errors,
  exact tags and final state without hidden type lookups. Eight further ordinary
  fixtures are required on each PostgreSQL version. Descriptions are observed
  after submission, so this is not pre-execution admission. Custom result OIDs
  fail explicitly without a completion receipt. The
  [completion contract](backend-completion.md) records the restriction and costs;
  the owner still exposes no raw SQL or Client. Catalog validity, admitted rows
  and M2 remain pending.
- The [frontend recovery contract](transaction-recovery.md) distinguishes
  statement-local, whole-transaction, implicit-commit and uncertain outcomes.
  Four ordinary stock lock cases on each of MySQL 8.4.11 and PostgreSQL
  17.11/18.6 completed twelve groups with seventy-two normal SQL-client exits
  and confirmed removal of all three disposable databases. Existing-engine MySQL
  savepoint recovery retains later row locks which native PostgreSQL releases;
  a savepoint before the first table operation is a separately observed variant.
  Failed initial expectations and runner setup attempts remain frozen. The
  required lock mechanism is tracked in
  [blocker #15](https://github.com/duotronic-ai/darmok-proxy/issues/15). These are
  stock reference observations, not proxy equivalence, driver, performance or
  release certification. M2/M4 and the installation choice remain pending.
- The [stock transaction reference fixture](transaction-reference.md) distributes
  the thirteen earlier cases as declared data with a standalone Python/Docker
  observer. Its required MySQL 8.4 CI job checks exact statement/occurrence error
  attribution, complete CLI stderr, unique request completion, final effects and
  disposable database absence, and preserves its receipts. This makes reference
  evidence reproducible from the checkout; it does not execute the proxy or
  complete differential, native-schema, transaction or M2/M4 gates.

- [Explicit native transaction characteristics](native-transactions.md) require
  isolation and access choices when beginning an owned transaction. Separate
  transaction/savepoint scope methods reject the wrong boundary before
  submission; savepoints retain their parent modes. Two further native-owner
  fixtures are required on each PostgreSQL version for all six combinations,
  ambient default override/preservation and savepoint recovery/release. The
  owner suite now has fifteen fixtures. This is a native control component;
  frontend session mapping, MySQL snapshot/lock equivalence, catalog validity,
  admitted execution and M2/M4 remain pending.

- The [stock transaction-characteristic fixture](mysql-transaction-characteristics.md)
  adds eleven ordinary MySQL 8.4 cases using an explicit corpus and the existing
  observer. Actual current-thread transaction events and session defaults cover
  all eight MySQL isolation/access combinations, session/next/active lifetimes,
  named updates, implicit/autocommit consumption, chaining and both active-transaction
  SET errors with retained writes. The corpus uses seventeen SQL clients, two
  read-only inspections and three metadata processes, with a separate required
  CI step/artifact. The thirteen recovery cases are unchanged. These are stock
  labels/boundaries/data observations; frontend policy, native snapshot/lock
  equivalence, read-only write enforcement, wire status and M2/M4 gates remain
  pending. Security-related work stays deferred.

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
