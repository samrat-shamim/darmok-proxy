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
