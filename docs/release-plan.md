# Darmok 0.1.0 execution plan

Status: **in progress; no release candidate exists**.

Current scope adjustment: security-related work is deferred at the user's
request. Authentication, TLS, authorization testing, security review and
adversarial/resource stress verification remain pending. Continue generic
extraction, SQL functionality, ordinary correctness tests, builds and
documentation. Deferred gates cannot count as release certification.

## Decisions

- Name: Darmok; repository: [darmok-proxy](https://github.com/samrat-shamim/darmok-proxy);
  executable: `darmok`.
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

- The Apache-2.0 source repository is public under
  [samrat-shamim](https://github.com/samrat-shamim/darmok-proxy).
  Extraction is incomplete and no runnable proxy or release candidate exists.
- The initial source and parser revisions are pinned in an external work log.
- The original workspace baseline completed on Rust 1.96.0/macOS arm64 with
  Docker PostgreSQL tests: **1,425 passed, 1 failed, 2 ignored**. No database skip
  notices were emitted. The failure is in the application-specific change
  capture integration path excluded from Darmok; it does not certify or block
  a Darmok feature. Full source pins, commands, failure details and hashed logs
  are preserved in the external extraction work log.
- [Foundation PR #1](https://github.com/samrat-shamim/darmok-proxy/pull/1) passed
  fresh Linux/macOS CI and independent review: 1,499 all-feature tests/doctests,
  1,492 default-feature tests/doctests, seven explicitly ignored upstream
  formatting cases, strict Clippy, formatting, repository boundaries, and
  minimal/std-only/visitor-only feature builds. It is merged.
- [Generic libraries PR #3](https://github.com/samrat-shamim/darmok-proxy/pull/3)
  is merged. The types, wire protocol and session libraries passed independent
  review, Linux/macOS CI, 1,595 all-feature tests/doctests, 1,588 default-feature
  tests/doctests, seven ignored upstream formatting cases, and one mandatory
  schema fixture on each PostgreSQL version. Domain allocation/trust state is
  removed and schemas preserve their exact names. This is component evidence,
  not certification of authentication or session semantics.
- The [compiler draft](https://github.com/samrat-shamim/darmok-proxy/pull/4) carries
  explicit AST binding layouts. It is not merged or certified for execution.
- [Native values PR #5](https://github.com/samrat-shamim/darmok-proxy/pull/5) is
  merged. Linux/macOS CI passed 1,595 all-feature tests/doctests and 1,588 default
  tests/doctests with seven ignored upstream formatting cases. Six required
  fixtures passed on each PostgreSQL version. Ordinary correctness review found
  no remaining decoder defect. Global temporal sentinels and implicit
  fallback-to-text coercion variants are removed. The [value contract](native-values.md)
  records the supported representations.
- [Native parameters PR #6](https://github.com/samrat-shamim/darmok-proxy/pull/6)
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
  [PR #8](https://github.com/samrat-shamim/darmok-proxy/pull/8) is merged as
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
  [release blocker #2](https://github.com/samrat-shamim/darmok-proxy/issues/2).
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
  [blocker #15](https://github.com/samrat-shamim/darmok-proxy/issues/15). These are
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

- The [frontend transaction-settings contract](frontend-transactions.md) defines
  the planned replacement of duplicated isolation strings and scope-erasing
  variable storage. It maps the eleven stock cases to separate defaults,
  pending choices and active settings, preserving assignment form and frontend
  boundaries independently of native BEGIN. Failed-start consumption,
  additional command/read forms and integrated outcome evidence remain
  unverified. This is a design prerequisite; no controller, session support,
  admitted execution or M2/M4 gate is implemented by it.

- Three further stock characteristic cases distinguish reverse partial-update
  order, clearing explicitly overridden next access in both directions using
  distinct defaults, and @@SESSION assignments between/inside transactions.
  The corpus now has fourteen cases and forty-one event captures, using twenty
  SQL clients, two read-only inspections and three metadata processes. Its
  original eleven cases, setup/reset SQL, observer and required CI workflow are
  unchanged, as is the thirteen-case recovery corpus. These finite observations
  refine the controller's settings requirements; no controller, native snapshot/
  lock equivalence, catalog validity or M2/M4 gate is implemented by them.

- The [typed transaction setting/staging component](frontend-transactions.md)
  replaces the extracted public string/boolean/lifecycle duplication. It owns
  session defaults, independent next overrides, active choices and autocommit,
  and derives canonical transaction-variable reads and translation identity.
  Borrowed stages retain ordered known boundaries and refuse settled reads or
  reuse after an unconfirmed submitted command. Receipt/output methods assert
  a trusted caller contract; no native operation or encoder is verified by them.
  Fourteen manual Rust traces compare forty-one five-field logical settings
  projections against the unchanged stock characteristic corpus, alongside
  ordinary phase/identity tests. Run them with
  `cargo test -p darmok-session --locked`; exact revisions, commands and logs
  are retained with development/CI receipts. SQL classification, integrated
  execution/recovery, wire metadata, native isolation mapping, snapshots, locks,
  catalog validity, performance and M2/M4 remain pending. No authentication,
  TLS, authorization, security or adversarial/resource work is added.

- The [canonical variable/SQL-mode model](session-variable-values.md) removes
  the remaining generic value map, public text/mode fields and duplicate warning
  counter. Canonical reads, parser projection and translation identity use one
  typed full mode set; other setting paths are read-only or unimplemented.
  The standalone parser's string helpers that discarded mode information are
  removed.
  A third required stock corpus declares fourteen ordinary mode-value cases
  using the unchanged observer, with a separate CI step and receipt artifact;
  both existing transaction corpora remain unchanged. Manual Rust projections,
  ordinary model/parser/identity tests and compile-fail examples run with
  `cargo test -p darmok-session --locked`. Exact candidate/check/review/CI
  records are retained outside the distribution. SQL assignment/scoping,
  warnings, native effects, actual mode query/write semantics, reset/encoding,
  catalog validity, performance and M2/M4 remain pending. Security-related work
  remains deferred.

- The [SQL session input component](session-sql-input.md) fixes dropped keyword
  scope in the transaction-setting AST and preserves direct, characteristics and
  snapshot forms distinctly. Direct SESSION/next settings produce typed intents;
  borrowed variable assignment/read views retain keyword context and @@ forms
  without guessing values or compound targets. Ordinary parser/model input
  tests run with `cargo test -p darmok-session --locked`. Exact candidate,
  command, review and CI records are retained outside the distribution. The
  stock corpora remain unchanged. Coercion, scope-specific values, native
  effects, syntax admission, output integration, performance and M2/M4 remain
  pending. Security-related work remains deferred.

- The [stock system-variable name fixture](mysql-system-variable-names.md)
  declares nineteen ordinary MySQL 8.4 cases for backticks, scoped quoted-text reads,
  ANSI_QUOTES, scoped and bare assignments, whitespace, case-insensitive names
  and quoted columns that resemble variables. It uses the unchanged observer
  with twenty-five SQL clients, two inspections and three metadata processes, and
  a separate required CI step/artifact. Existing corpora remain unchanged.
  Parser correction is tracked in [issue #25](https://github.com/samrat-shamim/darmok-proxy/issues/25).
  The initial draft completed eight cases, then rejected an immediate
  single-quoted read; its failure, unrun cases and cleanup remain preserved.
  Exact source, command, review and CI records are retained outside the
  distribution. This fixture does not execute the proxy or complete syntax
  admission, native effects, catalog validity or M2/M4 gates. Security-related
  work remains deferred.

- The [system-variable syntax component](mysql-system-variable-syntax.md)
  separates MySQL sigils from name tokens and uses explicit read/SET target
  nodes. Session classification borrows those facts without sigil decoding.
  Focused local checks pass ten existing SQL-input groups and three new groups,
  including all nineteen declared name forms, columns, spans and visitors.
  Local workspace tests pass with all features (1,647 passed) and defaults
  (1,640 passed), each with 22 existing ignored tests. Strict workspace Clippy,
  formatting, repository boundaries and minimal/std/visitor parser checks pass.
  Exact candidate, command, environment and evidence records are retained
  outside the distribution. Independent review found no ordinary functional
  defect; its 94 focused tests and minimal/std/visitor builds pass. GitHub
  Actions is disabled for the personal account, so fresh CI and merging
  remain pending.
  This does not implement session mode changes, value coercion, native effects, SQL
  admission, output, catalog coherence, performance or M2/M4.

- The [stock SET semantics fixture](mysql-set-semantics.md) declares nineteen
  ordinary cases for keyword/per-name scope, pre-update expression reads,
  DEFAULT timing, selected coercion and validation/update failure effects.
  The unchanged observer and four earlier corpora remain intact. An initial
  eighteen-case draft completed fourteen cases, then failed on expected JSON
  integer/boolean types; its real failure and cleanup are preserved. The
  corrected corpus passes all nineteen cases on pinned MySQL 8.4.11 Linux arm64
  at `f7bf9fa0dc1394226cf019fa48bd88cb8f28722b`: the outer observer and all thirty
  child commands exit 0, seven errors have exact attribution, twenty-five SQL
  markers match, and final database absence is 0. Schema, diff and repository
  boundary checks pass. Exact source, command, environment and evidence hashes
  are retained outside the distribution. Rust source is unchanged; old Rust
  results are not fresh checks for this leaf. A separate required CI step is
  prepared. Independent review found that the LOCAL case reused its reset
  access value and did not distinguish scope inheritance. Its followup changes
  the bare value and requires both defaults and both transactions to retain the
  update. All nineteen followup native cases pass. Independent followup review
  confirms resolution with seven passing offline groups and no new finding;
  the original ten-group review and finding remain preserved. GitHub Actions
  is disabled for the personal account, so fresh CI and merging remain pending.
  This does not implement a SET controller, native effects, output or any M2/M4
  or release gate. Security-related work remains deferred.

- The [selected SET execution component](set-controller.md) now joins parsed
  inputs, one authoritative session state, exclusive native ownership and
  encoded/sent output. It handles four variables, SESSION/LOCAL/next scope,
  pre-update ordinary reads/checks and ordered DEFAULT semantic checks. A native
  autocommit commit is confirmed before frontend end/setting publication.
  Independent review of `20742af7bea77712d9c872ecc2bc99bd186e4e33` found C1:
  an unsupported DEFAULT could follow earlier effects or a native commit.
  `d5626740bd583af7f284d236485241ac1cfc8391` admits DEFAULT support first and
  retains the supported late 1568 check at update time. Independent passive
  followup review closes C1 at that exact revision with no new concrete finding;
  the original finding remains preserved against its original head. The
  reviewer did not run Cargo or SQL; its source audit is separate from the
  author's current dynamic receipts.
  At that corrected revision, ten new ordinary controller groups pass on
  PostgreSQL 17.11 and 18.6, each with exit 0, including real native commit/data
  and rollback effects and unchanged data after rejected unsupported DEFAULT.
  Starts are trusted fixture setup; no native isolation mapping or full
  nineteen-script proxy execution is certified. Exact required commands are:
  `cargo test -p darmok-execute --lib --locked native_backend::tests::set_controller -- --ignored`,
  `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`,
  `cargo test --workspace --exclude darmok-postgres-tests --all-features --locked`,
  `cargo test --workspace --exclude darmok-postgres-tests --locked`,
  `cargo fmt --all --check`, `python3 scripts/check_repository.py`, and
  `git diff --check 9f787745ca2678a89d6fea0b749caa0dccd397d6..HEAD`.
  All exit 0. Workspace counts are 1,650/1,643 passing with 32 ignored;
  the new ignored database groups are explicitly required in both native-owner
  CI jobs. Current native runs select the ten new groups, rather than silently
  treating ignored groups as passing. Local Rust uses the pinned toolchain and
  the required `RUSTUP_HOME`/shared `CARGO_TARGET_DIR`; actual revision, tree,
  commands, environment, fixture port, exits and hashed streams are in
  `logs/set-controller-support-*.json` outside the distribution. Draft compile/
  lint failures and a terminated overlapping default-suite attempt remain
  separate receipts; the accepted default suite runs serially and exits 0.
  Actual MySQL frontend dispatch, starts, catalog leases, row admission,
  prepared execution, general coercion/warnings, session tracking, real drivers
  and measured end-to-end performance remain pending. Account Actions is still
  disabled, so this work cannot merge or complete M2/M4 or a release gate.
  Security-related work and the compiler draft remain excluded.

- The [selected COM_QUERY controller](query-controller.md) admits original
  decoded query SQL using current session modes and dispatches one SET to its
  private evaluator. There is no public constructed-AST execution entry point.
  MySQL SET accepts `=`/`:=` and direct characteristics in either order; other
  dialects retain their grammar. Known unsupported statements/batches return
  SQL errors before effects. The outcome guard now also names command-level
  SQL diagnostics. Ten native SET groups use the real query source path and
  four new groups declare source/mode/batch postconditions; four new stock
  observations declare ordinary operator/characteristic expectations.
  At `042b374a89121b025c701d30fdd94d967edfba5a`, fourteen required native
  groups pass on PostgreSQL 17.11 and 18.6. Exact commands are the selected
  native command listed above, both workspace commands, strict Clippy,
  formatting, repository boundary checks and
  `git diff --check b8ef2af0662308b0d5f4243fd8e0b53782c9e538..HEAD`.
  All eight exit 0. Workspace counts are 1,651/1,644 passing with 36 ignored;
  ignored groups are not counted as passing. Both mandatory Cargo environment
  variables use the task toolchain/shared target. Actual command, clean source,
  tree, environment, fixture port, exit and hashed streams are preserved in
  `logs/query-dispatch-support-*.json` outside the distribution. The serial
  verification wrapper also exits 0. The initial compile/name collision,
  inherited foreign-dialect fixture failure and unconfigured boundary-helper
  attempt remain separate failed draft receipts, never relabeled as passing.
  At clean `29ab912cc852ab5f8076cc7ad7231a4af09864a4`, stock MySQL 8.4.11
  source observations pass all four cases, fifteen processes and cleanup,
  each with exit 0. The observer and corpus are unchanged by the correction.
  This certifies stock expectations rather than full proxy differential behavior.
  Independent passive review found no additional runtime defect and confirmed
  R1, the original classifier fixture parsing PostgreSQL SNAPSHOT as MySQL.
  The one-file correction uses PostgreSqlDialect for foreign forms; followup
  review closes R1 at `042b374` with no new finding. The original finding and
  failed workspace observation remain tied to `29ab912`. Reviewers verified
  saved author receipts offline; they did not independently run Cargo or SQL.
  A subsequent documentation leaf leaves every nondocument byte identical to
  the verified/reviewed code revision. This does not establish a runnable
  frontend loop, row results, real-driver compatibility or M2/M4 completion.
  Fresh account CI and merging remain blocked. Security and compiler work remain
  excluded.

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

## Local SELECT command progress

The selected COM_QUERY path now includes local SELECT result planning and
protocol output. [The result contract](query-results.md) separates implemented
literal/variable semantics from unsupported clauses, numeric representations,
diagnostic reads and name-normalization outcomes. Parser-owned source facts,
declared metadata, both EOF modes and command diagnostics are integrated into
the existing public dispatcher. Table/native execution and the executable,
real-driver, catalog and release gates remain open. Executable comments are
explicitly unsupported before SELECT/SET effects; their version and source
contract is tracked in
[issue #32](https://github.com/samrat-shamim/darmok-proxy/issues/32).

A new mandatory stock CLI reference step observes five column-declaration
cases. It preserves CLI transcripts and compares its declared fields; it does
not assert raw server flags or a proxy exchange. Existing reference observers
and corpora retain their source bytes. PostgreSQL native-owner CI includes
four ordinary SELECT fixture groups alongside the existing SET fixtures.

At clean `029609697d270b44adf982df6e5df0de767559e2`, tree
`597d99d79d1b8861fc56f03fa285baf56b18f1b1`, fourteen serial local checks and
their outer wrapper exit 0. Exact Rust/check commands are:

- `cargo test -p darmok-execute --lib --locked native_backend::tests::query_results -- --ignored`
- `cargo test -p darmok-execute --lib --locked native_backend::tests::set_controller -- --ignored`
- `cargo test --workspace --exclude darmok-postgres-tests --all-features --locked`
- `cargo test --workspace --exclude darmok-postgres-tests --locked`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `cargo check -p darmok-sqlparser --no-default-features --features visitor --locked`
- `cargo fmt --all -- --check`
- `python3 scripts/check_repository.py`
- `git diff --check 4e1910ac5d4143b69083e7c44a1ca6e6da093c82..HEAD`

Both native commands run separately on PostgreSQL 17.11 and 18.6, with four
SELECT and fourteen SET groups per backend, zero failures and zero ignored
within those selected runs. Workspace counts are 1,658/1,651 passing, each
with 40 ignored; ignored groups are not counted as passing. The native fixture
ports are 32769/32768. Every Cargo process uses the task's pinned toolchain
and the mandatory `RUSTUP_HOME` and shared `CARGO_TARGET_DIR` from its receipt.
PostgreSQL version requests and the stock observer are the remaining three
checks. The exact observer is
`python3 tests/reference/observe_mysql_query_results.py --container darmok-mysql-reference-84 --image mysql:8.4@sha256:6ea90827b1100f8f2ae306a539f86d2c264a26ed435a2a9f75551dd5c3aeb242 --corpus tests/reference/mysql_query_results.json --evidence-dir <new external directory>`.
It observes MySQL 8.4.11 with five cases and eight actual child exits 0;
the exclusive actual directory is `logs/query-results-stock-columns-0296096`
outside the distribution. These are CLI declaration expectations, not raw stock
wire flags, a real driver exchange or a proxy differential.

Original `1fab01c157901f2a7754a1a8bfc097871cc01941` has a static independent
executable-comment provenance finding F1 and strict Clippy exit 101; its serial
wrapper exits 1 and does not execute its later checks. The intact test module
is relocated to EOF in `fd7b48a`, with runtime source otherwise unchanged.
The correction at `0296096` retains executable comment tokens in the same
tokenization and rejects them before AST/SET admission. Independent passive
delta review closes F1 and the lint failure at that exact revision, with no new
finding. Reviewers checked saved author receipts and hashes without running
Cargo, SQL or product APIs. Original reports, failures, worktrees and manifests
remain frozen; no failed attempt is relabeled as passing.

Exact clean source, tree, commands, environments, fixture ports, actual exits
and hashed streams are in `logs/query-results-admission-*.json` outside the
distribution. `logs/query-results-admission-code-evidence.json` seals those
receipts, stock observations and additive review against the code revision.
The original optional stock wire-client attempt failed before SQL and is
excluded from accepted wire evidence. A subsequent documentation-only leaf
preserves all nondocument bytes and has fresh boundary/diff checks; the code
receipts remain bound to `0296096`. This is local component verification and
does not complete M2/M4 or a release gate.

No CI restart, account change, main push, merge or release is authorized by
this implementation milestone. The recorded account Actions condition remains
a merge/release blocker; authorized functional work can still progress.

## Exact numeric literal progress

The local SELECT controller now admits exact decimal tokens, large
integer promotion and ordered unary signs. [The contract](exact-numeric-literals.md)
keeps source labels, row text and declared metadata separate, including scale
above 30 for literals, trailing zeros and negative signed-operand promotion.
Scientific notation and general numeric expressions remain explicit unsupported
outcomes.
The private evaluator uses borrowed digits until final row allocation and adds
no native request or runtime dependency. This is progress toward the full
SQL/type objective, not a replacement of its remaining gates.

Four pinned stock cases declare thirty-four columns, exact string rows and
zero warnings; the new observer and existing column observer share only their
CLI field decoder. The old five-case column corpus remains unchanged and must
be observed freshly when the shared decoder changes. Two further mandatory
native-owner SELECT groups consume the new corpus across both EOF forms and
check pending/active settings plus unsupported approximate-number rejection.
At clean `1cf9377178cb88bf327f0409b71affbd28b985ab`, tree
`7761e54626a1759e0c6ec177e58045840d33ea78`, eighteen local component/reference
checks and the serial verification wrapper exit 0. Exact Rust/check commands
are:

- `cargo test -p darmok-execute --lib --all-features --locked select_controller::tests`
- `cargo test -p darmok-execute --lib --locked native_backend::tests::query_results -- --ignored`
- `cargo test -p darmok-execute --lib --locked native_backend::tests::set_controller -- --ignored`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `cargo test --workspace --exclude darmok-postgres-tests --all-features --locked`
- `cargo test --workspace --exclude darmok-postgres-tests --locked`
- `cargo check -p darmok-sqlparser --no-default-features --locked`
- `cargo check -p darmok-sqlparser --no-default-features --features std --locked`
- `cargo check -p darmok-sqlparser --no-default-features --features visitor --locked`
- `cargo fmt --all --check`
- `python3 -B scripts/check_repository.py`
- `git diff --check 48c9dcd91e2828f30b43abbb3b9b544f8dd8f150..HEAD`

The selected native commands run separately on PostgreSQL 17.11 and 18.6,
ports 32769/32768, with six SELECT and fourteen SET groups per backend, zero
failures and zero ignored in those selected runs. The focused unit command
passes nine groups with all features. Workspace suites pass 1,663/1,656 tests,
each with 42 ignored; ignored database groups are not counted as passing.
Every Cargo invocation uses
`RUSTUP_HOME=/Users/shamim/Projects/duotronic/.darmok-work/rustup` and
`CARGO_TARGET_DIR=/Users/shamim/Projects/duotronic/.darmok-work/native-values/target`.
These are local Linux arm64 database fixtures and the task's pinned Rust
toolchain, rather than a claim about a hosted runner or released artifact.

The other four checks are two PostgreSQL version requests and both stock CLI
observers. Each observer runs with
`--container darmok-mysql-reference-84 --image mysql:8.4@sha256:6ea90827b1100f8f2ae306a539f86d2c264a26ed435a2a9f75551dd5c3aeb242`.
The existing column command is
`python3 -B tests/reference/observe_mysql_query_results.py --corpus tests/reference/mysql_query_results.json --evidence-dir <new external directory>`;
the numeric command is
`python3 -B tests/reference/observe_mysql_exact_numbers.py --corpus tests/reference/mysql_exact_numbers.json --evidence-dir <new external directory>`.
Saved exact argv includes all of these arguments. On MySQL 8.4.11, the old
corpus passes five cases/eight child exits 0; the numeric corpus passes four
cases/thirty-four columns/eleven child exits 0 and zero warnings. Values stay
strings in the corpus and comparisons; no floating-point JSON conversion is
used. These are stock CLI declarations/rows plus separate proxy-packet
fixtures, rather than a live stock/proxy driver differential.

Original `032715a25a0d341da02d76a95ddb0b8b8075513c` has a fixture compile
failure, E0277/exit 101 before tests run. The one-line checked conversion at
`5ed799b0b349c5027ba7b0ae92038c2ee87b081c` closes that compile failure.
Independent review also identifies F1: double negation of ordinary negative
signed operands incorrectly retains LONGLONG. Author stock observations
confirm NEWDECIMAL for `-(-1)` and related forms, while `-(-0)` keeps LONGLONG.
The correction at `1cf9377` models this declared-family rule separately from
mathematical value range and adds the fourth corpus case plus its unit
regression. Original failures, observations and the independent review remain
tied to their own revisions; earlier passing tests do not erase F1.

Exact source, tree, argv, environment, fixture port, actual exits and stream
hashes are saved in `logs/exact-numeric-1cf9377-*.json` and
`logs/exact-numeric-corrected-unit.json` outside the distribution, alongside
the exclusive stock transcript directories. The independent corrective
review closes F1 at `1cf9377` with no new functional finding. Its offline Git,
source and saved-receipt audit is separate from the author's dynamic runs;
the reviewer runs no Cargo or SQL. Reports, actual invocation receipts and
additive facts remain outside the distribution. The compact
`logs/exact-numeric-code-evidence.json` binds the corrected code, eighteen
checks plus outer wrapper, stock transcripts and independent reports.
A following documentation-only leaf preserves every nondocument byte, with
fresh boundary/diff checks and a separate Git tree proof. Code/test receipts
retain their original `1cf9377` source identity. No M2/M4, driver, measured
performance, fresh CI, merge or release gate is closed by these additions.
Security-related work and the separate compiler draft remain excluded.


## Original-source transaction-controller component

The [source transaction contract](query-transactions.md) connects public decoded
COM_QUERY controls to shared canonical planning, checked native controls and
one response guard. The parser preserves absent/negative completion clauses and
uses MySQL start/completion grammar. Completion policy is canonical and explicit;
idle completion, active/idle chaining and commit-before-replacement have distinct
ordered boundaries. Every proposed native start is admitted before prior work
can commit. READ UNCOMMITTED, consistent snapshots and release completion fail
explicitly before effects. Table execution, the owning frontend loop, RELEASE,
full transaction equivalence and M2/M4 remain pending.

Twenty-six new ordinary stock cases, two completion-type field declarations, ten
required native controller groups and two nested guard tests declare the finite
verification scope. Required CI consumes the new corpora and native fixture
module in default and BigDecimal builds. Active replacements retain their
preceding pair with explicit access overriding only access; changed defaults
apply after a later nonchained completion. Original SET numeric tokens supply
consistent whole-command admission across those builds. Fresh results and the
independent review below bind this component to its committed code revision;
declarations and dirty exploratory checks close no gates.


### Verified code and finite scope

At clean `bd31f7b345200ff0d7e797c4c6f6fb6fd4c08074`, tree
`82c74ba8b776b8c750dcd2098ec5eaa8750c184d`, all twenty-one planned local
checks and the containing matrix command exit 0. The separate focused reserved
RELEASE roundtrip also passes. Rust/Cargo are 1.96.0; the fixtures are PostgreSQL
17.11 on port32769, PostgreSQL18.6 on port32768 and pinned MySQL8.4.11. Every
Cargo invocation and Cargo-spawning helper uses both task overrides:

```sh
RUSTUP_HOME=/Users/shamim/Projects/duotronic/.darmok-work/rustup
CARGO_TARGET_DIR=/Users/shamim/Projects/duotronic/.darmok-work/native-values/target
```

| Check at the code revision | Actual result |
| --- | --- |
| Transaction module, default and `sqlparser/bigdecimal`, each PostgreSQL version | Four runs, ten groups passed each; no failed or ignored selected group |
| Existing SET and SELECT modules, each PostgreSQL version | Four runs, fourteen SET and six SELECT groups passed per version |
| Workspace tests/doctests, all features | 1,669 passed, zero failed, 52 ignored; required selected native groups ran separately |
| Workspace tests/doctests, default features | 1,662 passed, zero failed, 52 ignored |
| Strict workspace/all-targets/all-features and vendored connector Clippy | Both exit0 with `-D warnings` |
| Parser without defaults, with std, and with visitor | Three checks exit0 |
| Formatting, repository boundaries, diff and tool/fixture versions | Four checks exit0 |
| Stock transaction observer, new corpus | 26 completed cases, 37 actual child exits0, 19 exact error/code/state/physical-line observations, cleanup remaining0 |
| Stock completion metadata observer | One case, two field descriptions, four actual child exits0 |

The workspace commands exclude `darmok-postgres-tests`; their success is not a
new database integration result for that package. Native commands use
`cargo test -p darmok-execute --lib --locked native_backend::tests::<module> -- --ignored`;
transaction feature runs additionally use `--features sqlparser/bigdecimal`.
Workspace commands are `cargo test --workspace --exclude darmok-postgres-tests
--locked`, with `--all-features` for the all-feature run. Exact argv, clean
revision/tree, environment, fixture ports, real exits and raw stream hashes are
in `.darmok-work/logs/query-transactions-bd31f7b-*.json`. The sequential matrix
and its audit verify eighteen nonoverlapping Cargo/helper intervals and current
corpus/observer identities. Both observers retain their original implementation;
other reference corpora retain their Git entries. CLI metadata observations are
separate from the native fixtures' encoded library output and from real drivers.

The original `b301ec3` review found active replacement selecting changed defaults
(F1), numeric SET admission using normalized AST Display (F2), and a strict
Clippy must-use failure (V1). Their old failures remain frozen. The corrected
shared planner retains active characteristics through replacement; original
numeric source admission applies to all five selected SET variables before any
commit, in both numeric feature builds. Three native receipts are explicitly
bound after checked completion. Fresh reference runs also exposed reserved
RELEASE SET spelling; the selected reservation now belongs to the MySQL prefix
hook. The earlier corpus, missing-import, cross-dialect and test-roundtrip
failures remain earlier-revision evidence, never relabelled as final passes.

Independent passive correction review at the exact code revision closes
F1/F2/V1 and identifies no new runtime blocker. It independently audits author
commands and raw hashes, rather than rerunning Cargo or SQL. Its remaining D1
request-cost wording is corrected in this documentation leaf: numeric source
provenance adds no native request, while a supported SET may still need its
existing autocommit commit. Review report/facts/seal live under
`.darmok-work/logs/review-query-transactions-bd31f7b-*`; the seal SHA256 is
`052b498de61a3548117951c3a3bce1634facf339a10dc3af67f2e6ca3cd15e8d`.
The original review and incomplete intermediate capture remain distinct.

`.darmok-work/logs/query-transactions-code-evidence.json` binds 288 saved evidence
files, including local receipts, current stock observations, original failures
and review seals. Its SHA256 is
`86fcd9b395d0b39a82163d9a1ba4d902b333554fee005e3edbbea197124352c2`.
These artifacts remain outside the distribution. A documentation leaf must
preserve every non-document Git entry from the verified code revision; these
execution receipts retain their original code identity.

This selected library component does not close M2/M4, runnable frontend/table
execution, RELEASE ownership, failed-start recovery, full reserved grammar,
catalog validity, native/created-schema workloads, real drivers, SQL/snapshot/
lock equivalence or measured performance. Required hosted CI remains unavailable
under the separately recorded personal-account Actions condition; local results
justify neither merge nor release. Security and other excluded work remain
outside this component. The full goal remains active.


## Owning TCP command-phase progress

The [connection contract](frontend-connection.md) makes public SQL execution
consume a real frontend socket, canonical session and exclusive native owner.
The former borrowed public query function is private, with no transition shim.
Selected commands run serially; RELEASE confirms native boundaries and flushes
OK before closure, while normal QUIT/EOF records checked rollback separately
from local driver disposal. The report never returns a reusable backend.

Sixteen stock lifecycle cases and eight required native TCP fixture groups
cover independently resolved chain/release policy, NO RELEASE continuation,
discarded prefetched commands, both EOF forms, current modes, repeated PING
condition counts, and real post-disconnect table effects/removal. Coalesced
commands and split client writes are observed; receive chunking is unasserted.
Expected stock CLI exits1 remain actual exits1. The unchanged transaction
observer is reused only for identity/setup/effect/cleanup requests; the new
entry point and helper retain separate source identities. Existing reference
corpora and observers retain their bytes.

### Verified correction and evidence

The verified code is clean `085236c11077a71e95c29963eaa30084d30e6ce6`, tree
`c7766571be56c641ffaeb5955ffc6a1b822ebca4`. All twenty-five required local checks
are satisfied at that exact revision. The initial matrix records twenty-three
passes, a stock metadata failure before any lifecycle case, and an unrun PING
observer. Its actual exit remains1: the author omitted the existing disposable
fixture client environment. A separate two-command continuation supplies that
existing setting and exits0; its lifecycle and PING children both exit0. No
product or credential configuration changed, and successful Rust checks were
not rerun to conceal the observer setup failure.

Rust/Cargo are1.96.0; fixtures are PostgreSQL17.11 on port32769,
PostgreSQL18.6 on port32768 and pinned MySQL8.4.11. Every Cargo invocation and
Cargo-spawning helper uses both task overrides:

```sh
RUSTUP_HOME=/Users/shamim/Projects/duotronic/.darmok-work/rustup
CARGO_TARGET_DIR=/Users/shamim/Projects/duotronic/.darmok-work/native-values/target
```

| Check at the corrected code | Actual result |
| --- | --- |
| Owning frontend module, default and BigDecimal, each PostgreSQL version | Four runs, eight groups passed each; zero failed or ignored selected group |
| Prior transaction module, default and BigDecimal, each PostgreSQL version | Four runs, ten groups passed each |
| Existing SET and SELECT modules, each PostgreSQL version | Four runs, fourteen SET and six SELECT groups passed per version |
| Workspace tests/doctests, all features / default | 1,669 / 1,662 passed; zero failed, 60 ignored; required native groups ran separately |
| Strict workspace/all-targets/all-features and connector Clippy | Both exit0 with `-D warnings` |
| Three minimal parser builds, format, repository boundary, diff and versions | Seven checks exit0 |
| Stock lifecycle continuation | 16 cases, 43 real CLI children: 29 actual0, 14 expected actual1; acknowledgments/effects/closure verified, disposable database remaining0 |
| Stock PING continuation, PyMySQL1.1.2 | Four normal cases, eight actual OK payloads; clean0, warning/error1, successful SET reset0; retained conditions checked separately |

Workspace commands exclude `darmok-postgres-tests`; these are not fresh database
integration results for that package. Native commands use
`cargo test -p darmok-execute --lib --locked native_backend::tests::<module> -- --ignored`,
with `--features sqlparser/bigdecimal` in the declared numeric feature runs.
Workspace commands use `cargo test --workspace --exclude darmok-postgres-tests --locked`,
plus `--all-features` for that run. Exact argv, source/tree/status, fixture ports,
mandatory environment, real exits and raw stream hashes are retained under
`.darmok-work/logs/frontend-loop-085236c-*`. The audit verifies twenty-two
nonoverlapping Cargo/helper intervals and binds the failed original matrix and
successful continuation separately. Its facts SHA256 is
`08bffc2911a355532b50f1aae81ecf00e5d992c417d267370d557b9d6bac0540`.

Original review at `2cb2667` found PING's hardcoded zero count (F1) and
split-write wording that overstated TCP receive segmentation (D1). Stock
standard-client raw packets after invalid sql_mode error1231 are
`00000002000100`; the old idle proxy would encode `00000002000000`.
The private canonical statement counter now records all condition levels and
feeds terminal packet warnings. It is separate from the retained list; current
admitted SQL replaces both, while PING preserves both. No diagnostic-preserving
SQL form or warning-producing row execution is invented.

A regression-only revision `8ef6db5` records actual101 at the warning bytes:
`[0,0,0,3,32,0,0]` versus `[0,0,0,3,32,1,0]`. The prior `e7b4e7a` attempt's
actual101 is retained separately: its invalid BEGIN READ ONLY precondition
failed with1064 before PING. The valid fixture uses START TRANSACTION READ ONLY.
The corrected eight-group runs verify repeated PING1, successful SET/SELECT
clearing0, readonly status and both EOF capabilities. Earlier client-dependency
and primary-capture failures remain actual failures; PHP query properties are
not treated as PING packet proof. GPL source references remain external to the
Apache distribution.

The independent passive correction review closes F1/D1 with no new functional
finding. It independently audits the source, valid negative and saved raw
receipts; it runs no Cargo, SQL or network. Reports/facts/seals live under
`.darmok-work/logs/review-frontend-loop-085236c-*`; its seal SHA256 is
`d0d6057a581d66c6445c10fb896b06d56b49fe9d70a79ad7da1fb49e252256bb`. The original review remains sealed separately.
`.darmok-work/logs/frontend-loop-code-evidence.json` binds the original/corrected
checks, failures, ordinary stock observations and independent reviews; its
SHA256 is `c97d488aef7558bbd68dba1990476a0240ed03ad230b735f7ba46e55398e80ae`. These artifacts remain outside the distribution.
A documentation leaf must preserve every nondocument Git entry from the
verified code; execution receipts retain the original corrected code identity.

This is a post-handshake command-phase library. Executable/listener/handshake
integration, public table SQL, catalog validity, native/created-schema workloads,
full authenticated real-driver/diagnostic/snapshot/lock equivalence, measured
performance, bounded shutdown and M2/M4 remain incomplete. Security work and
compiler PR4 source remain excluded. Required hosted CI remains unavailable
under the separately recorded account condition; local component verification
justifies neither merge nor release. The full goal remains active.

## Native relation name discovery

The catalog component now accepts literal native schema/relation pairs through
`read_native_named_relations`. One typed statement resolves all names and reads
columns and domain ancestors in the same native snapshot. The immutable result
preserves request order and repeats while deduplicating relation/type maps.
Missing pairs fail the complete read with the first missing input index; empty
requests issue no SQL. Exact names retain case, dots, quotes, spaces and Unicode
spelling. Temporary objects use their actual catalog namespace, without
search-path or logical-alias resolution. The contract and costs are recorded in
[native catalog facts](native-catalog.md).

Verified code is `76a9da18c4d2466d04871b4abfc8db968a6c5943`, tree
`28aa911323ed50aed45874b7f675e7851c8f2676`, on `feat/native-named-catalog`,
stacked on the reviewed command-loop branch. Each clean-source receipt records
the exact argv, working directory, revision/tree, environment, fixture port,
actual exit and stream hashes. Every Cargo/Cargo-spawning command used
`RUSTUP_HOME=/Users/shamim/Projects/duotronic/.darmok-work/rustup` and
`CARGO_TARGET_DIR=/Users/shamim/Projects/duotronic/.darmok-work/native-values/target`;
all twelve child check intervals are nonoverlapping. Rust/Cargo are 1.96.0;
the disposable PostgreSQL fixtures are 17.11 on port32769 and 18.6 on port32768,
with UTF8 encoding and the standard 63-byte identifier limit.

| Final code check | Actual result |
| --- | --- |
| `cargo test -p darmok-postgres-tests --test native_catalog --test native_named_catalog --locked`, each backend | 15 passed, 0 failed, 0 ignored: 7 existing and 8 named fixtures |
| Workspace tests excluding the PostgreSQL package, `--all-features --locked` | 1669 passed, 0 failed, 60 ignored, 38 suite summaries |
| Same workspace tests with default features, `--locked` | 1662 passed, 0 failed, 60 ignored, 37 suite summaries |
| Workspace/all-targets/all-features Clippy, `--locked -- -D warnings` | exit0 |
| Formatting, repository boundary checker and base-to-head diff check | each exit0 |
| Rust/Cargo and both PostgreSQL version/encoding/identifier-limit reads | each exit0 |

The new required Cargo test target is included by the existing PostgreSQL CI
package command. Local evidence adds exact names, duplicates/order, missing
positions, ASCII/multibyte truncation rejection, temporary and empty relations,
no-I/O empty input during a native aborted transaction, native error propagation,
external/local/rolled-back DDL and shared Repeatable Read name/fact snapshots.
Existing quoted-definition, domain and relation-kind groups also compare named
and OID facts directly. An explicit schema-before-`pg_catalog` fixture verifies
that a native domain named `text` keeps its native identity while the query's
parameter and output casts retain builtin types.

Two real implementation failures remain preserved. At `b65685a`, qualified
multi-array `unnest` produces PostgreSQL42883: the original native17 invocation
exits101 with 4 passed/3 failed, before the named target runs. The corrected
`ROWS FROM` pairing at `26701d9` passes twelve local checks, but fresh review
then identifies incidental unqualified `text` casts. Its regression-only
`c669c02` invocation exits101 with PostgreSQL42804 and one failed test. Final
`76a9da1` qualifies all nine text casts and passes the matrix above. Independent
passive review identifies these F1/F2 corrections and the precise D1 wording
for text casts; it inspects source and saved receipts rather than running
Cargo, SQL, network or stress checks itself.

Raw receipts are outside the distribution under `.darmok-work/logs/`:
`named-catalog-type-identity-fixed17`, `named-catalog-76a9da1-*`, and the two
negative prefixes `named-catalog-candidate-native17` and
`named-catalog-type-identity-negative17`. The author audit exits0 and binds all
source identities, exact commands/environments/ports, native/workspace counts,
nonoverlapping child intervals and unchanged prior evidence. Its facts are
`named-catalog-audit-facts-v2.json`. The 50-file code evidence manifest is
`named-catalog-code-evidence-v2.json`, SHA256
`86cecb43b3698e49f8deb766d0366e5e44bda1b1dcddc76f03b0dc682a3f5e40`.
A documentation leaf retains all other Git entries from that verified code.

This is native discovery progress. It does not pin objects against later DDL
or admit table execution. Catalog execution validity, reusable plans, public
table SQL, native/created-schema workloads, executable integration, real drivers,
measured performance and M2/M4 remain incomplete. Required hosted CI, merge and
release remain pending under the recorded account condition; no CI retry or
merge/release gate is claimed. Security work and compiler PR4 remain excluded,
and the full goal remains active.

### Native output representation local evidence

`feat/native-row-output` adds the [native output contract](native-results.md):
borrow a checked native statement and exact frontend column definitions,
check encoding metadata before Bind/Execute, and validate/encode all supported
native scalar values as text or binary row payloads. Any failure preserves the
destination. The binary writer accepts borrowed wire definitions directly;
no metadata-name vector is copied per row. Ordinary required native fixtures
cover output representations and private owned DML RETURNING recovery/finish.
The local implementation matrix runs on `33efae0`
(`dcae6c7bf48421716d436cc67fb32f30a30e1520`) with sixteen terminal checks,
all exit0. PostgreSQL 17.11 and 18.6 each pass nine native output groups and two
owned-scope groups; native parameters/statements/values additionally pass
5+6+6 groups each. Workspace all-features/default pass 1669/1662 with 62 ignored
across 38/37 result summaries. Strict all-targets/all-features Clippy, format,
repository boundaries and diff checks pass. Rust/Cargo are 1.96.0; both backends
use UTF8 and max_identifier_length 63. Required native fixtures fail when their
database environment is absent; unrelated ignored stress fixtures were not run.

Revision `07cd3a5` (`ca3551571a11ee7e78b9cf076591bf1f1085c8da`) adds positive
signed fractional-only -0.12 and precision 65/scale 30 cases. Only
`tests/postgres/native_results.rs` and its contract documentation change;
the other 275 Git entries, including production and workspace-test code, retain
the matrix revision's exact blobs. Six affected checks run on this final
fixture revision, all exit0: both native output targets (nine groups each),
strict Clippy, format, boundaries and diff. Prior matrix receipts are retained
with their actual revision rather than relabelled as runs on this revision.

Native commands are `cargo test -p darmok-postgres-tests --test native_results --locked`
and `cargo test -p darmok-execute --lib --locked native_backend::tests::native_row_output -- --ignored`.
The regression command selects `--test native_values --test native_parameters --test native_statements`.
Workspace commands use `cargo test --workspace --exclude darmok-postgres-tests --locked`
with `--all-features` for that variant. Both mandatory Cargo environment values,
exact argv, source/tree/status, ports 32769/32768, times, real exits and raw hashes
are recorded per invocation. The single Cargo/helper lane is verified across
28 nonoverlapping child intervals; aggregate helper intervals are separate.

Three fixture failures remain actual101: `c56d405` infers an i32 for a bigint
boundary literal before tests compile; `b2b6a1b` uses the wrong connector enum
spelling before owner tests compile; `64bab90` passes the success group and
fails the combined rollback-and-release expectation. `33efae0` corrects that
expectation to RecoverSavepoint. These change fixture assertions only, not the
output implementation. All 135 production Rust entries retain their initial
`c56d405` blobs through the final fixture revision.

An ordinary stock MySQL 8.4.11 observation through PyMySQL 1.1.2/Python 3.9.6
at port 32770 confirms signed DECIMAL(2,2) length 4 and DECIMAL(65,30) length 67.
It also confirms that valid -0.12 has five payload bytes despite length 4.
This verifies a metadata convention, not general stock row formatting.

Evidence lives outside the distribution under `.darmok-work/logs/`:
`row-output-33efae0-*`, `row-output-boundaries-*`, the three negative prefixes
`row-output-candidate-native17`, `row-output-b2b6a1-owner17` and
`row-output-owner-state-fixed17`, and `row-output-decimal-reference`.
The author audit exits0; `row-output-author-audit-facts.json` records source
equivalence, command/count/stream checks and unchanged prior evidence.
`row-output-code-evidence.json` binds 106 files, SHA256
`19a07bdbf8a1c84f89d22d1934b777de633ce6aefa8545344e730410d9aca357`.
Independent passive review finds no production/API defect and checks the source,
fixture corrections, ordinary stock facts and saved terminal receipts. It runs
no Cargo, SQL or network; its records use `review-row-output-c56d405-*`.
A documentation leaf preserves every other entry from the final fixture revision.

This is not semantic admission, generated metadata, catalog execution validity,
or a public row executor. M2/M4, hosted CI, merge and release remain incomplete;
security work and compiler PR4 remain excluded, and the full goal remains active.

### Native lookup context local evidence

`feat/native-lookup-context` implements the [native lookup contract](native-lookup.md).
Fresh owners submit fixed `ROLLBACK; SET search_path = pg_catalog` in one request,
requiring exact ROLLBACK/SET tags and idle readiness. Startup lookup options no
longer determine application schema resolution. Qualified native names remain
explicit inputs; implicit temporary namespaces still affect PostgreSQL lookup.
This is an initialization prerequisite, not catalog execution validity.

Revision `093da6f724e8751248c32ae9feb48e8d641fc507`, tree
`813556a12fa397a9318adc545276ff525bdf99b3`, passes all twenty sequential local
matrix checks. PostgreSQL 17.11 and 18.6 each pass four native lookup and ten
control groups, plus 55 ordinary owner groups with twelve filtered tests.
Three abandoned/dropped-control fixtures are explicitly excluded from those
owner invocations. BigDecimal variants additionally pass ten transaction and
eight TCP groups per backend, with 57/59 filtered. Three new owner lookup
fixtures also pass in the initial targeted PostgreSQL 17 invocation.

Workspace all-features/default pass 1669/1662 tests, with 65 ignored across
38/37 summaries. Strict all-targets/all-features Clippy, format, repository
boundaries and diff pass. Rust/Cargo are 1.96.0 on macOS 26.0.1 arm64; both
backends use UTF8 and max_identifier_length 63. Required database fixtures fail
when their disposable database environment is absent. No new transport
interruption, lock-retention, security or resource-stress probes were run.

The native command selects `--test native_lookup --test native_controls` in
`cargo test -p darmok-postgres-tests --locked`. Ordinary owner checks use
`cargo test -p darmok-execute --lib --locked native_backend::tests -- --ignored`
with the three exact exclusions recorded in their argv. BigDecimal commands
select `native_backend::tests::query_transactions` or `::frontend_loop` with
`--features sqlparser/bigdecimal`. Workspace commands exclude
`darmok-postgres-tests`; the all-features variant adds `--all-features`.

Evidence is outside the distribution under `.darmok-work/logs/lookup-093da6f-*`.
Each receipt records exact argv, source/tree/status, both mandatory Cargo
environment values, port, timing, actual exit and raw stream hashes. The author
audit passes, binds 28 terminal receipts and verifies 27 nonoverlapping child
intervals; the aggregate matrix parent is counted separately. The 89-file
`lookup-093da6f-code-evidence.json` manifest has SHA256
`0b79962ac5cc75f49c4cea0077e43bd49989589b51b059b6e026e3ca02392635`.
Prior row-output evidence and review seals, publication facts and the recorded
hosted CI condition remain unchanged.

Two native probe failures remain actual101: `9bcdecf` passes two groups and
fails an assumed rebind after schema replacement; `31d4a5e` passes three and
fails an assumed rebind after observing the intermediate rename. `efc31d4`
corrects these assertions to distinguish unchanged context from an explicit
path change or temporary namespace creation; all four native groups pass.
The final fixtures inspect actual tableoid separately from cached column
origin, demonstrating why matching row descriptions cannot prove identity.
A separate helper-copy assumption failure is bookkeeping only and occurred
before any file mutation; it is preserved in
`lookup-audit-helper-assumption-failure.json`.

Independent passive review finds no functional code defect and requests a
wording correction: owner Drop requests local driver abortion and drops its
client without awaiting termination. The documentation leaf makes that
guarantee explicit; it supplies no new code-test receipt. Review runs no
Cargo, SQL, network or security work.

Semantic admission, the complete catalog-validity algorithm, public table
execution, reusable plans, executable integration, real drivers and measured
performance remain pending. M2/M4, required hosted CI, merge and release remain
incomplete; security work and compiler PR4 remain excluded, and the full goal
remains active.

### Native bound portal local evidence

`feat/native-portal-description` implements the [bound portal component](native-portals.md).
Bind/portal Describe/Sync precedes a separate Execute/Sync request. The native
receipt retains actual binary columns and distinguishes NoData from a
zero-column RowDescription. Exact prepared-handle identity and every output
fact are checked before Execute; described rows share those observed columns.
The private owner output fixtures now use this path and observe final readiness
before finish or recovery. This is an output prerequisite, not SQL admission or
complete catalog execution validity.

Code/fixture revision `bdaacb8431a4985dbf914aba2960cacb677f9934`, tree
`b33964de4300f71591ec73266e52ebec4995e7cc`, passes all sixteen local matrix
commands. PostgreSQL 17.11 and 18.6 each pass seven required target suites:
backend completion 8, controls 10, lookup 4, portals 9, output 9, statement
representations 6 and typed completion 8, totaling 54 groups per backend.
The nineteen-type exact-payload fixture now uses checked bound descriptions.
Each backend also passes 55 ordinary owner groups with 12 filtered, and
BigDecimal transaction/TCP variants pass 10/8 groups with 57/59 filtered.
The three abandoned/dropped-control cases remain explicitly excluded from
ordinary owner invocations; no new interruption or resource-stress work ran.

New portal fixtures cover exact handles and cloned handles, native metadata
identity on rows, local parameter failure before submission, native planning
failure with failed readiness, unsupported observed result OIDs with drained
readiness, implicit-transaction rejection, NoData/zero columns/zero rows/empty
SQL, suspended fetches and remaining native SELECT counts, and stream ownership
after caller handles are dropped. Ordinary schema rename/recreation plus a
temporary namespace changes the actual portal origin: the guard rejects the
old origin for both nonempty and empty output before Execute, including an
unexecuted INSERT RETURNING. This does not prove hidden dependency validity.

Workspace all-features/default pass 1669/1662 tests, with 65 ignored across
38/37 summaries. Strict workspace and vendored-connector Clippy, both formatting
checks, repository boundaries and diff pass. Rust/Cargo are 1.96.0 on macOS
26.0.1 arm64; both backends use UTF8 and max_identifier_length 63. Exact argv,
both mandatory Cargo environment values, source/tree/status, fixture port,
actual exit, timing and raw stream hashes are retained in every author receipt.

The database command selects `native_portals`, `native_statements`,
`native_results`, `backend_completion`, `typed_completion`, `native_controls`
and `native_lookup` using `cargo test -p darmok-postgres-tests --locked`.
Ordinary owner commands select `native_backend::tests -- --ignored` with the
three exact exclusions in their argv. BigDecimal selects `::query_transactions`
and `::frontend_loop` with `--features sqlparser/bigdecimal`. Workspace commands
exclude `darmok-postgres-tests`; one adds `--all-features`.

Evidence stays outside the distribution under `.darmok-work/logs/portal-bdaacb8-*`.
The author audit exits0, verifies the sixteen sequential matrix child intervals,
and binds 32 terminal child/probe/format/environment/description receipts plus
the aggregate matrix parent. `portal-bdaacb8-code-evidence.json` seals 107 files,
SHA256 `9edfd6c427f34306578530ba68d74f8f8704dd3992a4f5a0d3cb0bc065027f99`.
Prior lookup publication/evidence and the hosted CI blocker remain unchanged.

Two earlier candidates remain actual101. `6ff8fa5` fails to compile new fixtures
that incorrectly assume a stream API; `f2e3a41` passes eight portal groups and
fails the expected resumed SELECT count. The corrected expectation retains
PostgreSQL's remaining count rather than inventing a total. These corrections
change fixtures only; production behavior is unchanged from `b78d293`.

Independent passive review at the code head finds no functional blocker and
identifies D1: the unsupported-result OID rustdoc must describe both terminal
typed-stream failure and portal representation failure with separate readiness.
The documentation leaf corrects only three Rustdoc passages, updates the portal
contract and appends this evidence; its equivalence proof preserves all other
entries, implementation bodies, fixture bodies and sealed author files. Review
runs no Cargo, SQL, network or security work and records its own actual exits.
Two reviewer recorder bookkeeping failures are retained separately: a script
construction SyntaxError before child commands and an overly narrow changed-path
assertion. Neither changes source or supplies runtime verification; the corrected
passive audit exits0. Core records use `review-native-portal-bdaacb8-*`.

Bind can process parameters and plan native SQL; no queued Execute is not a
general no-effects guarantee. The extra request adds a round trip and extends
the transaction's usual lock duration; this is structural analysis, not a
throughput or allocation measurement. Catalog execution validity, semantic
admission, generated frontend metadata, public table execution, reusable plans,
executable integration, real drivers and measured performance remain pending.
M2/M4, hosted CI, merge and release remain incomplete; security work and compiler
PR4 remain excluded, and the full goal remains active.

### Explicit native schema local evidence

`feat/native-schema-initialization` implements the functional
[per-database initialization component](native-schema.md). An idle exclusive
owner submits fixed BEGIN/DO/COMMIT and requires exact completion and idle
readiness. Initialization creates the reserved schema only when absent;
verification uses a read-only transaction and never installs. Matching repeats
preserve artifacts. Partial installations, changed definitions, extra objects
and version/profile mismatches fail without repair. Confirmed SQL failure keeps
its original observations and separately awaits rollback. The operation cannot
join, finish or recover a caller's existing transaction.

The manifest validates namespace inventory, relation/type/function structure,
PostgreSQL 18 NOT NULL constraint validation, and singleton version metadata.
Four strict signed-int8 substring kernels handle UTF8 text and bytea with two
or three arguments. This is typed kernel behavior, not frontend coercion,
unsigned/warning translation or admitted frontend SUBSTRING support. Ownership,
ACL and privilege properties remain deferred with the excluded security work.
Participating explicit operations use a database-scoped transaction advisory
lock. Neither installation verification nor qualified helper names provide a
catalog execution-validity lease against arbitrary DDL.

Code/fixture revision `3821d036529c3a3d9b5fbbff08631473bbb5708f`, tree
`c6987fca944bec05bb951d2b462013cf59f1b960`, passes all twelve local matrix
commands. PostgreSQL 17.11 and 18.6 each pass 10 native-control groups and 63
ordinary owner groups, with 12 filtered. The owner groups include eight new
initialization fixtures: unchanged repeats/read-only verification, absence and
partial/conflicting namespaces, metadata/version drift, functional definition
and inventory drift, outer-work preservation, rollback after creating every
artifact, 810 stock substring values, and two ordinary owners initializing and
verifying one physical database. BigDecimal initialization variants pass eight
groups per backend, with 67 filtered. These ordinary two-owner observations do
not certify forced-contention, arbitrary-DDL or stress coverage.

The exact required commands are:

```text
cargo test -p darmok-postgres-tests --test native_controls --locked -- --nocapture
cargo test -p darmok-execute --lib --locked native_backend::tests -- --ignored --nocapture --skip native_backend::tests::abandoned_transaction_recovery_never_restores_ready --skip native_backend::tests::unpolled_control_is_inert_and_dropped_pending_control_is_uncertain --skip native_backend::tests::dropped_scope_and_dropped_finish_never_restore_ready
cargo test -p darmok-execute --lib --features sqlparser/bigdecimal --locked native_backend::tests::native_schema -- --ignored --nocapture
```

Each database command runs separately for ports 32769/32768. No new resource or
forced-interruption experiment ran. Every Cargo invocation and helper that
spawns Cargo uses `RUSTUP_HOME=.darmok-work/rustup` and
`CARGO_TARGET_DIR=.darmok-work/native-values/target`, expanded to this workspace's
absolute paths, with only one Cargo/helper at a time.

Workspace all-features/default pass 1669/1662 tests with 73 ignored across
38/37 summaries. Commands are `cargo test --workspace --exclude darmok-postgres-tests --all-features --locked`
and the same argv without `--all-features`. Strict
`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`,
`cargo fmt --all --check`, `python3 scripts/check_repository.py` and
`git diff --check` pass. Rust/Cargo are 1.96.0 on macOS 26.0.1 arm64; both
backends use UTF8, max_identifier_length 63 and en_US.utf8 database collation.

The independent stock capture has 810 cases from MySQL 8.4.11 using installed
PyMySQL 1.1.2 on Python 3.9.6; the module reports `__version__ = 1.4.6`, retained
as the corpus's client field. The repository-local standard-library CLI observer
also verifies all 810 values and immediate zero-warning counts against pinned
image digest `6ea90827b1100f8f2ae306a539f86d2c264a26ed435a2a9f75551dd5c3aeb242`.
Its actual0 receipt is bound to `328c078`; observer and corpus bytes are proven
identical at the tested code head. The stock command and finite limits are in
`native-schema.md`; it is not a raw-wire proxy differential.

Evidence remains outside the distribution under `.darmok-work/logs/schema-*`.
Receipts record exact argv, source/tree/status, both Cargo environment values,
port, timing, actual exit and raw stream hashes. The author audit exits0, binds
24 terminal receipts and verifies 23 nonoverlapping intervals; the nested matrix
parent is counted separately. `schema-3821d03-code-evidence-manifest.json` seals
133 files, SHA256
`4ae91eef422cc2c87c59e6e71987a8077242aecd813d91d775513757cffd139e`.
`schema-3821d03-audit-facts.json` has SHA256
`9b570d94073ecfc6e52db22480df3c9b6d30b3e414a61aa4943434cf98ab0005`.
Prior portal publication/review/evidence and the hosted CI blocker are unchanged.

Three failed attempts remain preserved. `94ff5ec` has actual101 with seven
failed groups because PL/pgSQL terminates the unparenthesized IF CASE condition
at its inner THEN. `c925e5b` groups that expression and passes seven groups on
both backends. The first portable stock observer has actual1 before case
execution because it compares a tagged image name with Docker's canonical
RepoDigest spelling; `328c078` compares the pinned content digest and passes.
The first environment reader has actual1 on unavailable `lc_collate` GUC;
reader v2 uses `pg_database.datcollate` and exits0 without changing source or
fixtures. Audit readers v1/v2 were never invoked; v3 is the actual0 reader.

Independent passive source review at `c925e5b` finds no functional defect;
its 18 Git commands and separate invocation exit0. The later bounded source
and evidence review identifies F1: the future-CI stock observer directory needs
an always-run artifact upload. The workflow/documentation leaf adds only that
upload and evidence text; its proof must preserve all other tree entries and
sealed runtime files, and independent leaf review must close F1. No native
runtime receipt is relabeled as a test of this leaf. Reviews run no Cargo,
SQL, network or security work and retain their actual invocation outcomes.

CLI/configuration/routing integration, full functional SQL admission and catalog
validity, public table execution, real drivers and measured performance remain
pending. This is component progress; M2/M3/M4, required hosted CI, merge and
release remain incomplete. Security work and compiler PR4 remain excluded, and
the full goal remains active.

### Native schema temporary type correction and final local evidence

Self-review after the first workflow/documentation leaf found a remaining
functional lookup problem: implicit temporary namespaces can shadow unqualified
manifest declaration types even with `search_path = pg_catalog`. The ordinary
PostgreSQL 17 probe `schema-temp-types-probe-v1` has actual3 at `2914a62`: a
temporary `text` domain resolves the manifest's text arrays as integers and
raises an invalid-input error during local-variable initialization. The probe
connection exits and rolls back all temporary setup; it changes no permanent
installation or application objects. The probe exercises ordinary type
resolution within a transaction.

Final code/fixture revision `edd7361ffcd4b393986fc5e4e5e83c5edb77f622`, tree
`6b00ffd146641108a6276b105616af881c6961bd`, qualifies all declaration types and
array casts, updates the private creation switch, and adds the ninth ordinary
namespace fixture. Installed function body strings, table/metadata definitions
and all installation SQL after the DO BEGIN are unchanged. No schema version
change, replacement, migration or compatibility behavior is introduced.

All twelve revised matrix commands pass. Each PostgreSQL 17.11/18.6 backend
passes native controls 10, ordinary owners 64 with 12 filtered, and BigDecimal
schema variants 9 with 67 filtered. The three exact abandoned/dropped-control
exclusions remain in ordinary owner argv. Every native corpus run still checks
all 810 stock values. Workspace all/default pass 1669/1662 with 74 ignored
across 38/37 summaries; strict Clippy, formatting, repository boundaries and
diff pass. Commands and mandatory Cargo environment are the same as the prior
record, with fresh revision-bound receipts. Environment reader v3 records this
head with the same Rust/Cargo, OS, architecture, pinned fixtures and encoding;
it exits0 and preserves both earlier environment readers.

`schema-edd7361-author-audit` exits0. Its audit binds 27 terminal receipts and
verifies 26 nonaggregate sequential intervals; nested matrix parent is counted
separately. The 241-file `schema-edd7361-code-evidence-manifest.json` has SHA256
`0faa5066d6db9ded151ba019717d65213fa5a890c386eaf0ce7bc75833db3ea3`.
`schema-edd7361-audit-facts.json` has SHA256
`9272a7af6029b3162b154cfbc4dad959978801f7ab48972c2a40b333392394f4`.
This graph preserves the earlier 133-file seal, all failed attempts, original
reviews, and the historical workflow leaf proof. Stock observer and corpus
bytes remain equal to the independently verified `328c078` artifacts.

The original `c925e5b` review and bounded `3821d03` review remain immutable;
the latter preserves F1 open at that older source head. The seven-line uploader
fix at `2914a62` matches the observer directory, always runs, uses the existing
pinned upload action and fails if receipts are absent. Revised source/evidence
review checks this closure alongside the type fix and unchanged installed
artifacts. Documentation refinement D1 states precisely that function bodies
qualify built-in functions/operators/helper names, while manifest declarations
and casts qualify built-in types. The final leaf changes only evidence/spec
prose; its proof preserves all other entries and sealed files. No runtime
result is relabeled as an execution of a documentation leaf.

Required hosted CI, CLI and runtime integration, complete catalog validity,
admitted table SQL, real drivers, measured performance, merge and release remain
pending. M2/M3/M4 and the full goal are not complete. Security work and compiler
PR4 remain excluded; no hosted CI retry, account action, merge or release ran.

### Original-source binary literal execution

The `feat/native-binary-literals` branch adds distinct MySQL bit tokens/AST
values, quoted digit validation and complete unquoted-token classification.
The local SELECT lane emits direct hex/bit byte strings, parentheses/plus/aliases
and selected `_binary` introducers with stock field metadata. It adds no native
query or catalog plan. Numeric/coercion, other introducers/quoted binary text,
COLLATE and public table execution remain explicit unsupported gates. The
[component contract](query-binary-literals.md) records the exact scope and cost.

A stock standard-client capture from clean `271b4cf` independently records
MySQL 8.4.11 fields and values under four mode combinations. The selected
corpus has 40 positive cases/200 fields and cells, 28 syntax cases and 44
unimplemented contexts/names. Both initial captures remain outside the
distribution; the first width query used a reserved alias and retains its actual
SQL error, while the second uses a valid alias. Their process exits are0, not
claims that all submitted SQL succeeded. PyMySQL distribution1.1.2/module1.4.6
and Python3.9.6 remain pinned to the existing ordinary fixture client.

Draft checks currently pass two local corpus tests, five parser regressions and
two PostgreSQL17 command-phase TCP groups with both EOF formats, diagnostics,
next-setting preservation and earlier outer writes surviving rejected input
followed by valid results and commit. The repository-local stock observer also
passes112 observations:40 positive,28 syntax,44 unimplemented reference
contexts. It checks pinned image identity/content digest and connected fixture
UUID, and preserves each stock outcome and six actual subprocess exits0. The
stock result fields come from the pinned standard client; only supported binary
rows are exact byte observations. This is not an authenticated proxy driver run.

These are dirty-source development receipts under `logs/binary-draft-*`, not
committed-candidate certification. Three compile attempts retain actual101:
ambiguous Into at a tokenizer error, test code using mode constants as flags
and a private token constructor, and a partial test-edit replacement. The
corrected development checks retain separate actual0 receipts. Every Cargo
invocation uses the task RUSTUP_HOME/shared CARGO_TARGET_DIR, serially. A fresh
committed matrix and independent review remain required.

Future MySQL CI declares a pinned standard-client virtual environment, required
observer and always-run pinned artifact upload. Hosted CI is not polled or
retried under the existing account blocker. Security, compiler PR4, resource
stress and forced interruption work remain excluded. M2/M3/M4, catalog execution
validity, executable integration, real drivers, performance, merge, release and
the full goal remain incomplete.
