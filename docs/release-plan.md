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
- A project-owned PostgreSQL extension is permitted when required for correctness;
  the selected execution/DDL/lock mechanism still needs a proof and verification.
- No application-specific integrations or external control-plane dependency.

## Milestones and mandatory exit gates

| ID | Work | Exit gate | Status |
| --- | --- | --- | --- |
| M0 | Pin and inventory the starting implementation; establish baseline; write contracts | Reproducible source inventory and test results, documented inherited failures, architecture and scope contracts | Complete; baseline has one recorded failure |
| M1 | Independent workspace, vendored parser, package names, licensing and CI | Fresh checkout builds and tests without another repository or private dependency; upstream notices preserved | Complete |
| M2 | Extract the generic translation and execution engine | All application assumptions and allocation/capture/control-plane dependencies removed; both minimal end-to-end examples pass | In progress; component extraction only |
| M3 | Configuration, initialization, authentication, routing, diagnostics | Idempotent init, explicit mismatch errors, tested TLS/login/authorization/reset behavior, least-privilege runtime | In progress; explicit initialization executable component only |
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

On 2026-10-02, the sixteen reviewed PRs #27–#31 and #33–#43 were
squash-merged in dependency order at the owner's instruction. The final stack
merge is `7515ab2db9319303bb4557ba4cd4bd861d4fbcf5`, tree
`78fdc6304e6c3107aa921d08ca0085e0ab7a3057`. Each child was rebased onto
its actual merged parent with a guarded branch update; the full rebased and
merged trees equal the original reviewed trees. No source behavior changed
while restacking. The external `merge-stack-completed-audit-v1.json` binds all
sixteen results and409 successful command receipts, SHA256
`00635b1c3f9b71a295b4d6a4c92429fc415e97a1dee8ae7ab953b1ea64db9e01`.
The initial dry merge of PR28 failed with documentation conflicts caused by
squashed ancestry and was aborted; its actual1 receipt remains preserved.

Earlier pending-merge observations below retain their original evidence scope;
this ledger supersedes their merge status. Review and local source verification
remain distinct from hosted CI and release certification. No CI polling/retry,
administrator bypass, account action or release publication was performed.
Compiler PR4 remains outside the current work scope. Reviewed components now
follow one cycle: publish the PR, squash-merge it, then start dependent work.

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
initial corpus has 40 positive cases/200 fields and cells, 28 syntax cases and 44
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

The first committed candidate `10d6a240d158475c5e4f1ced5dfd2bfd6abc2cef`
passes ordinary PostgreSQL17/18 owners66 each and BigDecimal frontend10 each.
Its all-feature workspace run stops with actual101 at the existing
`sqlparser_mysql::parse_bitstring_literal` assertion: that test incorrectly
requires MySQL's new bit AST and Generic's unchanged byte AST to be identical.
The corrected test asserts each dialect independently, retaining Generic's
original result and identifying the modified upstream file. The failed run and
four successful backend runs remain immutable; the remaining matrix commands
did not execute. This candidate is not certified by those partial results.

The subsequent `bb77400cd60511cd46d320591959d4c73da2e9a0` passes both native
backend variants, workspace all1676/default1669 with76 ignored each, but strict
Clippy stops with actual101 at `manual_is_multiple_of` in the decoder. The next
revision uses the Rust1.96 divisibility API and computes first-group width once
before the byte loop. No warning suppression is added. The partial matrix and
lint failure remain distinct; feature, boundary and stock checks had not run.

The subsequent `9673f9b9a1c3bdda3beb207d28e35f375984c13b`, tree
`28ad20e9574d876b3f3fe6a590f92781b5e75d04`, passes all14 matrix commands,
including the full suites and strict lint. Its fresh stock observer matches112
cases, and author audit seals659 files/42 receipts with39 nonaggregate serial
intervals. This remains a historical code candidate after the fixture extension.
The independent source review found no functional defect but identified that
empty/unary-plus `_binary` forms were only captured in a rejected mixed query.
A separate clean-head stock capture now records those four expressions in a
successful query under all four modes. Four exact observations extend the
positive corpus to44 cases/216 fields and cells,116 total cases, and run through
the existing pure and ordinary TCP loops. No runtime implementation changes.
A new committed matrix and bounded review are required for the expanded fixture.

Future MySQL CI declares a pinned standard-client virtual environment, required
observer and always-run pinned artifact upload. Hosted CI is not polled or
retried under the existing account blocker. Security, compiler PR4, resource
stress and forced interruption work remain excluded. M2/M3/M4, catalog execution
validity, executable integration, real drivers, performance, merge, release and
the full goal remain incomplete.

Final expanded code/fixture revision `e6ccbf39b835be3d44620464eb7d1c61652147a2`,
tree `0d4589614348cef92ca95326e022a3af694ae849`, passes all14 local matrix
commands. Each PostgreSQL17.11/18.6 backend passes owners66/0 with14 filtered
and BigDecimal frontend10/0 with70 filtered. The three exact abandoned/dropped
control exclusions remain in owner argv; ignored groups are not counted as
passing. Workspace all/default pass1676/1669, each with76 ignored, across39/38
summaries. Strict Clippy, three parser feature builds, formatting, repository
boundaries, diff and the fresh stock observer all exit0.

Both mandatory Cargo inputs are recorded on every Cargo/helper receipt:
`RUSTUP_HOME=/Users/shamim/Projects/duotronic/.darmok-work/rustup` and
`CARGO_TARGET_DIR=/Users/shamim/Projects/duotronic/.darmok-work/native-values/target`.
The matrix invokes the following owner and frontend commands once per fixture,
at ports32769/32768 respectively. Each database URL selects the existing
`darmok_test` fixture. The remaining commands run once, serially.

```text
cargo test -p darmok-execute --lib --locked native_backend::tests -- --ignored --nocapture --skip native_backend::tests::abandoned_transaction_recovery_never_restores_ready --skip native_backend::tests::unpolled_control_is_inert_and_dropped_pending_control_is_uncertain --skip native_backend::tests::dropped_scope_and_dropped_finish_never_restore_ready
cargo test -p darmok-execute --lib --features sqlparser/bigdecimal --locked native_backend::tests::frontend_loop -- --ignored --nocapture
cargo test --workspace --exclude darmok-postgres-tests --all-features --locked
cargo test --workspace --exclude darmok-postgres-tests --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo check -p darmok-sqlparser --no-default-features --locked
cargo check -p darmok-sqlparser --no-default-features --features std --locked
cargo check -p darmok-sqlparser --no-default-features --features visitor --locked
cargo fmt --all --check
python3 scripts/check_repository.py
git diff --check 271b4cfadeccce057fb2665135ff7fec0e5b70a3..HEAD
```

The last check uses the existing pinned-client interpreter with `-B`, followed
by `tests/reference/observe_mysql_binary_literals.py --container
darmok-mysql-reference-84 --image
mysql:8.4@sha256:6ea90827b1100f8f2ae306a539f86d2c264a26ed435a2a9f75551dd5c3aeb242
--port 32770 --corpus crates/execute/fixtures/mysql-binary-literals.json
--evidence-dir /Users/shamim/Projects/duotronic/.darmok-work/logs/binary-e6ccbf3-stock-evidence`.
Its116 actual observations match exactly:44 positive cases/216 fields and cells,
28 syntax errors and44 unimplemented reference contexts. Six saved subprocesses
exit0. Image ID/content digest and connected server UUID association are checked.
The four new cases exactly preserve their separate clean9673 stock capture;
the original112 cases exactly preserve the initial-v2 capture. No generated
expectation, raw decoded numeric-wire claim or proxy driver claim is introduced.

The twelve environment-v4 readers exit0 and bind this head/tree to Rust/Cargo
1.96.0, macOS26.0.1 arm64, pinned PostgreSQL17.11/18.6 UTF8/max identifier63
fixtures, MySQL8.4.11, Python3.9.6 and PyMySQL distribution1.1.2/module1.4.6.
`logs/binary-e6ccbf3-author-audit-v1.json` exits0, SHA256
`65abf9469430f56772c1c7339e165433f571ba669f9608e81e66a7f5453c1323`.
It binds60 terminal command receipts and56 nonaggregate sequential intervals;
the four nested matrix parents are counted separately. The889-file
`logs/binary-e6ccbf3-code-evidence-manifest.json` has SHA256
`560b50d96fc459f6c8d7ae20700a8e3c617cf8dbe53529557e24a8b5847302b1`.
`logs/binary-e6ccbf3-audit-facts.json` has SHA256
`74e558ca6267884320108167bf92e64cba8779db6505a332c0cb0b4718a8900c`.
The graph preserves the prior659-file seal, original241-file initialization
evidence, PR41 publication/account-blocker anchors and all failed/draft attempts.
No historical or dirty-source result is relabeled as this committed run.

Independent source review at10d6 records44 Git commands0; the9673 corrective
source/evidence addendum records17 Git commands0; the e6 expanded-fixture
addendum records24 Git commands0. All three final passive invocations exit0.
No functional finding remains; four new successful queries close the original
positive introducer coverage concern. One exploratory missing-filename reader1
and one pre-write nested-quote builder1 remain separately preserved bookkeeping
failures, not runtime results or passed invocations. No reviewer reruns Cargo,
SQL, Docker or network checks. The current source/evidence report has SHA256
`1d7c80b09fb137e0d80d13214d3e16ae3ef7d5737b3c6896de3050506c38b96e`,
its seal `7712a2d544bf51416ba9e7a564083f9452578ab98e90831b07e57752684f891e`
and separate actual invocation
`602a3152f0e300b5e9ca872d3adf1331bb6cf075df5c52934b9ce2a06a748e3e`.

The evidence leaf changes only this release record and the component contract;
its exact parent/tree/evidence proof and independent wording review are separate
from the recorded runtime checks. Required hosted CI, public table/catalog-validity
execution, executable integration, real drivers, measured performance, merge,
release and M2/M3/M4/full-goal completion remain pending. Security work,
compiler PR4, resource stress and forced interruption remain excluded.

### Standalone setup executable component

The `darmok-cli` package now builds the real `darmok` executable with `init`,
`schema verify`, help and version. The explicit environment input must select a
physical database; no default database is inferred. The command invokes the
existing native owner and reviewed version-1 manifest, awaits disposal and
preserves ordinary failure/rollback outcomes. It adds no schema SQL, automatic
repair, raw SQL escape, credential policy or serving-path admission. The
[command contract](cli.md) records the deliberate addition of independent
schema verification before logical route configuration.

The independent initial source review found no functional defect and noted
that positive process coverage used URI inputs only. The followup fixture adds
successful keyword/value initialization and verification of the same explicitly
selected nondefault database, preserving catalog/application snapshots and
ordinary connection closure. This is coverage strengthening, not a production
bug fix or new credential policy. Its exact committed results are separate from
the original candidate and its frozen evidence.

Three offline process groups and three required database process groups are
implemented. Future pinned PostgreSQL17/18 CI explicitly invokes the latter and
always preserves their child receipts. Exact committed candidate checks and
independent review are recorded separately after verification; draft process
checks are development evidence only. The new README documents building and
running the setup commands while making clear that proxy serving and release
readiness are still pending.

This advances M3 initialization and the standalone executable path. It does not
complete M2/M3/M4 or the full goal. Configuration/routing, complete functional
SQL/catalog admission, `serve`, real-driver matrices, measured performance,
release artifacts and the final release remain required. Hosted CI remains
blocked by the existing personal-account condition: no CI polling/retry,
account actions, merge or release are performed. Authentication, TLS, grants,
security review, compiler PR4, resource stress and forced interruption remain
excluded.

### Standalone setup executable local evidence

The final tested code revision is
`6c0f3e899687e5cf4bf27c17d1178d0e643f3250`, tree
`d30e82956723e92049d9a64b8a17275d8399a13f`. Its production CLI/native owner,
schema manifest, dependency lock and CI entry points are unchanged from initial
candidate `9b3b2ff3c92a637063494a1daaa3a16a87a08046`, tree
`3df496c467b084d48892ea869523734dba53abb4`. The four-path correction adds
positive keyword-format fixture coverage, receipt metadata and the two component
documents. It does not repair a runtime defect or implement credential policy.

Both clean candidates execute their own 15-command local matrix with actual0
for every command. The final matrix is:

| Check | Final executed result |
| --- | --- |
| CLI required process groups, PG17/18 | 3 passed,0 failed/ignored each;23 actual children each:11 exits0,12 expected exits1 |
| Existing native schema groups, PG17/18 | 9 passed,0 failed/ignored each;71 filtered |
| Workspace all/default features | 1679/1672 passed,0 failed;79 ignored;42/41 summaries |
| Workspace and vendored connector Clippy | Both strict commands0 |
| Parser minimal/std/visitor | Three commands0 |
| Formatting, repository boundaries, diff | Three commands0 |
| Optimized `darmok` build | `cargo build -p darmok-cli --release --locked`,actual0 |

The three offline process groups are included in each workspace run and record
16 children each:5 exits0,5 expected exits1,6 expected usage exits2. Required
database groups use four disposable databases per major; the ordinary missing
database case never creates another. The two new keyword children on each major
have actual0, exact success text/empty stderr, `keyword_value` metadata and the
same explicit nondefault database as URI initialization. Catalog/application
snapshot preservation and ordinary server connection closure are asserted by
the process tests. All disposable CLI databases are confirmed removed by fresh
post-run readers. This establishes physical selection, not authorized aliases.

All Cargo invocations and Cargo-spawning helpers use
`RUSTUP_HOME=/Users/shamim/Projects/duotronic/.darmok-work/rustup` and
`CARGO_TARGET_DIR=/Users/shamim/Projects/duotronic/.darmok-work/native-values/target`.
`logs/cli-6c0f3e8-matrix.json` and its child receipts retain exact argv, committed
head/tree, environment, timestamps, actual exits and hashed raw streams.
`logs/cli-6c0f3e8-environment-facts-v3.json` binds15 fresh readers0 to
Rust/Cargo1.96.0, macOS26.0.1 arm64, Clap4.6.7 and pinned PostgreSQL17.11/18.6
UTF8/max identifier63 images, versions and cleanup observations. Per-reader
sidecars preserve actual statuses and stdout/stderr separately.

The final optimized binary is copied into an immutable development artifact.
Its SHA256 is
`90955f6f647b1b8146726136c9dfe64c9f980d5c4dbfb87548dd0fac399dcaf6`.
Three artifact readers exit0: native arm64 file identity, help and version;
the last two execute from an empty working directory. This checks loading and
offline CLI behavior only. It supplies no optimized database-command, serving,
Linux artifact, benchmark or release certification.

The original environment observer actual1 is retained. It compared a tagged
configured image reference with Docker's tagless `RepoDigests` representation;
the corrected immutable observer checks the exact configured reference, local
image identity and normalized repository digest. This was evidence bookkeeping,
not a failed product check. Eight original raw-reader pairs are preserved;
that failed observer did not persist individual child status sidecars or final
aggregate facts. The corrected baseline/final observers each record15 actual0
reader sidecars. No past status is rewritten or inferred as a new execution.

The final author audit `logs/cli-6c0f3e8-author-audit-v1.json` exits0, SHA256
`5503051b313b922e9e55edc0df307a7945890e648d64353a580c5e371ea34899`.
It binds45 terminal author receipts and43 nonaggregate sequential intervals;
two nested matrix parents are counted separately. The1755-file
`logs/cli-6c0f3e8-code-evidence-manifest-v1.json` has SHA256
`ed397fde508c6175e1dfad32ea0bffe66f77a86fe466e9591a25a8435fff5d12`.
`logs/cli-6c0f3e8-author-facts-v1.json` has SHA256
`4cbe785cf38ec8d93296940adf5641337ec52afc90b9fc228dcfa8a9dc6165f1`.
This preserves the1373-file initial CLI graph, its metadata failure, original
889-file binary evidence, PR42 publication and unchanged account-blocker anchors.

Independent passive source review records39 Git commands0 on the initial
candidate and21 Git commands0 on the corrected source/evidence. Both final
invocations exit0; neither executes Cargo, SQL, Docker, binaries or network
checks. The original nonblocking C1 keyword coverage observation closes only at
the corrected revision using its four actual successful child receipts. No
functional finding remains. The final report SHA256 is
`545097a83d09cc1ff846775e61508f5ddffd11033582faa41c6a0f8f8c48123b`,
seal `8536545fb935e086480f567f9fee100833392caccba3ca8e0e04fdebfe01bb13`,
and separate actual invocation
`c8e82656f85995ae59a357ecc5fbb8089fa2a765fefb16742137d9db82150f36`.

The final evidence leaf changes only this release record and `docs/cli.md`.
Its parent/tree, unchanged runtime entries, preserved evidence and passive leaf
review are proved separately. Runtime executions remain pinned to the code
revision above. M3 initialization/executable progress is concrete; the full
milestone and goal remain open. Hosted CI, public table/catalog validity,
configuration/routing, `serve`, real drivers, measured performance, complete
artifacts and release remain pending. The CLI component is merged as recorded
above. No CI polling/retry or account
action is performed; security work, compiler PR4, resource stress and forced
interruption remain excluded.

### Native scalar metadata component

`feat/native-result-metadata` adds the [native scalar field mapper](native-metadata.md)
needed to construct result definitions for independently created PostgreSQL
objects. It derives representations from reported types/modifiers, with
explicit numeric, temporal, charset and byte-width conditions. It makes no
origin, key, projected nullability or catalog-execution assumption. Five
required PostgreSQL fixture groups and three offline groups cover the mapper
and its integration with checked portals and both row encoders. No M2/M4 gate is complete;
public table execution, live dependency validity, configuration/routing,
`serve`, real drivers, measured performance and release artifacts remain required.

The clean tested revision is `55d137cd4fd936ba6ef0c8eaf78f18bdad7e1953`,
tree `a607e7ff59b7cd33e4aebe405f8fe4d3d4e7b470`, based on setup-tool
leaf `f95df5be7af891a82b9fd761d3a62820b09368b1`. It adds seven paths without
changing the dependency lockfile, native owner/control/encoding code, CLI,
frontend SQL admission or serving path. The existing PostgreSQL CI package
command automatically requires the new fixture binary; no hosted run is polled.

All twelve committed-source commands have actual exit0:

| Check | Executed result |
| --- | --- |
| Metadata/output/statement fixtures, PostgreSQL17/18 | 20 passed,0 failed/ignored each;3 summaries (5 new metadata,9 output,6 statement) |
| Workspace all/default features | 1682/1675 passed,0 failed,79 ignored;42/41 summaries |
| Workspace and vendored connector Clippy | Both strict commands0 |
| Parser minimal/std/visitor | Three commands0 |
| Formatting, repository boundaries, diff | Three commands0 |

The five metadata groups cover all nineteen scalar outputs and NULLs through
actual checked portals and both encoders, decimal endpoints/rejections,
temporal precisions0..6, binary-session UTF-8 byte bounds, BPCHAR padding,
utf8mb3 supplementary-character errors with unchanged output, domain base
descriptions, later DDL, lost expression typmods and a native enum named
`varchar` rejected by identity. Unsupported numeric RETURNING descriptions
are rejected before Bind/Execute and independent table reads confirm zero
writes. These are representation observations; no MySQL table-query, live DDL
lease, native effect-admission or full compatibility gate is certified.

All Cargo commands and helpers that may spawn Cargo retain both scoped
`RUSTUP_HOME` and shared `CARGO_TARGET_DIR` paths used above and run serially.
Immutable receipts are under `.darmok-work/logs/metadata-55d137c-*`, with exact
head/tree, argv, timestamps, actual exits and raw stream hashes.
Fifteen fresh environment readers exit0: Rust/Cargo1.96.0, macOS26.0.1 arm64,
pinned PostgreSQL17.11/18.6, UTF8/max identifier63 and zero remaining named
metadata fixture objects. Each workspace feature run preserves sixteen
ordinary CLI child receipts:5 actual0,5 expected1,6 expected usage2.
These child statuses are distinct from the twelve top-level successful checks.

The initial draft fixture compilation actual101 is retained in
`metadata-draft-03-pg17`: its empty generic binding iterator was ambiguous,
so no PostgreSQL test executed. The corrected fixture uses checked empty
bindings. Six draft receipts remain frozen; the other five have actual0.
The code audit `metadata-55d137c-author-audit-v1.json` has actual0, SHA256
`c1fa0da54099fecafd4d206dd34641e6c11206bb07f20571ddf746a0abef1704`.
Its227-file manifest has SHA256
`875acbb08f48dd8901c870be5c54395113298de8561869a8dc58f50caa0e3618`,
and facts `f1f8f268a59807e44ad5d74cf112e87980864724677e0b1c8c61fccf24be9fdc`.
It binds the new source/evidence and unchanged parent CLI manifest/publication
anchors; parent runtime evidence retains its original revision and scope.

The documentation leaf records the user's extension installation preference,
clarifies that complete semantic result definitions remain pending, and records
this evidence. Code/fixture executions stay pinned to the revision above.
Independent passive review has no functional finding. Its29 Git readers and
final v2 invocation have actual0, with report SHA256
`0950f8c8bd772b7423d2b9a305dbcf41fe82f466ceed75be0a5216429c0eca26`,
seal `3b909ef7075e904f34da225282aaa656b601a22a589779f528be73242d5b1608`
and separate invocation `91de7f16f7617d2ef16e051b9c6be0fdbd4c9b876233ebdf8503456d645c05e7`.
The first review invocation actual1 stopped at its clean-status assertion while
the author's pending documentation leaf was present. That bookkeeping failure,
helper and raw streams are retained; the clean-source v2 does not rewrite it.
No runtime was executed by the reviewer. The low-priority D1 wording observation
closes in this documentation leaf by naming the remaining complete semantic
result definitions explicitly.

Hosted CI remains blocked by the previously recorded personal-account condition.
The owner now requests reviewed PRs to be squash-merged in dependency order;
this supersedes the earlier merge hold. CI polling/retries, account actions,
release publication, security work, compiler PR4, resource stress and forced
interruption remain excluded. A component merge does not certify the pending
hosted-CI, compatibility or release gates.
The full goal remains active, including admitted table execution, `serve`, real
drivers, measured performance, complete artifacts and the final release.

### Server catalog publication lease component

The project-owned PostgreSQL17/18 module in `postgres/darmok_server` supplies a
transaction-owned read fence and generation stamp. Its native mechanisms live
in a separate, versioned `darmok_server` extension namespace; SQL compatibility
functions remain in `darmok`. Concurrent native PostgreSQL two-phase transactions are required for v0.1 per
the user's selected footprint. MySQL prepared statements and MySQL XA remain
separate features; no server-setting restriction substitutes for this native
concurrency requirement. Security-related work remains deferred.

The [lease contract](server-catalog-lease.md) now limits the global lease to
admitted catalog phases. Complete native dependencies are acquired outside the
lease and retained after it ends, before row execution. This is mandatory for
prepared transactions that combine DDL in one relation with row locks in another.
Ordinary metadata publication is fenced at native pre-commit; irreversible shared
drops use their native object boundary. Every native SQL prepared completion is
fenced, including pure DML; native core owns exact GID handling. Native invalidation
state distinguishes surviving metadata from rolled-back child DDL for ordinary
transaction commits. Shared drops exclude new readers through transaction-owned intent and drain
existing leases before irreversible effects. They release that short lock across
native backend-drain/storage waits, so ordinary exit publishers can finish using
the native lock queue. Native pre-commit separately fences final publication.

The earlier candidate `3e21463` passed catalog fixtures but its ordinary native
owner regression encountered a database-removal/temp-cleanup barrier deadlock.
That failure remains preserved and blocks acceptance of that revision. The next candidate `00899b5` passed 18 PostgreSQL18 groups, but independent
review found an unfenced absent-GID race: PREPARE was only queued when its
utility hook released the gate. Creation must retain that gate through native
prepare/abort callbacks. That source is also blocked and its evidence is
preserved. The lifetime correction at `73047ce` builds on both majors, but its
PostgreSQL18 run has actual exit101:14 groups pass and six valid-preparation groups
fail. Native utility execution leaves a successful PREPARE completion unchanged;
the hook incorrectly initialized its capture to UNKNOWN and rejected it before
the portal supplied the parse tag. That source is blocked. The correction seeds
the known PREPARE tag while retaining native ROLLBACK overrides and the lifetime
gate. The corrected `996ba71` passes20 catalog groups on each major, but its
PostgreSQL17 owner run exits101:65 pass and one target-database/temp-backend cleanup
fails with native E55006. Holding the early DROP fence across native backend drain
blocks retirement of that same target backend. That revision is also blocked.
The new shared-drop intent mechanism removes the lock across native lifecycle
waits; its regressions cover both target/unrelated exits and concurrent intents
through a native busy error. The earlier manual exit-barrier polling is removed.
At `d4fe45e`,21 catalog groups and66 ordinary native owner fixtures pass on each
major. The PostgreSQL17 package run nevertheless exits101 in the old concurrent
namespace observation: a prepared query rebinds to the replacement relation while
its cached description still names the original. Isolated module/bare PostgreSQL17
diagnostics both retain the old relation, and native source permits global plan
invalidation from namespace events. The observational fixture now serializes its
controlled DDL sequences and adds explicit native plan invalidation that proves
rebinding with an unchanged lookup path; it never treats cached origin as binding
proof. These test/documentation changes do not alter the server module.
The next candidate `36dff9e` stops its PostgreSQL17 catalog run with actual
exit101:20 groups pass and the concurrent-drop fixture fails. That assertion
incorrectly orders the two independent clients' responses. Native abort clears
its intent and wakes the reader; the reader result may reach its task before the
DROP error does. The corrected fixture observes continued reader admission wait
after the first drop commits, then checks the second drop's native E55006 and
reader success independently. It retains the exact generation/catalog checks
and changes no module behavior. The failed command and stopped matrix remain
preserved; they do not certify the corrected source.
Revised source must pass both PostgreSQL versions, the default/enabled 2PC settings,
prepared DDL/DML/mixed cases, the normal-exit regression and independent review.
The module does not change server configuration or add routing/auth/grant policy.

Required current-revision builds, PostgreSQL fixtures, local command matrix and
independent review are recorded in the subsequent evidence entry. Earlier draft
observations do not certify this source. Combined explicit module installation/
verification, fresh leased catalog snapshots, native scope integration, complete
semantic admission/results, reusable plans, MySQL snapshots/row locks and public
table execution remain required. M2/M4 and the full release goal stay open.

### Server catalog lease verification

The clean tested revision is `32bbc56f3094e23c71a070dce29f4c56d6eafb77`,
tree `ceefd195868418fb29f2fd8c9b6fefaa8072d10f`, based on main
`14ac0abb5a57228d3b600049086fb1b16667bde1`. All 25 serial command checks
have actual exit 0, as does their enclosing matrix:

| Check | Executed result |
| --- | --- |
| Full PostgreSQL package, PostgreSQL 17/18 | 108 passed each, 0 failed/ignored; 14 summaries each, including 21 catalog lease and 5 namespace groups |
| Ordinary native owner fixtures, PostgreSQL 17/18 | 66 passed each, 0 failed/ignored, 17 filtered |
| CLI schema process fixtures, PostgreSQL 17/18 | 3 groups each; 23 child processes each: 11 actual 0, 12 expected 1 |
| BigDecimal query/TCP transaction fixtures, PostgreSQL 17/18 | Four commands, 10 passed each, 0 failed/ignored |
| Workspace all/default features | 1682/1675 passed, 0 failed, 79 ignored; 42/41 summaries |
| Formatting, repository boundaries, strict workspace/connector Clippy | Four commands, actual 0 |
| Parser minimal/std/visitor; module license; installed artifacts | Six commands, actual 0 |
| Environment and fixture cleanup | Five environment readers actual 0; both servers have 0 prepared transactions, other clients, named lease relations and drop-test databases |

The four disposable profiles run pinned PostgreSQL 17.11/18.6 on Linux arm64
with `max_prepared_transactions=10` or the native default 0. The enabled
profiles exercise concurrent prepared DDL, pure DML, mixed row/catalog changes,
GID reuse, queued preparation/deferred triggers and native completion errors.
The default profiles independently acquire and release leases. Normal temporary
backend cleanup covers both the target and an unrelated database. Three forced
interruption/abandoned-control owner fixtures remain explicitly excluded;
neither local servers nor client controls are interrupted as failure experiments.

The module images were built at `d4fe45ef6e28c50a9c9bd1481bf7afc4d70468d4`,
tree `ab27123e3f766229f5e4a0b502d890d5ccca6918`, with actual exit 0 on
both majors. All eight module build inputs, including C, recipe, SQL/control,
license and README, match the tested revision exactly. This is explicit build
reuse, not a claim of a new build at `32bbc56`. The current identity receipt
binds those inputs to the running images, healthy profiles and installed
extension version 1.0. Seven installed library/bitcode/SQL/control/document
hashes are preserved per major; the module's Apache license matches the root.
The Alpine SDK package closure and other target platforms remain artifact gates.

Rust/Cargo 1.96.0 on macOS 26.0.1 arm64 ran every Cargo command and Cargo-spawning
helper serially with both scoped `RUSTUP_HOME` and `CARGO_TARGET_DIR`. Immutable
receipts are under `.darmok-work/logs/catalog-lease-32bbc56-*`; each binds clean
head/tree, argv, timestamps, actual exit and raw stream hashes. The two workspace
runs each preserve 16 ordinary CLI children: 5 actual 0, 5 expected 1 and 6
expected usage 2. Those child outcomes are distinct from top-level check exits.
The enclosing matrix stdout SHA256 is
`492aa723a52e5cb22aee30bc70b4c0ef50f55d4dcd0b0461430541cd9e61ef9d`.

The author audit has actual exit 0, stdout SHA256
`b7f5f3598eed4f234f47733f762c5b8de256a276365970d70afd3563b939fbdd`.
Its 1186-file manifest has SHA256
`7331fdd1b785e042b73d12ddcc625ed4508d238aba679cdd72f69e195f491bfa`,
and facts `2b58f20c0592d3df307c7ee31b2a4a965130dd71f3e05b8fed6e758f00a389be`.
It binds the current evidence, compiled module inputs, primary sources and
historical review seals without rewriting earlier failures. Earlier component
passes, isolated diagnostics and stopped matrices retain their original source
and scope. Independent final review is required before merging this component.
The lease primitive does not certify fresh catalog reads, complete native guards,
semantic admission, table execution, serving, real drivers, cache performance,
hosted CI or the full release. The goal remains active.

### Prepared-completion correction

Independent source review at `bed10ba8e18e71d01d17997e6228db3338a16ced`
found two further blockers in the prepared-target classifier. A native prepared
transaction can retain an exclusive lock on `pg_prepared_xacts`; querying that
view before native completion can wait on the very target whose finish would
release it. The unqualified text equality in that query also depends on the
finisher's operator lookup path, so case-distinct native GIDs can lose exact
classification. These are source findings, not executed hanging experiments.
The review addendum seal is
`4f064bce03684bb92389ff9b9df53e69846b1e371f013ab5d84d1b08129aaf8b`.
It supersedes the earlier finite acceptance; the successful `32bbc56` matrix
remains historical evidence and cannot certify the replacement.

The replacement fences every SQL prepared commit or rollback before native
completion. It removes classifier SQL, marker locks, GID hash/session gates and
completion-tag capture. Native core alone resolves exact GIDs and preserves its
target checks and errors. PREPARE keeps metadata private and cannot transfer the
global fence. Errors clean up through native transaction resource ownership.

This deliberately trades selective prepared-DML cache reuse for correctness:
all prepared completions serialize with catalog readers and advance generation,
including pure DML and native errors after fence acquisition. The fence includes
native completion work and waits; it has no uniform short-duration guarantee.
Ordinary pure-DML commits and ordinary lease protocol round trips are unchanged.
New ordinary regressions cover retained prepared-view locks, schema-local text
equality with case-distinct GIDs, queued PREPARE validity, concurrent completions,
native error cleanup and row waits outside catalog leases.

New module builds and current-source verification on PostgreSQL 17/18 with
enabled/default two-phase settings, the ordinary owner/CLI matrix and independent
frozen-source review are required before merging. The subsequent entry records
the replacement's executed checks. Fresh catalog reads, complete native guards,
frontend data snapshots, table execution, serving, cache performance and release gates remain
open. Security work and interruption/stress experiments remain excluded.

### Prepared-completion verification

The clean tested and freshly built revision is
`202166984df7fb00bd0358c04177ef4c15ee3e13`, tree
`f35ed2e9f98066beeb08611d6d237ddc0a21619d`. Both module builds, all
13 setup steps and their enclosing command have actual exit 0. The four profiles
run pinned PostgreSQL 17.11/18.6 with native two-phase settings 10 or 0; no earlier
module binary is reused to certify the changed C source. Previously healthy,
inactive profiles were stopped normally only after native checks found no
prepared transactions or other client backends. Historical quarantines remain
untouched.

All 25 serial checks and their enclosing matrix have actual exit 0:

| Check | Executed result |
| --- | --- |
| Full PostgreSQL package, PostgreSQL 17/18 | 110 passed each, 0 failed/ignored; 14 summaries each, including 23 catalog lease and 5 namespace groups |
| Ordinary native owner fixtures, PostgreSQL 17/18 | 66 passed each, 0 failed/ignored, 17 filtered |
| CLI schema process fixtures, PostgreSQL 17/18 | 3 groups each; 23 child processes each: 11 actual 0, 12 expected 1 |
| BigDecimal query/TCP transaction fixtures, PostgreSQL 17/18 | Four commands, 10 passed each, 0 failed/ignored |
| Workspace all/default features | 1682/1675 passed, 0 failed, 79 ignored; 42/41 summaries |
| Formatting, boundaries, strict workspace/connector Clippy | Four commands, actual 0 |
| Parser minimal/std/visitor; license; installed artifacts | Six commands, actual 0 |
| Environment and fixture cleanup | Five environment readers actual 0; both primary servers have 0 prepared transactions, other clients, named lease relations and drop-test databases |

Both majors verify native COMMIT and ROLLBACK of a prepared transaction retaining
the prepared-transaction view's exclusive lock. The finisher first waits on the
publication fence, then completes and releases the retained view lock. A second
fixture proves schema-local text equality differs from native equality, then
finishes case-distinct GIDs with separate commit/rollback catalog outcomes.
Prepared pure DML and rolled-back child DDL now explicitly wait for catalog
readers and advance generation. Queued PREPARE remains private; a finish before
native validity reports the native missing-GID error. Duplicate/reused GIDs,
top-level and savepoint native errors, active-lease preparation rejection and
mixed row/catalog transactions retain their ordinary lifecycle checks.

Immutable receipts are under `.darmok-work/logs/catalog-lease-2021669-*`.
The enclosing matrix stdout SHA256 is
`b923b456270c0dde915ab5cc7a303f839ab43052a92256cc85a4799e44365d90`;
the setup enclosure is
`3485fb9e9ae802a38a0de08dff3beaa10de359e0b9a7e8c86ea44a49796a6424`.
Every Cargo invocation and Cargo-spawning helper uses the scoped Rustup and
target directories, serially. Rust/Cargo 1.96.0 on macOS 26.0.1 arm64 ran the
Rust checks; pinned Linux arm64 builds supply seven installed artifact hashes
per major. The SDK package closure and other platforms remain artifact gates.

The author audit has actual exit 0, stdout SHA256
`d8f3bbaac7d72e418f1a35c7fea9185821f79d46c63e95dd1673af9a231c7d89`.
Its 2194-file manifest has SHA256
`973e4e6c1bbd0961a977f01cca7abb5bd12cd42b3d6935f6638d6edac9a9f38a`,
and facts `daf0f13ec81389fff89e4c34a0b3496171b3d6cdf8cd7f15e861473c49ff6697`.
It preserves the prior successful matrix, failures and superseded review seals
under their original revisions. Seventeen exact Git source snapshots also have
actual exit 0 and match the manifest; their receipt SHA256 is
`b14254af3e995a5b4c516675c38b0435d2adf13fcf189f0ad2af69537c38c299`.
This documentation leaf adds the executed evidence without changing module or
fixture inputs. Independent final review is required before component merge.
These passes do not certify fresh catalog reads, complete native guards, semantic
admission, table execution, serving, drivers, cache performance, hosted CI or the
full release. The goal remains active and the user's exclusions remain in force.

### Catalog lease merge and next control boundary

[PR #45](https://github.com/samrat-shamim/darmok-proxy/pull/45) was normally
squash-merged as `56b5ac5fe973946e7862469d970ade3a646a9814`, tree
`05aab2bcdd324ecdcc4899e095f395918ede2add`. That tree equals the final reviewed
documentation leaf `6334aba60ea5c99bf3d8b941b67de48cc4ac4b84`; all 314
other tracked entries equal tested revision `2021669`. The independent finite
component review has report SHA256
`459dfddf46b98bc036cc92c77e0489d28419ba48cd999531c715faef1b7ca2c2`,
seal `c8c3e70b8ec72046b39577f3eacf0c37857fcc7afae70a1b51c18826a9f48a08`,
and separate actual-0 invocation
`f70ffbd925161b555a2aefea669d8be5b998a698bdaec8c3e9a0a9c6e08c7329`.
The publication audit has actual exit 0, binds 125 files, and records exact tree
equality, the public personal repository, its non-experimental description,
and no eligible open PRs. Its facts SHA256 is
`aca6d7bd41b0277ac2e1537f595d78096e0e92bc99f046624dc70afe42a502de`.
The audit does not claim hosted CI or release completion.

[Issue #46](https://github.com/samrat-shamim/darmok-proxy/issues/46) records why
the existing SELECT-based controls cannot serve fresh catalog discovery without
prematurely creating the owner's first native data snapshot. PostgreSQL 17/18
source research supports top-level parameterless SET/SHOW as snapshot-neutral,
but this is an unimplemented candidate transport. Independent design review
keeps complete catalog/index/TOAST/output/invalidation guards and a continuous
supported backend-state invariant open. It also requires exact-backend module
proof before accepting SET acknowledgments, and fresh acquisition portals with
historical-stamp/expiry semantics. The passive review report has SHA256
`75feb525b0346365188a9e17f9762f3659c3b654a0499f6a409efae188c47ee1`,
129-file seal `3c6994cb61f284b0c406a5643bab5291355aededcdf59c80f1b51c338fd7f2e9`
and separate actual-0 invocation
`11da2d583c2564abddbe5b59abd55698879385d2eb0aaa80b1aa4663d3f80242`.
Bookkeeping failures remain preserved; no runtime or implementation acceptance
is inferred. Concurrent native two-phase transactions remain required for v0.1.

### Observed text simple-query component

The next connector component supplies exact completion for row-returning
simple queries. Upstream simple-query streams erase exact tags, empty-query
distinctions and final transaction state; command-only events reject row
descriptions. Reusing the existing encoder, request queue and text row carrier
preserves the native sequence in one request without result-type lookup SQL.
Raw column facts are shared per statement; unknown OIDs require no invented
connector type. Unsupported binary descriptions terminate explicitly after
submission. The [completion contract](backend-completion.md) defines the API,
costs and ten required ordinary fixtures. Current-source PostgreSQL 17/18
verification and independent review are required before merging. Catalog guard,
snapshot/admission, table execution, serving, driver/cache/performance/artifact,
hosted CI and release gates remain open; the goal and exclusions are unchanged.

### Observed text simple-query local verification

The tested clean code revision is
`ca84ea2c70d66e953ed2deb61a8a4920bcccf3b2`, tree
`a943445f4c29297c89137398ff24268b54d9c98c`.

| Check | Executed result |
| --- | --- |
| New text simple-query fixtures, PostgreSQL 17 | 10 passed, 0 failed/ignored |
| Full PostgreSQL package, PostgreSQL 17/18 | 120 passed each, 0 failed/ignored; 15 summaries each, including 10 new transport and 23 catalog lease groups |
| Workspace all-feature tests/doctests | 1682 passed, 0 failed, 79 explicitly ignored; 42 summaries |
| Formatting, repository boundaries, strict workspace/connector Clippy | Four commands, actual 0 |
| Isolated connector without runtime features | Offline consumer check actual 0; locked metadata confirms no connector features and all 80 registry packages match the product lock |
| Environment and cleanup | 14 native/source commands each, actual 0; eight module inputs equal tested `2021669`, both pinned images/library hashes match, native 2PC remains 10, no prepared transactions, other clients or snapshot-fixture tables remain |

The original optional `cargo check -p tokio-postgres --no-default-features`
invocation was rejected before compilation because the vendored dependency is
outside the workspace. Its actual 101 and the enclosing matrix's propagated
101 remain preserved. The corrected external consumer selects the exact local
connector with default features disabled and leaves product source/lockfiles
unchanged. A first metadata helper failed before spawning Cargo because the
default Python lacked `tomllib`; Python 3.13 ran the same helper successfully.
A first author audit expected seven module inputs, overlooking `.dockerignore`;
the corrected audit verifies all eight. Both actual-1 bookkeeping failures
remain separate from the executed product checks.

Immutable receipts are under `.darmok-work/logs/simple-completion-ca84ea2-*`.
The author audit has actual exit 0, stdout SHA256
`87752199318d2468dc7ce40d7e2d7343d80f316da7fb4c359cbc6519123c5839`;
its 538-file manifest is
`a420a855c46a9f5916d60219594cdcc593cc80ede5d3403ad814ef1d5c1db72c`,
and facts `a26c0399648050f29f156c0a2393a10082ee41159a7d95e95ed8f75f50c0b411`.
Seventeen pinned source snapshots and 21 Git readers have actual exit 0.
Four environment readers confirm Rust/Cargo 1.96.0, macOS 26.0.1 and arm64.
Every Cargo invocation and Cargo-spawning helper uses the scoped Rustup/target
directories serially. The passive design-review seal's 129 files were rehashed;
its open proof obligations remain unchanged.

Independent source review found no ordinary functional blocker and requested
precise allocation wording: descriptions collect a Vec, own name strings and
construct a shared Arc slice; rows retain their existing ranges allocation.
This documentation leaf applies that correction and records verification
without changing code. Final independent acceptance is required before merge.
Pending-only/partial handoff and upstream helper propagation are source-reviewed,
not extra runtime fixture claims. No native data-snapshot policy, server catalog
control, complete guard proof, semantic admission, table executor, serving,
driver/cache/performance/artifact/hosted CI or release gate is certified here.

### Observed text simple-query merge

[PR #47](https://github.com/samrat-shamim/darmok-proxy/pull/47) is merged at
`f5d8315c1e9c55462335204919a0db05ab0e03bb`, tree
`52dca3e9507ae5636b610aa2fc4e83c115f51f92` (317 tracked entries).
The documentation leaf has that same tree; all 315 non-documentation entries
match tested `ca84ea2`. Independent final acceptance is recorded under
`.darmok-work/logs/review-simple-completion-ca84ea2-v1`. Its 169-file seal is
`4733650b7bc07ca1d58343955211ec2401ad673cb34e7dda8a6239929ece9a0d`;
publication facts and a separately audited 888-file manifest
have SHA256 `1cffebe01dc74baa5d187702f564d5a9fab19d600233ce2894a6dabeab9e6875`.
This closes the transport component only, not the open execution/release gates.

### One-shot discovery implementation boundary

The selected architecture replaces exported SELECT lease handles with inert
SET LOCAL and one-shot native SHOW. Catalog caches, invalidations, installation
verification and fixed-heap descriptor waits occur outside Share. An
unchanged-generation reacquisition permits direct fixed-prefix heap scans;
Share ends before cleanup and output. The returned facts/stamp are immutable
observations, never live leases. This permits concurrent native 2PC without a
`max_prepared_transactions=0` requirement. See
[catalog-discovery.md](catalog-discovery.md) for the continuous private-owner
profile, cost, phase budgets and remaining admission/guard obligations.

Conditional architecture review of proposal v3 has SHA256
`c3553cbf347cc105a39672e6cad649355ec0df77181365f0e584a231dc59c520`.
The independent report is
`7a7c7d19eabbbb3502970a1cd1e70052c7e0c6111b1babb3739d0f3818e49509`;
its 260-file seal is
`3cabd5c4e4966b7260bcfe1f53308c5efe08ba824875e8fbb2b31800faf04dce`.
The separate final invocation has actual exit 0 and SHA256
`862728d6c78c1bfd9a7380ef5626597754f9a08213033c214f901376a1dd0b9c`.
Root's clean-base rehash audit also has actual exit 0, stdout
`baa34eeebd7c8c1abbf5a28fece6ec8f2d0d77ac1b44755607d204096d4c3cc1`.
Evidence is under `.darmok-work/logs/review-catalog-discovery-design-v1-f5d8315-v1`
and `catalog-discovery-f5d8315-01-architecture-audit`; failed bookkeeping attempts
remain preserved. This is design/source acceptance, not runtime proof.

The implementation adds the native reader, strict fact decoder and exclusive
scope integration, retires the unreleased SQL lease API and replaces publication
fixtures with a separate native test probe absent from the product image.
The current-revision local verification below supersedes the pending build and
fixture status. The earlier SELECT-lease results do not certify this changed
native module.
MySQL data-view/lock policy, native dependency guards/recheck/admission, table
execution, serving, driver/cache/performance/artifact/hosted CI and release
gates remain open. Security work, compiler PR #4, stress/forced interruption,
remote CI/account actions and release publication remain excluded.

### One-shot discovery local verification

The tested clean implementation revision is
`3bc480ad0b73bf403cfd02c7d650fffe23680615`, tree
`bbaab322a552126a36d51a711b2d3485251977ee`.

| Check | Executed result |
| --- | --- |
| Required private-owner discovery fixtures, PostgreSQL 17/18 | 3 passed each, 0 failed/ignored; both scope kinds, first data views, native missing-relation recovery and disposal-only installation/profile failures |
| Full PostgreSQL package, PostgreSQL 17/18 | 123 passed each, 0 failed/ignored; 16 summaries each, including 11 discovery and 15 publication groups |
| Workspace all-feature tests/doctests | 1690 passed, 0 failed, 82 explicitly ignored; 42 summaries |
| Formatting, repository boundaries, strict workspace/connector Clippy | Four commands, actual 0; boundaries cover 9 packages |
| Fresh product/probe builds | Four actual-0 builds at `958be08710bc70d7714a6bdaf6eb5b0b86981e1a`; all 14 module/probe inputs equal tested `3bc480a` |
| Setup and final environment | 75 setup and 40 final environment commands, all actual 0; six healthy profiles, exact image/library/bitcode identities, probe absent from product images, no prepared transactions, other fixture clients or named fixture tables remain |

Each version used three fresh profiles: a preloaded/installed primary with
`max_prepared_transactions=10`, a preloaded/installed native-default profile
with `0`, and an installed but unpreloaded product profile with `10`. The `0`
profile is an additional ordinary case, never a requirement. Prepared catalog
heap waits occur outside Share; both native COMMIT PREPARED and ROLLBACK PREPARED
complete while the reader waits. Prepared user DDL has committed setup before
preparation and becomes visible after completion. No stop, restart or forced
cleanup was used; the six healthy disposable profiles remain retained.

Build bases are pinned `postgres:17-alpine` digest
`b0f9560a2de083e2cc7382e75f808c7381a32852a7ec49117deedb300e552b24`
and `postgres:18-alpine` digest
`77f585114c32fbca283dc835b0596f4e52b51b4c6662d7810b2f4084f60a1873`.
The executed servers are PostgreSQL 17.11/18.6 on Linux arm64/musl; the local
Rust/Cargo toolchain is 1.96.0 on macOS 26.0.1 arm64. Every Cargo invocation or
Cargo-spawning helper ran serially with scoped Rustup and target directories.
Other platforms and hosted CI are not certified by these receipts.

The bounded sequential observation used 32 warm-up and 128 measured requests,
one SET/SHOW round trip each. PostgreSQL 17 had 81 namespaces, 416 classes,
3133 attributes and 619 types, with p50 463250 ns and p95 606458 ns. PostgreSQL
18 had 21/416/3168/623 respectively, with p50 383625 ns and p95 440709 ns.
This measures the complete discovery request over local TCP, not proxy
throughput, cache hit rates or parallel contention. Two Share acquisitions
describe an uncontended successful attempt; admission/generation retries add
acquisitions. Full scans scale with catalog size, and phase allocation budgets
do not bound total process memory.

Two failed fixture-development attempts remain preserved: `958be08` native
compile exited 101 because the empty portal parameter iterator lacked a type;
`a01ef8d` discovery exited 101 with 10 passed/1 failed because a combined
CREATE/BEGIN/ALTER/PREPARE request left the table private to its prepared
transaction. Explicit typing and a separately committed CREATE correct those
fixtures. The failed prepared transaction was resolved through ordinary
ROLLBACK PREPARED, actual 0. Earlier unused-result warnings were removed by
checking completion/disposal receipts or explicitly discarding cost observations.

Independent review of `a01ef8d` found F1: generic native discovery errors could
permit scope recovery although installation/profile failures require disposal.
`3bc480a` permits recovery only for a requested missing relation or exhausted
pre-effect generation attempts, with exact failed readiness and no stream or
shape error. New owner fixtures cover missing per-database installation and
the unpreloaded module's placeholder echo. The original review finding remains
preserved in `.darmok-work/logs/review-catalog-discovery-a01ef8d-v1`; its 99-file
seal is `d4645bb6cb747a54e8d6e1311c9186ac5692fc9216c99fd6cc796759eb20a523`.
The independent current-source pass finds no further functional blocker and
requested the acquisition-count clarification included in this documentation
leaf. Final bounded source/evidence acceptance is required before merge.

Immutable command receipts are under
`.darmok-work/logs/catalog-discovery-{4217002,958be08,a01ef8d,c97f94b,3bc480a}-*`.
The actual-0 author audit is `catalog-discovery-3bc480a-11-author-audit`, stdout
SHA256 `9fa0999b4617325add7be7cda3a2e6bdd5c39e0763b94d3c0c56277e94f01597`.
Its facts have SHA256
`2c0e38e62460e5d9050d62a90a8954496ecdc47407bacafd1209cf1c97985b84`;
the 1421-file manifest is
`cab4c7dd525a4e81be4494f36da32e178e086903df8c1f23586cb44462f65186`.
It verifies 11 current and 16 historical command receipts, all 162 source
readers, the 115 nested setup/environment commands and all 14 build inputs.
The old 129-file passive-control, 260-file conditional architecture, 169-file
simple-query and 99-file initial implementation seals were rehashed unchanged.
Pinned primary-source manifests bind 94 discovery and 30 earlier control
source captures. Independent current review evidence is under
`.darmok-work/logs/review-catalog-discovery-3bc480a-v1`.

This closes local one-shot observation verification under its private-owner
profile. Complete native dependency guards, fresh recheck, supported preparation
and admission still belong to [issue #46](https://github.com/samrat-shamim/darmok-proxy/issues/46).
MySQL data/current-read and lock policy, table execution, serving, real drivers,
cache/performance/artifact checks, hosted CI and release gates remain open.
The goal remains active; security work, compiler PR #4, stress/forced interruption,
remote CI/account actions and release publication remain excluded.


### Publication gate repair after PR #48

[PR #48](https://github.com/samrat-shamim/darmok-proxy/pull/48) was normally
squash-merged as `3cc6d528aa8d0943859862c5c00352a78b496503`, exact tree
`f9052783ef7fd90bd36c8e321cc08a4e1842e691`. Its publication audit has actual exit 0,
1726-file manifest SHA256
`cc11d31e8944edafbc283e6e4dc22c47a75290be5433a48e393faaeccc6c544e`, and facts
SHA256 `14a7a6dc70f0a53c1734283f62bb03ea6b8ba6aaed04548ceae6d60ce05c866a`.
It verified the public personal repository and exact reviewed leaf; hosted CI,
account/admin changes and release publication were not performed.

Subsequent pinned-source review found the late native ON COMMIT catalog cycle
tracked in [#49](https://github.com/samrat-shamim/darmok-proxy/issues/49). The old
publication evidence does not certify this case. PRE_COMMIT precedes indexed
TEMP ON COMMIT DELETE ROWS catalog waits on both supported majors; a prepared
catalog lock holder could need the global fence retained by that publisher.
Earlier native Parse/Bind history also means a prepared finisher can have its
own late temporary cleanup. No baseline hanging reproduction was performed.

The repair separates a retained ordinary publication gate from a short global
drain, gates both discovery phases, and releases prepared completion's global
reference at native utility return or ordinary ERROR before the caller cleanup.
The source contract is updated; fresh changed-source native tests, exact new
artifacts, bounded cost measurement, self review and independent core review
passed as recorded below. Concurrent native 2PC remains required and enabled.
This does not close
#46, snapshot-neutral preparation, dependency guards, semantic admission, MySQL
read views/locks, table execution, driver/cache/performance/artifact or release
gates. Security, compiler PR #4, stress/forced interruption, hosted CI/account
work and release publication remain excluded.


The conditional source architecture review is sealed separately in
`.darmok-work/logs/review-native-dependency-guards-3cc6d52-v1`: report SHA256
`6fe84023251e70b534f3ebbabd33c8d247fcba17d8ac96a03aee9f3c9c0230a1`,
334-file seal `23e662a96d90396afdc99e6055d48532c6c52718a7f519229aa3dc8f58fe93ca`,
outer invocation actual exit 0. It is a source recommendation, not runtime
acceptance. Draft implementation `957d11950b0860c5af0298b7318371fc4ff030c7` compiled
and produced four fresh PostgreSQL17/18 product/probe images. Review found named
Parse statement reuse across outcomes; `4fe5b2e365654a54421ce84ee0b9d499561dd000`
corrected only those names. All 14 product/probe inputs remained equal.

The first runtime attempt at `4fe5b2e` did not reach the intended publication
path: first execution of a prepared observer query still built a plan and waited
for the prepared `pg_class` holder. The local Cargo receipt has actual exit 101,
with its root-owned test executable ended by SIGINT after source/lock diagnosis.
A separate normal ROLLBACK PREPARED connection attempt timed out during startup
(actual exit 2); SQL was never submitted and no rollback is claimed. No native
PostgreSQL signal, termination, stop or restart was performed. That primary
profile remains quarantined; this run is failed harness evidence only, with no
interruption or semantic acceptance claim.

The corrected observer forces generic plans, executes every probe before each
prepared catalog holder, uses an explicit integer sub-ID result, and bounds each
critical phase. It retains a separate finishing client across caught assertion
failure so normal prepared rollback can be attempted before rethrowing; absence,
native errors and cleanup timeouts are logged and never convert failure to
success. Corrected checks use six fresh disposable profiles. Their current
results and independent review are recorded below; the module inputs are
unchanged.

### Publication gate local verification

The tested clean source revision is
`7d7fe8ec7ac903cfdd306ff4e78268a9fcd763d3`, tree
`b4dd1efdff671a9e6bd33184a74d6c3f4b77bde1`.

| Check | Executed result |
| --- | --- |
| Full native PostgreSQL package, PostgreSQL 17/18 | 126 passed each, 0 failed/ignored; 16 summaries each, including 11 discovery and 18 publication tests |
| Private-owner discovery subset, PostgreSQL 17/18 | 3 passed each, 0 failed/ignored; this is the three `native_catalog` groups, with 86 other owner tests filtered |
| New publication regressions | PG17 groups 1+1+1 passed; PG18 publication 18 passed; both prepared outcomes, exact no-Sync native history, late cascade and native cleanup errors |
| Workspace all-feature tests/doctests | 1690 passed, 0 failed, 82 explicitly ignored; 42 summaries |
| Formatting, repository boundaries, strict workspace/connector Clippy | Four commands, actual 0; repository boundaries cover 9 packages |
| Fresh product/probe builds | Four actual-0 builds at `957d119`; all 14 product/probe inputs equal tested `7d7fe8e` |
| Acceptance setup and final environment | 74 setup and 40 final environment commands, all actual 0; six healthy exact profiles with no prepared transactions, other fixture clients, named fixture tables or functions |

The six acceptance profiles use the same pinned PostgreSQL 17/18 base digests
listed above, executing native 17.11/18.6 on Linux arm64/musl. Each version has
an installed/preloaded primary with `max_prepared_transactions=10`, a native
default `0` case, and an installed but unpreloaded product profile with `10`.
The `0` case is additional coverage, never a requirement. Product images have
no probe module; exact shared-library/LLVM identities and all 14 input files
were checked after the tests. These six profiles are retained without a stop,
restart or forced native cleanup. The older failed primary remains separately
quarantined and is outside acceptance. Rust/Cargo 1.96.0 ran serially on macOS
26.0.1 arm64 with scoped Rustup and target directories.

Both simultaneous late publishers retain gate RowExclusive while waiting for
native `pg_class` RowExclusive, with no global reference. A real SHOW waits
for gate Share outside global Share while normal prepared commit/rollback
completes. The finisher-history fixture acknowledges Parse/Bind/Flush before
its first Execute, then sends Parse/Bind/Execute/Sync without an intervening
Sync. Both outcomes complete before the caller's own late catalog wait; target
locks and effects are checked. Native cleanup failure preserves the native
error, rolls back metadata and leaves neither module tag held.

The bounded discovery sample reused the package fixture: 32 warm-up and 128
measured requests, one SET/SHOW round trip each. PG17 scanned 27 namespaces,
416 classes, 3133 attributes and 619 types; p50/p95/max were
460125/536459/658084 ns. PG18 scanned 31/416/3168/623; p50/p95/max were
429542/476708/562459 ns. The revised uncontended attempt takes four Share
acquisitions, two gate and two global, and four heap scans. Retries add work.
These sequential local samples do not establish causal overhead, proxy
throughput, cache hit rates or contention; phase budgets do not bound total memory.

Immutable receipts are under
`.darmok-work/logs/publication-gate-{957d119,4fe5b2e,7d7fe8e}-*`.
The actual-0 author audit `publication-gate-7d7fe8e-16-author-audit` has stdout
SHA256 `fd2c2f8232d290fccefc67109ec642e352a30e16d65bed7182f48bfee0c3f829`.
Its facts are
`9697aa337def846f1e1ffea0ed794a5d90bb80be184adb7882f9b4d20f7c3448`;
the 1488-file manifest is
`71bc535724259fede7da358ebe73fcd9d3db5217a1deb5a2edf20c5e5cfd6934`.
It rehashed 16 current and eight historical command receipts, their 144
source readers, 188 nested setup/environment actions including the original
74-command setup, all 14 native inputs, pinned primary sources and the prior
334-file architecture and 58-file original source seals. The historical
fixture exit 101 and failed normal cleanup connection exit 2 remain preserved.

Independent finite core review is under
`.darmok-work/logs/review-publication-gate-7d7fe8e-v1`. The report has SHA256
`acf70388944a6f98b861fba3258ecafbe13dd5ce644d335fc111910983e028ee`;
facts `fb3173f49ea7ead13e8f790451ed6a1e9baf4cc9c2bda563397de679982e0dab`;
121-file seal `b62842fb88ec9df6cd03150b0211d4b70ef5c35d4dab64d7e44d302466e5790f`;
separate invocation actual 0,
`b440102fb515798d904692dd728144a34130452f6a1dc19e13503ee91f37dc8a`.
The reviewer independently verified the frozen author graph, raw current and
historical receipts, exact native frame traces, profile cleanup and input
identity. It found no remaining production blocker within the documented
builtin-core/private-reader scope. F1 statement-name reuse is corrected and
exercised; D1's stale two-acquisition wording is corrected in this documentation
leaf. The reviewer's earlier passive pathname failure remains preserved with
the corrected reader and final audit's actual-0 results. No reviewer runtime
was executed. A separate leaf audit verifies that only these three documents
change and all executable/test/dependency and native inputs equal tested source.

This establishes finite publication/utility-completion and one-shot observation
components under their documented profiles. Complete dependency guards,
supported preparation and statement admission remain open in #46. MySQL
read-view/lock equivalence, table execution, serving, real-driver/cache/
performance/artifact, hosted CI and release gates remain open. Security,
compiler PR #4, stress/forced interruption, remote CI/account actions and
release publication remain excluded; the goal remains active.

### Private native interlock source selection; historical draft checkpoint

PR [#50](https://github.com/samrat-shamim/darmok-proxy/pull/50) was normally
squash-merged to `b9869aefb31eef5c9753666a0954aec03388818d`, tree
`6cc4f3628381470ef4295682d5c01087be352f6a`, from reviewed leaf
`d5f41668464a1463eadee8dc05a750b510720466`. Issue49 is closed; issue46 remains
open. Historical publication acceptance cannot certify a new semantic tag.

`feat/native-statement-guards` drafts the
[private C interlock](native-statement-guards.md): semantic reader S, publisher
RX, compatible prepare-coverage AS, exact owned reference cleanup, short
shared-drop semantic drain before intent and immediate reader postcheck without
CV sleep. Native concurrent 2PC remains enabled. There is no SQL lease/frontend
handle/application statement lane/executable-plan constructor.

Frozen proposals rejected a retained shared-drop writer for two static native
queue cycles. The refined short drain removes those edges conditionally. Native
readiness uses each actual prepared dummy under TwoPhase SH then every native
lock partition SH. Paired native startup finishes lock replay before primary
admission; this is not executed recovery acceptance. The withdrawn PID/VXID
classifier was unproven, including fast-path vs native VXID field distinctions;
it is not an established recovered-alias counterexample.

Root's immutable `.darmok-work/logs/native-statement-guards-field-matrix-v3`
contains 238 pinned unique sources, 154 exact full-function/startup excerpts and
152 actual-0 passive readers. Files seal SHA256
`dc2e5c24ad9ecf574e015db8c4eac03ccd5362effc0b350c5dbaaf55ac17d1d6`;
recorded outer `native-statement-guards-b9869ae-10-matrix` actual0 stdout
`940cb3120504b7e4bba2fe6dde04f08bd1910fd755dae734ee4ad6765b664c1c`.
The authored field matrix SHA256 is
`b3f2c1ff39fa74808de61815e327f902cc353b92385effc41bec0303a92063c6`,
ownership proposal
`1f5714043686f98ea5062ef64e7d31a5d933810f8181da5c9f8ad6ef7bc158f6`.
An earlier extractor no-match1 omitted the PostPrepare_Inval callee; its helper,
partial folder and actual1 outer remain frozen, followed by distinct corrected
and expanded audits. Passive incorrect-path/no-match navigation is not runtime
or product evidence.

Independent proposal reports retain all prior failures/static findings:
`review-native-statement-guards-prepared-readiness-b9869ae-v1` report SHA256
`d44a170e5d1f48bd19e8f39cd4f198ed0c06a560e93aa95b16522b502862da35`,
201-file seal
`9cd799ba8b959c8202624c823f5f44f6f578f8540d88d17f204c895e0800e57e`,
separate actual0 invocation
`d5e6c6f658c03d503fdd569313c7922e1b59805dc7ccc1fb5be44a918bafe9cf`;
`review-native-statement-guards-field-b9869ae-v1` report
`65ad6524aa3cce77d0954632377422db281283dc8eb55b16bf160027dc771562`,
223-file seal
`4a3485162faab150c9d04f51bfb3b7ff325ed1caffd2331ac17a7b8dcd3aeb54`,
separate actual0 invocation
`c4efcb35c0c94149320de5c77cd46d4b55054ab5babc36b0e8fcf9907e923bb8`.
These are external proposal/core-source reviews, with early clean and later
draft-dirty states distinguished; neither accepts implementation/runtime.
Required owner switching/restoration, parent promotion/TRANS_INPROGRESS reentry,
segregated test-only lifetime and volatile automatic tokens/unwind counts are
implementation contracts requiring fresh code/evidence review.

At this draft checkpoint, compilation, meaningful PG17/18 fixtures, environment
cleanup and scoped checks were pending; their current results follow below.
Physical/negative/overload closure, neutral preparation,
callback admission, Describe/materialization, MySQL read-view/lock equivalence
and rollback-capable execution remain open. No recovery/proxy-throughput/cache/
artifact/hosted-CI/release gate closes. Standing exclusions and the active goal
remain unchanged.

### Private native interlock local verification

The tested clean source is `22ce51bc5710de72bc4a14084571d0898f682fb5`, tree
`6366a257ce98c1d62319bc4e5b0bc6f72d787493`. This verifies the private C ownership,
semantic publication interlock and per-prepared-dummy coverage check under the
documented builtin-core profile. It does not admit application statements.

| Check | Command and executed result |
| --- | --- |
| Full native PostgreSQL package, 17/18 | `cargo test -p darmok-postgres-tests --locked -- --nocapture --test-threads=1`: 131 passed each, 0 failed/ignored, 17 summaries each; includes 18 publication and five guard fixtures |
| Private-owner discovery subset, 17/18 | `cargo test -p darmok-execute --lib --locked native_backend::tests::native_catalog -- --ignored --nocapture --test-threads=1`: three passed each, 0 failed/ignored, 86 other tests filtered |
| Formatting | `cargo fmt --all -- --check`: actual 0 |
| Repository boundaries | `python3 scripts/check_repository.py`: actual 0, nine packages |
| Strict workspace Clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: actual 0 |
| Strict connector Clippy | `cargo clippy -p tokio-postgres --lib --locked -- -D warnings`: actual 0 |
| Product/probe images | Four corrected `docker build` invocations at `9855a45f972b925c8de1044b80c885da01f1b076`, all actual 0; all 15 native inputs equal tested source |
| Setup and final environment | 103 initial, 34 fresh-profile and 59 final-environment actions, all actual 0; eight current profiles healthy with zero prepared targets and coordination references |

Rust/Cargo 1.96.0 ran serially on macOS 26.0.1 arm64 using scoped Rustup/target
directories. Ten recorded Cargo/Cargo-spawning intervals do not overlap. Native
servers run 17.11/18.6 on Linux arm64/musl with the pinned PostgreSQL base digests
already recorded above. The PG17 product build reused identical cached inputs;
its successful build is not a claim that cached C was recompiled.

Each major has preloaded primary and ordered-callback test profiles with native
`max_prepared_transactions=10`, a preloaded native-default `0` case, and an
installed but unpreloaded product-image negative case with `10`. The zero case
is additional coverage, not a requirement. PG17 primary/ordered acceptance uses
fresh v2 profiles; its earlier failed v1 profiles are retained and not reused.
All four PG18 acceptance profiles and PG17 negative profiles are v1. The ordered
profile preloads the test probe before the product module. The two unpreloaded
product profiles contain no probe; the other six use test images. Final facts
bind exact image/library/LLVM/installed-header identities to all 15 inputs.

The five new fixtures exercise owned/stale tokens, nested error and
subtransaction cleanup, first and established snapshot neutrality, one-invocation
volatile-token cleanup, retained-reader commit/prepare rejection, publisher
ordering, both native prepared outcomes and pure DML coexistence. A synthetic
uncovered dummy cannot borrow another prepared target's coverage marker. Two
shared-drop cases verify old-reader drain and queued-reader yield while ordinary
temporary-backend close permits native retirement. They use bounded ordinary
SQL and client close, without forced native interruption. Synthetic coverage
removal proves representation/direct-verifier behavior only, not pre-install
or recovery execution or the once-ready path under a deliberately broken hook.

The cost helper sends LOAD, BEGIN, 32 warmup and 128 measured sequential scoped
SET requests, final status and ROLLBACK through one in-container TCP-loopback
connection. Each scope releases its volatile C token in PG_FINALLY. Native psql
timings have 1000 ns display quantization: PG17 p50/p95/max
6000/36000/109000 ns; PG18 12000/40000/85000 ns. Final status has no owned token
and false snapshot flags; this is not 160 separate status observations. There
is no baseline comparison, isolated C-call cost, throughput, contention,
physical/preparation/executor/cache or recovery acceptance. Native allocation,
cross-database exclusion and prepared lifetime remain real costs.

Original failures remain intact. The initial PG17 probe build at `4e5c503`
exited 1 because the native text-output helper required `utils/builtins.h`;
the corrected include retains strict warnings. The first guard suite at
`9855a45` exited 101, four pass/one fail: combining ROLLBACK PREPARED and DROP in
one simple-query request caused native 25001 at final cleanup. Corrected source
submits separate top-level requests. Both normal cleanup receipts are actual 0
with zero prepared/coordination counts. The failed-run quarantine note has
SHA256 `132c4bc600685189756896e9ff4a315d5bc1f338ec0b4f11405ccb93a7aacb0d`.
No stop, restart, signal or forced native cleanup was performed.

Immutable root receipts are under
`.darmok-work/logs/native-statement-guards-{4e5c503,9855a45,22ce51b}-*`.
The actual-0 `native-statement-guards-22ce51b-24-author-audit` binds 23 command
results and 970 verified paths, including all native inputs and the 196 nested
setup/environment actions. Its facts SHA256 is
`6449afab77138f21d700e8e17798d050b6aba9761de258eaeafc1abd5c236faa`;
67-file seal `f4626c03cd4a55edc4bdc42d70d3118b9afcc9e651f1c1999cb894c67d7e9763`;
outer stdout `4414a8861768fe613d93cb8659f356dc823aeab0699a83fc17ac87f0485dfe32`.

Independent original code review is frozen under
`.darmok-work/logs/review-native-statement-guards-9855a45-code-v1`, report SHA256
`6958da9745ec510686e07316474712cf9c69c6f31b7f20173f76fc8f3d9a77a6`,
175-file seal `661df7e86c70c69108fe691b1f96c4a8ffeb7aaf0c4ff39656c325587c17a810`,
separate actual-0 invocation
`98b1f5f182472f3e02f06ca906aedba13701146ede2a0058575849cca4c58935`.
F1 cleanup, D1 allocation wording and D2 release-predicate wording close only
at tested `22ce51b`. Independent closure/evidence review under
`.darmok-work/logs/review-native-statement-guards-22ce51b-evidence-v1` finds no
remaining blocker within this finite component. Its report SHA256 is
`14e2b40efa577cd883975ab861b5c2223ffe7a8f97195c498c8f2b31109a9c96`;
147-file seal `ddee534e4ffb2a6f7751dcbc38c9f359856b3f66cac63f965664a4382ebf14c7`;
separate actual-0 invocation
`39e3a93ef9d620e38327521a0d1a938accb618ecd6b203de2e090e0280fbebec`.
The reviewer's own first offline reader exited 1 by wrongly expecting probe
sidecars on unpreloaded product profiles; that failure is preserved. Its
corrected audit validates their distinct artifact contract and exits 0. All
author and baseline seals remain unchanged. The reviewer ran no runtime tests.

The final documentation leaf must preserve every tested executable/test/
dependency entry and all native inputs. Earlier full workspace tests remain
historical; no current full workspace test run is claimed. Complete physical,
name/negative/overload candidate closure, snapshot-neutral native preparation,
callback admission, Describe/materialization, MySQL read-view/lock equivalence,
rollback-capable execution, serving, driver/cache/performance/artifact, hosted
CI and release gates remain open. Issues46/15 and the goal remain active.
Security, compiler PR4/resources, stress/forced interruption, native-profile
restart/stop/signal, remote CI/account work and release publication remain
excluded.

### Native interlock merged; physical reference owner selection checkpoint

PR [#51](https://github.com/samrat-shamim/darmok-proxy/pull/51) was normally
squash-merged from reviewed documentation leaf
`daeb0820cca1d3e8806a75a000c91e14de7e478d` to
`7e0d3da410f32c87e83c5d929b925bef2e62f03f`, with matching reviewed tree
`5bf624434793bfb7525fcfcdeb1321cc85cebcbe`. All 329 other entries and all 15
native inputs equal the tested `22ce51b` revision. No in-scope PR remained open
at the publication audit. Issue46 remains open with an updated dependency and
neutral-admission gate; issue15 remains the separate MySQL data/lock gate.

The independent final-leaf review has report SHA256
`265857cd54e453cd3e21561a761cc0dff4eea9e235c6160b0b1e767cc34acf2d`,
68-file seal `8d1d77b089373844b17ccc3c0572841e339fca43105368ce969823640ec88071`,
and separate actual-0 invocation
`051d8352612c1011b799698d49b44e1e31ea542681b9a09eaa4aa5f2dde7db34`.
An additive correction records 330 unchanged entries for the earlier two-file
implementation delta; the original report's 331-entry typo remains preserved.
Root's final-leaf audit is actual0 with a 46-file seal
`17ed26c7eedc98b83be2a5358f22b4e834ec737bd5d10d4b6d95a66080e702d0`.
The normal merge receipt `native-statement-guards-daeb082-26-merge` is actual0;
the publication audit is actual0 with 13-file seal
`c74dbd34f8e972f05d29cb2df4868f125822c9adf7e7774268752cf65d13f071`.
It verifies personal/public ownership, the corrected repository description,
exact merged tree and filtered in-scope PR state without hosted CI/account
actions. Earlier helper/navigation failures remain immutable.

The next [private relation-reference owner](native-relation-guards.md) is
drafted from clean merged main. Its external V4 proposal has SHA256
`6b2cd3718114a51bfe57a2d5ebcbe53812b4fe2a4d6ea0fbed5b89b497f50395`.
It preserves ordered explicit OID/mode requests, exact native increments and
CurTransactionResourceOwner ownership; release removes only attempt increments
and retain leaves native transaction cleanup responsible. A native acquisition
ERROR requires matching native abort before component reuse. Failed-child veto
belongs at PRE_COMMIT_SUB, successful promotion at COMMIT_SUB. Exact local
cache clearing, complete closure and immutable execution remain separate gates.

Conditional independent proposal review under
`.darmok-work/logs/review-native-physical-attempt-7e0d3da-v1` accepts only that
finite design: report SHA256
`52d94ce5f8ccf047688de929ef0075732b90b8c9c4a1dab7c58ece01eaf8489c`,
222-file seal `c94d2bdaae148e7baf3296a3eb4974fd151224d1cd2e6ddf8b39a56ff7fc6231`,
separate actual-0 final invocation
`f54971393b04a28b81bf1cf65a00c3e567ae7e7b0a547678d3fa27bfccd68e62`.
Its 69 passive readers include 64 actual0 and five preserved navigation failures;
a first finalizer schema failure is preserved before a corrected new helper.
The new paired native shared-tag primary-source capture has nine-file seal
`dfa2790e93b2828aa1605e74d7a1e430fc4fdf94acb155c19154d05e05748ec6`,
with actual-0 outer `native-physical-guards-7e0d3da-01-shared-tag-source`.
These are source/design results, not compilation or runtime acceptance.

Physical references must precede semantic Share, and semantic/raw exclusion
must end before tuple/MultiXact/XID execution waits. Static ordinary mixed
DML/DDL source ordering rejects keeping semantic S through row execution;
no hanging or forced-error experiment ran. Native 2PC remains enabled.
Fresh product/probe images and ordinary PG17/18 fixtures are required because
native inputs now change. Earlier profiles cannot certify this draft.
All broader closure/preparation/execution/release gates and standing exclusions
remain open; the goal remains active.

### Private native relation-reference verification

Tested clean source is `c0438c7882a197227c147fe4b86e2c67bddbdd5d`, tree
`fc64e517dfeaedc8767b1c252ce980112ba07a58`, based on merged PR51. This verifies
the [private ordered reference owner](native-relation-guards.md), with no SQL
registration, frontend handle, closure proof or application execution entry.
Concurrent native PostgreSQL 2PC remains enabled.

| Check | Exact command and observed result |
| --- | --- |
| New native relation group, 17/18 | `cargo test -p darmok-postgres-tests --test server_relation_guards --locked -- --nocapture --test-threads=1`: four passed each, 0 failed/ignored |
| Full native package, 17/18 | `cargo test -p darmok-postgres-tests --locked -- --nocapture --test-threads=1`: 135 passed each, 0 failed/ignored, 18 summaries; includes publication and statement-interlock groups |
| Private-owner subset, 17/18 | `cargo test -p darmok-execute --lib --locked native_backend::tests::native_catalog -- --ignored --nocapture --test-threads=1`: three passed each, 0 failed/ignored, 86 filtered |
| Formatting and repository boundaries | `cargo fmt --all -- --check` and `python3 scripts/check_repository.py`: actual0, nine packages |
| Strict Clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` and `cargo clippy -p tokio-postgres --lib --locked -- -D warnings`: actual0 |
| Native builds | Four new strict product/probe Docker builds against the pinned 17/18 base digests, actual0; GCC shared objects and LLVM units compiled at tested source |
| Setup/final environment | 111 setup and 67 final read actions, all actual0; eight healthy fresh profiles with zero prepared/coordination/mock relation references |

Rust/Cargo 1.96.0 ran on macOS 26.0.1 arm64, with the scoped Rustup and target
directories and 11 nonoverlapping recorded Cargo/Cargo-spawning intervals.
Native servers are Linux arm64/musl PostgreSQL 17.11/18.6. Four profiles per major
cover primary preload and ordered test callback with `max_prepared_transactions=10`,
the additional native-default zero case and an installed but unpreloaded product
negative case with ten. Zero is not a serving requirement. Ports32831..32838
belong to fresh `darmok-physical-guards-{17|18}-c0438c7-*-v1` profiles. Product
negative profiles have no probe; test profiles use the separately built probe.
Installed headers/libraries/LLVM inputs and exact image identities bind all 16
tracked native inputs. Earlier profiles/results are historical.

The four fixtures establish shared/local native tags, multiple modes, borrowed
same-owner and parent counts, input-copy ownership, pre-lock validation,
stale tokens, child promotion/abort, scoped completed-acquisition ERROR cleanup,
retained native commit/rollback/prepare and both native prepared outcomes.
An acquisition waiting on prepared DDL holds the earlier declared higher-OID
reference and lacks all module tags, distinguishing preserved caller order from
sorted acquisition. Normal native completion unblocks it. Scoped caller ERROR
does not execute pending native-wait ERROR; the abort-required rule and semantic
unwind separation are pinned source proofs. No interruption/recovery experiment
or native session/transaction overlap-success claim follows.

The finite cost helper uses 32 warmup and 128 measured sequential scopes, three
ordered references including two modes on one OID, through one in-container
TCP-loopback psql connection. PG17 p50/p95/max 15000/37000/46000 ns; PG18
10000/23000/90000 ns, with 1000 ns display quantization. Final status has no
owned token and false snapshot flags. This includes wire/utility/wrapper/native
owner costs and is not a baseline, isolated C-call, throughput or contention
comparison. Normal native allocation and retained count accumulation remain.

Root receipts are under `.darmok-work/logs/native-physical-guards-c0438c7-*`.
Actual0 `21-author-audit` verifies 20 command results and 1232 paths, all 16
native inputs, 178 nested actions and preserved proposal/source-review seals.
Its facts SHA256 is
`93dfe561c808ef7f5aab98b31195beea02ce1ec49a97eeba1f047d31b48fe456`,
28-file seal `dacffcdf6762866950b127a0687be4f57540cd2d2f6722e3ab60b8619bd9fd89`,
outer stdout `bc60dd1e6dd1650fb3d0fb3d86bc29cfbd58806f96729e7738d0fdcf21df8e83`.
Nine author audit readers are actual0. All new builds/fixtures/checks are actual0;
earlier recorded failures remain unchanged rather than being recertified.

Independent source review under
`.darmok-work/logs/review-native-relation-guards-c0438c7-code-v1` has report
SHA256 `233dc46ee312edce537e08cef5d56d7899d8b0a4e49cafe2b2f40511da9f0ad3`,
136-file seal `ea1aa79323fd6fdbc6dc20ba93c42dca2ef6e4cbd0abff9807ec410561bd1668`,
separate actual0 invocation
`2d4ccac932a82549949fca74b156cb6efd8785c9a1314468cefc895d2271f54c`.
All 43 passive readers are actual0, with no concrete source blocker within the
finite admitted-caller contract. Runtime/evidence acceptance is a separate
review; the final documentation leaf must preserve every tested executable/
test/dependency entry and all 16 native inputs.

No current full workspace test run is claimed. Exact local cache clearing,
full physical/transitive/name/negative/overload closure, neutral preparation,
callback/provider admission, immutable execution, Describe/materialization,
MySQL read-view/lock equivalence, serving, driver/cache/throughput/artifact and
release gates remain open. Issues46/15 and the goal remain active. Security,
compiler PR4/resources, stress/forced interruption, profile restart/stop/signal,
hosted CI/account work and release publication remain excluded.

### Merged private relation-reference checkpoint

PR [#52](https://github.com/samrat-shamim/darmok-proxy/pull/52) was squash-merged
normally from reviewed leaf `5a60ef2e1227fe11638452bed187016f4a0730fb` into
main `296667c48e3b412e172359047ce8ebf5645761ae`, with tree
`4e084cd6421d1ca3bed0c0000d987098b350f3c6`. The two-document leaf preserves
all 333 other Git entries and all 16 native inputs from tested `c0438c7`.
Independent final evidence review has report SHA256
`aa923906fca742d6ce9a8926e3183b264a3b476a0408891839ff5ea42ee0e0f2`,
112-file seal `3f1283be9bb70332ae47e118ecf35104e0814ad1860a775908e21cf8ebd374bc`,
separate observed actual0 invocation
`4362700e40c2385e24ba0b4b80efd58a2ee6df3b0e20de63946495e61e29589e`.
Thirty passive readers and corrected offline audit v3 are actual0; its earlier
schema/navigation/builder bookkeeping failures remain distinct and preserved.
No reviewer runtime or full workspace acceptance is claimed.

Root merge/publication receipts under
`.darmok-work/logs/native-physical-guards-5a60ef2-{25-merge,26-publication-audit}`
are actual0. The latter verifies four GitHub reads, exact main tree, the personal
public repository, unchanged generic description and no in-scope open PRs;
PR4 is filtered before output. Its facts SHA256 is
`567efe703856f85d5771a6b0b0e64a75b2c3191f1efdf4aa535ac410bca6e4f8`,
13-file seal `61add4dba36f9b2f8df9a21e416005cbabf201d540233f025b5ca40288c5e0e3`.
Issue46's updated remaining gates were written and read back exactly while OPEN.
No hosted CI/admin/account or release action occurred. Issues46/15 and the
overall goal remain open.

### Native relation cache-clearing design and draft

The next isolated branch begins at that merged source. The earlier pure relation
count path cannot establish native LOCALLOCK clear state. The selected change
delegates ordered exact declarations to native `LockRelationOid` under the
recorded transaction owner, before module exclusion. Core owns SI recursion
and exact `MarkLockClear`; a copied manual LOCALLOCK refresh pass is rejected.
Mutating module reentry is denied during acquisition/cache dispatch. Any grant
or subsequent SI ERROR remains abort-required with no guessed native decrement.
Native two-phase transactions remain enabled.

Immutable external proposals v1/v2/v3 remain preserved. Final v3 SHA256 is
`564707f9d621e7c2d894047e3165d4a3f21824d183ca8d4b42646ad3827a16d7`.
Independent design-only review of clean `296667c` has report SHA256
`0c92f137d6e5898e90ccbc82b678866bd382cb0efc97d1fc7ddb036f8e8f0b6a`,
213-file seal `66c1232ac5ff4875093255b40cbff62a0c338572314eff632db7c17d6176e907`,
separate observed actual0 invocation
`3d2556a92db502b2de54569062894b7a96d6d0e7d3e19ea962b13c02e87c5eb7`.
All 56 passive readers are actual0; the first source-selection failure and
partial copies remain preserved before the corrected 22-file capture.
Root's separate 22-source paired rehash has facts SHA256
`6a6ad1fdd22a5af789b7e5d5fdd4bf2fa0b737da76c7946449b7123688b49446`,
seal `874880d544f9435ebffff021fa597149a55613b1172f42ed29bc8f8c14f6b64b`;
`native-cache-clearing-296667c-01-primary-rehash` is actual0. These are
design/source results, not compilation/runtime acceptance.

The two new ordinary fixture drafts check exact native clear-result/borrowed
owner state and warm builtin relcache refresh after both prepared outcomes,
with first-unselected and established data views. The warm helper releases its
own AS count, setup OIDs come from an observer, and passive callbacks do not
mutate or issue SQL. Native clear is per exact mode and may skip SI processing;
it is not global freshness, full closure, callback neutrality or preparation.
Fresh strict product/probe images and affected PG17/18 fixtures remain required.
All broader gates and standing exclusions remain unchanged; the goal is active.

### Prepared-abort fixture oracle correction

At draft `e6fce687a9955e723bc3eb2eea0c0decd1479375`, four strict native
product/probe builds and Rust native-package compilation are actual0. Eight
fresh profiles bind all 16 native inputs; 112 setup actions are actual0.
The ordinary PG17 six-fixture group is actual101: five passed and the warm
fixture failed unconditional positive SI/callback assertions after its first
prepared ROLLBACK case. Its earlier prepared COMMIT case passed. The observed
rollback had zero acquisition invalidations/callbacks, restored owner, unchanged
first-unselected flags and the previously committed warmed shape. This is a
fixture-oracle blocker, not native component acceptance or an established
production defect. No error was injected, no test was ignored, and the original
raw receipt remains under `native-cache-clearing-e6fce68-08-relations17`.

Paired native `REL_17_11`/`REL_18_6` `twophase.c:1621..1628` sends saved SI only
inside `if (isCommit)`. A prepared abort never publishes that private definition.
Corrected immutable proposal v4 SHA256 is
`425ea7ce92d700e0844ccbcab72e441f5ee33424deb369a870c13f79b2610cac`.
The separate finish-source proof has facts SHA256
`62d602289195a46035739e0598175b707612c48ef949011183f592d4ef1550ac`,
three-file seal `55101903db5edc84f042fbc36d3cb82cd364636f0e691eafc16502cca928cf8f`
and actual0 outer `native-cache-clearing-e6fce68-09-finish-primary`.
The independent source reviewer identified the same fixture blocker against e6.

The corrected Rust oracle requires positive target invalidation/callback
observations for COMMIT only. Both outcomes still require exact native clear,
owner/snapshot/no-exclusion wait completion and the expected current shape.
Rollback need not have zero global SI because unrelated messages can arrive.
No synthetic callback or product fallback is introduced. Only Rust/docs change;
all 16 native inputs must rebind exactly before reusing fresh e6 images/profiles.
Corrected-source runtime and independent closure/evidence acceptance remain
required. All broader gates and exclusions are unchanged.

### Verified native exact-mode cache clearing

Tested corrected source is `e829f50745ea9eb2de1531444c2e04fb9c2eac81`, tree
`bdf9e5689dcfa1355bd08807c6b6be48aa600990`, on an isolated feature branch from
merged main `296667c48e3b412e172359047ce8ebf5645761ae`. Native relation acquisition
delegates each copied ordered declaration to `LockRelationOid` under the recorded
transaction owner. Native core owns SI recursion and exact per-mode clear state.
The phase rejects mutating module reentry before any module publication exclusion.
A native grant or subsequent cache-refresh ERROR is abort-required, without
decrementing a guessed partial list. Already-clear mode state does not establish
globally fresh catalog facts or complete statement protection.

| Current check | Observed result |
| --- | --- |
| Strict product/probe PG17 and PG18 builds at `e6fce687` | All four actual0, native GCC/shared and LLVM inputs compiled |
| Corrected relation fixtures at `e829f507` | Six passed, zero failed/ignored on each major |
| Complete native package at `e829f507` | 137 passed, zero failed/ignored across 18 summaries on each major |
| Private-owner discovery at `e829f507` | Three passed, zero failed/ignored, 86 filtered on each major |
| Formatting, nine-package repository boundaries, strict workspace and connector Clippy | Actual0 |
| Setup, exact-input rebind and final environment | 112/67/67 nested actions, all actual0; eight healthy profiles |
| Three-reference bounded costs | Actual0, 32 warmups plus 128 measured scopes per major |

The native builds bind the same 16 product/probe inputs at both source revisions;
only three Rust/docs entries change in the corrected commit. Four fresh images
have IDs `9e8742ae9731915053fd26ba8697e235ccd8a632f95b93298d658ffc217ba5d5`
(PG17 product), `82f6d54c545f1c741de70eec339a729452cbc62fa631b80b561a1d12f8951a17`
(PG17 probe), `f8cd4300fee793f53d4eb758d80cd3cffb256d237aae578a29b19fd8e0db990d`
(PG18 product), and `d69bf5eb6dc4d51fa8b04774aacf6073466824d0762063e03e7d70a158d57032`
(PG18 probe). Pinned base digests are
`b0f9560a2de083e2cc7382e75f808c7381a32852a7ec49117deedb300e552b24`
and `77f585114c32fbca283dc835b0596f4e52b51b4c6662d7810b2f4084f60a1873`.
Rust/Cargo 1.96.0 runs on macOS 26.0.1 arm64; native servers are Linux arm64/musl
PostgreSQL 17.11/18.6. All Cargo/helper chains retain the scoped Rustup and
target paths. Fifteen recorded Cargo/Cargo-spawning intervals do not overlap.

Profiles `darmok-cache-clearing-{17|18}-e6fce68-*-v1` use ports32839..32846,
with exact unchanged library, LLVM, installed header and image identities.
Primary/ordered and unpreloaded product profiles use native maximum ten;
the additional zero profile is not a v0.1 requirement. Product negative profiles
contain no probe. The final environment facts SHA256 is
`000c37e97924cb42e90a1ae7c80112cc513c2943b0c2e4f195f509932ea5ce4c`:
all eight healthy profiles have zero prepared transactions, module/mock relation
references and real relation-fixture tables. No profile was stopped, restarted
or signalled; existing fresh native images were reused only after exact rebind.

The six fixtures retain the four ownership cases and add native exact-mode
clear/borrowed-count behavior plus warm builtin relcache completion. The sampler
does not dispatch SI or mark clear. The warm descriptor releases its own AS
increment before prepared AX, observer-only setup avoids selecting an early
reader data view, and both normal prepared outcomes unblock acquisition without
any module tag. Only COMMIT requires positive target SI/callback observations;
both outcomes require current shape, exact clear state, restored owner and
unchanged first-unselected/established flags. Passive callbacks neither mutate
nor issue SQL. Continuous builtin caller restrictions still apply.

Original source review preserves F1/P2 OPEN at `e6fce687`, with report SHA256
`2f009650a2be907be170ff9f10ca1020b3937f74e3c8c9080af23345d195a445`,
176-file seal `80bb736a3b7664570580671a157cb9aed5a62f796bb3868f23664223c6889f53`,
separate observed actual0 invocation
`bc3b77479c5439eb5ad02b6651fd3c6f7a73f0b3fd19f75f9ed855991d9bafa5`.
Corrected-source closure under
`.darmok-work/logs/review-native-cache-clearing-e829f50-closure-v1` closes F1
only at `e829f507`, with report SHA256
`1b843b1331c23d23002b77159d544bcae5ac4138f906958eebf4f3b364d07297`,
119-file seal `605e645eb094fd3a3c4f27efc9b81720f7edd0a8490c71a3249dadf96e850b92`,
separate observed actual0 invocation
`fcde85b8eef58bd76bad3b8dbbba96467f0dc8f0ef09acebaf1f685a20e6d275`.
Its 36 passive readers are actual0; shell-glob and pre-child builder failures
remain separate. The earlier source review's failed filename/builder/recorder
attempts and all design/primary seals remain unchanged. Neither reviewer ran
native runtime or Cargo checks.

The first corrected-source owner17 command is also actual101: two passed and
the third failed because `DARMOK_TEST_UNPRELOADED_DATABASE_URL` was omitted.
Its failure occurs before that fixture connects. `15-owner17-v2` supplies the
existing unpreloaded profile and passes all three; owner18 passes likewise.
The original `08-relations17` actual101 and this `15-owner17` actual101 remain
raw failures, rather than being recertified. No product fallback or test ignore
was added. Three passive root inspection/navigation schema errors are documented
separately and are not counted among zero recorded commands.

Current cost p50/p95/max is 11000/25000/47000 ns on PG17 and
13000/26000/86000 ns on PG18, with 1000 ns psql quantization. One in-container
TCP-loopback connection sends three ordered references per scope, including two
modes on one OID. Protocol, utility, test wrapper, passive callback instrumentation,
native SI, native lock and owner-allocation costs are included. One final 16-field
status reports no token, restored caller owner and false snapshot flags; unrelated
nonnegative SI counters are permitted and no per-sample status claim follows.
There is no baseline, isolated C timing, throughput or contention comparison.

Root receipts are under `.darmok-work/logs/native-cache-clearing-*`.
Actual0 `native-cache-clearing-e829f50-24-author-audit` verifies 24 recorded
results (22 actual0, two preserved actual101), 1689 paths, all 16 native inputs,
246 nested actual0 actions and all current source/design/proposal seals.
Its facts SHA256 is
`bdb9fc46e9c9fd09cb239896e7932a247aeb7981ae8e547fa0110007d3e8c093`,
31-file seal `d4554ac0af6768631e1b8c1c9fc84da7de1c689ec85ff73e1c5e611b74901036`,
outer stdout `6ab25046c1d58b900866a80fe008bae2885422136dafba43999330149429d462`.
Ten author audit readers are actual0. A final two-document leaf must preserve
all other tracked executable/test/dependency entries and the same 16 native
inputs, then receive a separate independent full evidence/leaf review before
normal PR merge.

No current full workspace test run is claimed. Forced native-wait/callback ERROR
execution, globally fresh SI facts, complete transitive/catalog/application/index/
TOAST/storage/name/negative/overload closure, neutral preparation, callback/provider
admission, Describe/materialization, immutable execution, MySQL data/lock behavior,
serving, driver/cache/throughput/artifact and release gates remain open.
Issues46/15 and the goal remain active. Standing exclusions are unchanged.

### Native README correction and historical evidence

The first documentation checkpoint is `12a92f17e18ea5602c1bf367f023ace4330c4d09`,
tree `eef696e26abad45d709eaf11b642fc66d3e58c6c`, directly after tested `e829f507`.
Its actual0 `25-documentation-leaf` proves two changed documents, 333 other
identical Git entries and all 16 unchanged native inputs. The checkpoint's facts
SHA256 is `92a7c304f7681750b388ccabebc97dbf7fb91a82671143cba44286360b6a9884`,
43-file seal `46a98c2346f89dec1728023421171a80b17e6d5e2a19acc43acb6680bc569399`.
That statement applies to this historical checkpoint, not the later README edit.

Independent full evidence review under
`.darmok-work/logs/review-native-cache-clearing-12a92f1-final-v1` is finite
functionally positive but withholds documentation acceptance for D1/P3: the
packaged native README still said cache clearing was under verification.
Its report SHA256 is
`75b6211229122246976a9e5720f3ad2ee2368d09b2471a62d1007def6c5e54c3`,
facts `781103a20300179f27e1094880f505b72c1d89d8812aac4046fa6b17554b193c`,
122-file seal `00edcc2618509f1b10395098e07082d858a4e4588324f9264664da3579f82ec2`,
separate observed actual0 invocation
`35a92dc6ddd3a6c4b05881a4033a52c7fc23ca663ab5c192bc1b765c6e1b6d49`.
The frozen full audit verifies the complete native/image/profile/cost/source
graph, all 24 results, 246 nested actual0 actions and 15 serialized Cargo/helper
intervals. Thirty-two passive readers are actual0. The reviewer did no native
runtime, Cargo, profile lifecycle, network or hosted CI work.

Two finalizer actual1 attempts remain preserved: first a redundant rehash used
the old manifest's advancing README working pathname after live reads had been
released; next a strict reference count omitted the additional tracked
`scripts/check_repository.py` helper. The final historical seal witnesses all
17 tracked references through pinned Git bodies and rehashes 1793 external
files normally. It does not reinterpret either actual1 as source/runtime failure
or recertify the corrected working copy as tested source.

The correcting leaf changes only this ledger and
`postgres/darmok_server/README.darmok_server.md` from `12a92f1`. The README now
states finite local PG17.11/18.6 verification at tested `e829f507` and keeps
full freshness, closure, callback admission and neutral preparation gates open.
Compared with the tested revision, three documentation entries change while all
332 executable/test/dependency and other Git entries remain identical.

The README is one of the 16 native build-context inputs and is installed through
Makefile `DOCS`; it is not a compiler/link input. The other 15 inputs, native
objects/headers/SQL and test code are unchanged. Existing test images retain the
old packaged README and their historical 16-input manifests. The new source
documentation is not claimed byte-identical to those complete image inputs.
An explicit pinned `e829f507` Git-body witness must verify the old README entry
when auditing historical graphs; the new working pathname must never silently
stand in for that tested byte sequence. No new binary/image/profile/runtime
check is claimed for this documentation-only correction. Separate independent
correction acceptance is required before normal PR merge; D1 remains open in
the historical `12a92f1` report. Broader gates, exclusions and the goal are unchanged.

### Merged native cache-clearing checkpoint

PR [#53](https://github.com/samrat-shamim/darmok-proxy/pull/53) was normally
squash-merged from reviewed correcting leaf
`e2325a5d9924e842f5b29fa2b43d42ad490b335e` into personal public main
`5184b30b99eb9333c9ee78e169a8c5e05d23c3a5`, with tree
`0c3a411bd1c7ef34fd3f2051ffedbe5630fec7c7`. The implementation/test/dependency
entries remain identical to tested `e829f507`; the final README DOCS distinction
above remains explicit. Existing native test images retain their historical
packaged documentation. No new image or runtime result is inferred from merge.

Independent correcting-leaf acceptance has report SHA256
`8316bd17e125d5f3eb6335200229e8c58173fc9b99da3988eaf81fe66911e7d5`,
facts `f48d44bef67ddd219218cf77c954a3d3a9c2df4d7ca9934fae00ccd8640d3dfd`,
75-file seal `36bea3f3a17d44f6f55e174390daa5de6355f1926c98067933ea07be530ed842`,
separate observed actual0 invocation
`9005dbd72233ac91871ddf3e6a3e6f17b841ef6943aa35142965c7a2d3adb3c4`.
Twenty passive readers and the offline correction audit are actual0. D1 closes
only at the correcting leaf, with no new finding; historical D1/F1 and actual
failures remain unchanged. The separate pre-write builder actual1 is preserved.
Root rehashed all 75 members, outer streams and finalizer; its rehash record
SHA256 is `1dc1babcb31599363d1bf4eb5cd8dad6046ef673935ac884428a4ccb7dfee11e`.

Root `28-merge` and `29-publication-audit` are actual0 under
`.darmok-work/logs/native-cache-clearing-e2325a5-*`. Four GitHub reads verify the
exact main tree, personal PUBLIC repository, unchanged generic description and
no in-scope open PRs; PR4 is filtered before output. Publication facts SHA256 is
`3792e7125a27d6e93664ac70f7975143d6655e9633d92b063ecd581a728617d8`,
13-file seal `899d5980c78754daf1ea46bbbb924e9f6232471c03640370b341f00fb06ea021`.
Issue46's checkpoint and remaining gates were updated while OPEN, with unchanged
title and exact body readback. Its body SHA256 is
`0841bcab712bd21c60ce265759574d97ac276adcda7076e278331df42ae96c60`,
update facts `cb3cc551ba86f2cf9a2fef7b19b31049b85d83de8d5948221e56c6a6181b7dc1`.
The original component worktree is frozen; the next isolated branch begins at
this merged source. Full physical/transitive/name/candidate closure, fresh facts,
neutral preparation, immutable execution and all other broader gates remain
open. Issues46/15 and the overall goal remain active; standing exclusions persist.

### Verified private catalog-declared heap storage

This isolated branch begins at merged main
`5184b30b99eb9333c9ee78e169a8c5e05d23c3a5`. Checkpoint
`bded47601a74d6b705c9b0a0fcc573084de28ac6` records the preceding PR53 merge.
On 2026-10-03, the private whole-invocation C boundary in
[native-heap-storage.md](native-heap-storage.md) completed finite local
verification and independent implementation review at
`2640513033227cfe00ad5be4511b12e85491c09c`, tree
`389b60efc8b412b9887d91f1d2bb20176254d9a9`.

The accepted scope is counted literal schema/relation bindings to ordinary
builtin heaps, all live builtin-btree indexes and catalog-declared TOAST storage.
Duplicate root bindings and separate exact AS/RS/RX increments are preserved;
the complete declaration is deterministic and parent-first. Actual TEMP/shared/
tablespace identity and real mapped file numbers are checked. Other relation
kinds/AMs and foreign TEMP fail explicitly. Traditional inheritance hints do not
expand children. This graph describes the metadata point, not future
data-derived `va_toastrelid` or rewrite `rd_toastoid` opens.

Every observation uses the no-CV try-fence, including histories with earlier
borrowed/retained references. Initial scans/snapshot/descriptor increments close
before physical waits. Fresh SI, installation checks, final descriptors and
native mapping resolution precede semantic S. A final fixed-field observation
rechecks literal definitions, graph, generation and private/context identity.
Only a source-admitted pure C copying consumer runs with S and physical ownership;
raw references have ended. S ends before callback-capable native reader cleanup
and exact physical release/retention. Entered-invocation ERROR remains native
abort-required. No SQL/frontend storage entry point, admitted plan, arbitrary
provider/descriptor callback or data execution is supplied.

The selected immutable design is
`.darmok-work/native-heap-storage-attempt-proposal-v4.md`, SHA256
`72a9c4111183785bc972377401d77ce30595006f81a77269796c2f4d907fc349`.
Its historical conditional design acceptance is under
`.darmok-work/logs/review-native-heap-storage-attempt-bded476-design-v4`:
report `02d4f29ee929fc606a8f9ffd9c8832abd4677b3f20cc6cdb299a3146e50b4a13`,
facts `f874fb639a10b4949786e258e034b3077720f8ca29d7a156104a9f1747f028ff`,
209-member seal `8e82ea4ca481ed8c37cdb65062866924a45ceefd50cfb98c5722a5f7c5eabcd7`,
separate actual0 invocation
`15c284c448f30234c71096c5c4db5293d137739b59d6f6d6510e413a9ba3d9d1`.
Earlier v2 F1/D1 remain in that historical report; v4 corrects every observation
to a try-fence and distinguishes periodic allocation checks from peak bounds.
Design acceptance alone did not certify the implementation.

Paired pinned PostgreSQL REL_17_11/REL_18_6 primary bodies and HTTP/source
receipts remain frozen in the dependency-closure bundles and
`.darmok-work/logs/native-heap-storage-primary-v1/verified-sources.json`, SHA256
`9177ae30e6bc9819cd79cad9e62c49955b92a008d3c42cbf35898430a6195184`.
Actual native namespace identities, all-live index enumeration, mapper lookup,
TOAST opens and concurrent-index stages were checked against those bodies.

Two actual failures are preserved. The initial clean
`74a94d8d6192c54a32d544084ce9a8c129b9961b` PG17 product build exited1 under
strict native shadow warnings: nested unsuffixed PG_TRY generated locals shadowed
the outer PG_TRY locals. Matching documented `_cleanup` suffixes in the inner
PG_TRY/PG_FINALLY/PG_END_TRY fix that cause without suppressing warnings or changing
cleanup semantics. Both installed native `elog.h` bodies confirm the optional
matching-suffix contract. The initial clean
`589ad108f9af01f56acfb6f6ee3c6c765872c13f` storage fixture run exited101 with
zero of six groups passing: PostgreSQL JSON OIDs were strings while C OIDs were
numbers. The correcting `2640513` oracle uses explicit SQL bigint casts. No
product delta or heuristic string coercion was needed. The original receipts,
raw streams and source identities are never relabeled as successful checks.

Four strict GCC/LLVM product/probe image builds are actual0 at clean
`aacfe42cab42f6f0b4b08a3e866a9cb3e6485ab7`, tree
`913df666034f87d73aa5adeead73dd17366a9af5`, under
`.darmok-work/logs/native-heap-storage-aacfe42-{01-product17,02-probe17,03-product18,04-probe18}`.
The commands are `docker build --progress plain` with the native product context,
then the separate probe context and `DARMOK_PRODUCT_IMAGE` set to the built
product. PG17 uses explicitly pinned
`postgres:17-alpine@sha256:b0f9560a2de083e2cc7382e75f808c7381a32852a7ec49117deedb300e552b24`;
PG18 uses the Dockerfile's pinned
`postgres:18-alpine@sha256:77f585114c32fbca283dc835b0596f4e52b51b4c6662d7810b2f4084f60a1873`.
Exact argv, timestamps, source-before/after and raw streams are in each receipt.

| Major | Product image SHA256 | Probe image SHA256 |
| --- | --- | --- |
| 17 | `cd31229e9bebceb39293e7b2dc242f6a9b2980fa229b1f133b96a104b5aa6d73` | `56ee9c2d19a76521b7bbd99b715fd05711cf8d7d55ee4fe50867481338ba64fa` |
| 18 | `5e294e3e3b1910ad3ff7d53c6837d6ca6577ae2b6c6becdf2b28d6ed83630c2f` | `53a57b59e12a6dd18117eaf0cf42ab8cc5adebd77d0b72d9709386832d93a051` |

All 20 native Docker inputs (14 product, six probe), including the installed
README DOCS input, remain byte-identical at tested264. Product images contain
the three module bitcode objects/index and installed headers, with no probe;
test images add two probe bitcode objects/index and the probe library. Saved
installed source/header/library hashes bind each running profile to its image.
The input-binding receipt `589ad10-07-input-bind` is actual0; its facts SHA256 is
`f4c0daff7dc1bbe373504e67f686d4d17ab5212218ad71e28c020e66503d7bfd`.
Subsequent changes to264 affect only Rust fixture/registration inputs.

Eight new profiles `darmok-heap-storage-{17|18}-aacfe42-{primary|no2pc|unpreloaded|ordered}-v1`
remain running/healthy at host ports32847–32854. Primary/ordered profiles use
native `max_prepared_transactions=10`; the zero-setting profile is only a required
negative dependency. Concurrent native two-phase transactions remain supported,
with no serving requirement to disable them. Setup facts under
`.darmok-work/logs/native-heap-storage-aacfe42-profile-v1-actions` have SHA256
`29ae45b808549ac958c606dc7d497bc59c83c612d8d33c3c72c6d620a1afc419`;
all120 nested actions are actual0. No existing native profile was interrupted.

Candidate264 commands are serialized through the scoped Rustup/shared target
recorder. Native environment variables select the four explicit dependencies
per major; disposable test URLs and exact argv are captured in local receipts.
Under `.darmok-work/logs/native-heap-storage-2640513-*`:

| Receipts | Command | Result on each major |
| --- | --- | --- |
| `09-storage17`, `10-storage18` | `cargo test -p darmok-postgres-tests --test server_heap_storage --locked -- --nocapture --test-threads=1` | Six passed, zero failed/ignored |
| `11-package17`, `12-package18` | `cargo test -p darmok-postgres-tests --locked -- --nocapture --test-threads=1`, with the recorded ordered-profile variable | 19 binaries, 143 passed, zero failed/ignored; includes the six new fixtures |
| `13-owner17`, `14-owner18` | `cargo test -p darmok-execute --lib --locked native_backend::tests::native_catalog -- --ignored --nocapture --test-threads=1` | Three explicitly selected required groups passed, zero failed/ignored |
| `15-format` | `cargo fmt --all --check` | Actual0 |
| `16-boundaries` | `python3 scripts/check_repository.py` | Nine packages, actual0 |
| `17-clippy` | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Actual0 |
| `18-connector-clippy` | `cargo clippy -p tokio-postgres --lib --locked -- -D warnings` | Actual0 |

The six new groups cover literal UTF8/quoted/63-byte/duplicate roots and exact
three-mode counts; release, native retention and child abort/promotion/commit/
prepare; real mapped/local/shared/global and own TEMP identity; explicit foreign
TEMP and unsupported-kind/AM errors with normal abort; prepared DROP/recreate
commit and rollback while physical readers wait without module/catalog references;
and ordinary live invalid/not-ready and ready/invalid concurrent index phases.
First-unselected and established repeatable-read snapshot flags are checked.
Native catalog/path/pg_locks observations are separate from pure consumer token
flags. All cases use normal completion and cleanup, with no forced native-wait
ERROR, stress, interruption or recovery claim. No current full workspace test run
is claimed; earlier totals do not certify this leaf.

Finite ordinary costs (`19-cost17`, `20-cost18`) use one TCP loopback psql
connection,32 warmups and128 sequential calls, one heap/two indexes/declared
TOAST/index (five nodes), three duplicate-mode roots and13 exact references.
Every call includes both full three-heap observations, graph comparison, SI/
descriptor/native owner/lock work and pure JSON copying. PG17 p50/p95/max are
327500/638000/3223000ns; PG18 315000/398000/511000ns. Printed psql quantization is
1us and only the final copied status is inspected. No isolated C timing,
allocation count, comparative baseline, throughput/contention or full cache/
executor/performance acceptance is inferred.

The final `21-environment` audit is actual0, with77 nested actual0 commands;
facts SHA256 is `e20a6475f5ab67a00a5eae0a0f52835ba17fe804b0f1cc9ac7472edf6acc1314`.
It binds all20 native inputs and eight profile/image/installed identities, pinned
native versions, host/Rust/Cargo versions and zero checked prepared/module/mock
references and named new/old fixtures. The author fresh source pass and actual0
`22-author-audit` bind1178 sealed files; facts SHA256
`9d2caefcb37ffffd37814685fe279feec05c0788ca36a85b4728b9c6ff5c56ac`,
seal `737ac45a55ad41ae9084302494ad768e1a8643728a06ab817a5341250f3410c2`.
Source/path reader mistakes and a truncated inspection remain bookkeeping
failures/limits, distinct from recorded native checks.

Independent implementation acceptance under
`.darmok-work/logs/review-native-heap-storage-2640513-implementation-v1` has no
open finite-component finding. Report SHA256 is
`8aec8a8ab1c4892d009c42ca5e53262667ec45639c780435e70e57a699f5f645`,
facts `dd0c4572dcd02038c0225608fbbd7d2cf970d0d0573acffab796517da92852b5`,
1352-member seal `ce951de38e3cce6e123a7db56257a1d81aa6727f9b4612d1c7a8b37e1f9a229a`,
actually executed final invocation
`901fe2bb68795c5742b7187a369bdb87c7d5686e17bea7a3c24c980e18d6d657`
and independent post-rehash invocation
`a94252170f69d49484a834de17180996524dd4bb8ac30ee60065c79bd856169f`,
both actual0. It verifies22 nonoverlapping recorded results (20 actual0 plus
the original build1/fixture101), all1178 author members, the209-member design
seal,20 exact Git/native inputs,120 setup/77 environment actions and raw paired
six/143/three counts. Its45 pre-final passive commands contain43 actual0 and two
preserved schema-assumption actual1 results; all21 Git commands are actual0.
The separate pre-write quoting builder1 and partial recorder1 remain preserved;
no unrecorded child exit is fabricated. End and post source checks are actual0,
and live reads were explicitly released before this status update.

Root `23-review-rehash` is actual0 and rehashes all1352 members plus separate
final/post invocations, streams, helpers and source checks. The root rehash
record SHA256 is `1a876f82bd55071e0876454fe260b49edba784861cd8df098320b0a426f78873`.
The following final leaf changes only this ledger and the unpackaged storage
contract. All executable/test/dependency entries and all20 native inputs,
including the packaged README, must remain identical, with historical edited
doc bytes witnessed from pinned Git rather than substituted from advancing
working paths. Separate independent documentation acceptance is required
before normal squash merge. No new binary/image/runtime result follows from
the status edit.

Full type/default/check/rule/trigger/domain/FK/partition/function/operator/
collation/provider and negative/overload candidate closure, data-derived TOAST/
rewrite opens, invocation effects, immutable IR, snapshot-neutral native
Parse/Bind/Describe/reanalysis, table/data execution, MySQL row/current-read/
savepoint lock equivalence, serving, driver/cache/throughput/artifact and release
gates remain open. Issues46/15 and the overall goal remain active. Standing
security/compiler-PR4/hosted-CI/stress/profile-interruption/account/admin/release-
publication exclusions remain unchanged.

### Merged private heap-storage checkpoint

PR [#54](https://github.com/samrat-shamim/darmok-proxy/pull/54) was normally
squash-merged from independently reviewed leaf
`790abd678f80a26b06d71742d5a5af71bea011c2` into personal public main
`8361877170c2cdd475e2c7fb136467c059e1d1d5`, tree
`c4cf053353e5a44fd3b3ecca4abf8b4faf17e9d5`. Its two-document status leaf
preserves 339 other tracked entries from tested `2640513` and all 20 native
inputs from built `aacfe42`, including packaged README DOCS. Existing native
images and their tests are not recertified merely by the merge.

Independent documentation acceptance under
`.darmok-work/logs/review-native-heap-storage-790abd6-docleaf-v1` has report
SHA256 `a43ca49a910d239e054daf1ada6e288a374aa5a084bf1faaf59f8507c544b07a`,
facts `1dfc9bdf2957311bff52064589df2978da963cf14feaeaba62dc798ffe3199d5`,
1500-member seal `24303e49bb7218c97b7b797eaaaa05674a5d0fe4dc5a7f2aab463759e0fa6949`,
separate final invocation
`edac34c51ff1a4709af30ecc2201ca40ff7184192eed115fc723a7267d09bc66`
and post-rehash invocation
`6f1a5a88e2762d62f5b4be522d6903b945db20695efc8e42202b6efb83b74ec0`,
both actual0. Twenty passive commands, including thirteen Git readers, are
actual0, with no correction needed. Historical implementation/design failures
remain immutable. The advancing historical document uses exact tested Git bytes.

Root `25-docreview-rehash` is a preserved bookkeeping actual1: relative post-
reader names were resolved against process cwd instead of the review folder.
The new exclusive `26-docreview-rehash-v2` is actual0 and verifies all 1500
members plus separate executed final/post records, raw streams, helpers and
clean source checks. Its root record SHA256 is
`61c528a0a4633bec26708adbd9d2523f378d0e1676a1644068be84a39c5a2eaf`.
No product, source or runtime correction follows from that reader mistake.

Root `29-merge` and `30-publication-audit` are actual0 under
`.darmok-work/logs/native-heap-storage-790abd6-*`. Four GitHub reads verify the
merged head/tree, personal PUBLIC repository, generic description and no
in-scope open PRs; PR4 is filtered before output. Publication facts SHA256 is
`388ea9a8ef71007e19d9c1f20c67a47dc06951cc88ec9bdcf843aafc9ae3a353`,
13-member seal `4ac815b54f664ea676bb5cbacc6c167b828e654c513c913c73f87fa165330b84`.
Issue46 remains OPEN with unchanged title and exact body readback; its updated
body SHA256 is `bd825cd96ed3f2516432e45de2cff641351847b914bad9d29e5c7a9b8ed09847`,
update facts `0e8ef544cb649831b38d34c92ed6dd7279351c672233c56c55b37cdab61a0e30`.
Fetch/worktree receipts `32-fetch` and `33-next-worktree` are actual0, leaving
the component worktree frozen at the reviewed leaf. The next isolated branch
begins from merged main. Full semantic/candidate/physical closure, neutral
preparation, effect/immutable-plan admission, execution and MySQL data/lock
gates remain open. Issues46/15 and the goal stay active; exclusions persist.


### Fixed column/type facts design checkpoint (implementation pending)

Public base is PR54/main `8361877170c2cdd475e2c7fb136467c059e1d1d5`.
The isolated `feat/native-semantic-closure` worktree is clean checkpoint
`23842c4ca8f25f98eb879dca570a2044c4332e7b`, tree
`2acfe8ebe6c1375fc00fd1e6c0aac076301b56f9`, before this specification append.
No native input, test or executable change has yet been verified for this lane.

The primary capture command is `record-catalog-discovery-command-v1.py
native-semantic-closure-23842c4-01-primary <worktree> 23842c4... python3
capture-native-semantic-closure-primary-v1.py`, actual0. Scoped RUSTUP_HOME
is `.darmok-work/rustup`, CARGO_TARGET_DIR is `.darmok-work/native-values/target`.
`logs/native-semantic-closure-primary-v1/verified-sources.json` contains 90
pinned REL_17_11/REL_18_6 bodies (45 each, 52 cached/38 HTTP200 fetched), SHA
`be0d555554e377a582e623f5429455f6a4350a6e782ac254685e40715f2ea484`;
39-member seal SHA
`79e8c802277b89599f61f4f73c0d2d7e845351eb956659ba91bbaa31a7023f88`.
Capture is provenance, not full-body semantic or runtime acceptance.

Immutable external proposal v1 (12976 bytes) SHA
`1b11ef5cd2a9364925537391f321643e9d10930bc3d4b0c0cee3a20291ebdff2`
received finite conditional design review from `/root/native_values_review`.
`logs/review-native-attribute-type-facts-23842c4-design-v1/report.md` SHA
`eee2814716f1f9a6e6421d2f9222c6480b10d1e17cb97a06b46ad2fd64fdeba0`,
facts SHA `91dd57faf16bd1153679de98ba18e498f087f95dd13197ea2e1f880d35f9da39`,
263-member seal SHA
`5de0e494f8448cbcca18db5ddbe587b3e9a17f749f9ae5e7ba7ba599083f82b3`.
47 passive readers0, no own failures; independent finalization/post check0.
Live reads released. Required C1 distinguishes true live layout redundancy
(attlen/attbyval/attalign) from legitimate column overrides. P1 recommends exact
root→positive-attribute→live-type filtered copies in one coherent snapshot/raw
span while retaining full scans. Paired fixed headers and complete native
TupleDescInitEntry/BuildDescForRelation/ATExecSetStorage/RemoveAttributeById and
RelationBuildTupleDesc/RelationBuildDesc/RelationInitTableAccessMethod paths
support the finite declarations and expose excluded variable/provider work.
Root `02-design-rehash` actual0 verified263+companions277 unique files; root
facts SHA `fc9544398aa4e5c19919e2e5d29333d552652c811e46d5671248536738d06867`.

New exclusive proposal v2 (16367 bytes), SHA
`fdb494febc10315183e42c12415236c27ea4d1a8e38bc1f7d9063f7ac2b0a29c`,
closes C1 at design stage and selects P1. It separates scanned rows, selected
array bytes and context allocation, and adds ordinary override, builtin-like
custom type, removed dropped type, repeated type and cold/warm cases. V1 and
all prior evidence remain unchanged. Independent v2 finite delta review found
no further required design correction. Review folder
`logs/review-native-attribute-type-facts-23842c4-design-v2`: report SHA
`c4f2a38b173754b039d34d08e02f44436d0e7a2d482be468c0d1db301a45fc50`,
facts SHA `b35846a875ffa44869c62df670fec126da97bc9fa4b108b37f72cdef6ef933ae`,
60-member seal SHA
`ca69fc34e7da686d7991dc043746961aff4fe8b2d05ef30b865e61bbb45c583a`.
10 passive readers0, 19 selected anchors rehashed, no own failures; separate
finalization/post check0 and clean23842c4/tree2acfe8e. Live reads released.
Root `03-design-rehash-v2` actual0 verified60+companions74 unique files;
root facts SHA `f946d718e4ddd4693317aee7bbdbe4ca058cf37f553b6d8b290fdda0c8eaa156`.
Both root chains use the actual-exit/raw-stream recorder; no runtime gate passed.

A root passive inventory included an absent `docker` path and surfaced rg's
error, although the display pipeline returned0; it is not verification evidence.
A subsequent repository-wide verified filename inventory returned0. No source
or native behavior was changed for that inventory mistake. The source capture,
review and recorded root rehash commands above all actually returned0.

The selected specification is `docs/native-attribute-type-facts.md`. It extends
the existing private whole-invocation metadata point, with positive slots and
direct fixed type declarations only. Five descriptors prepare before exclusion;
all initial reader increments close before physical waits. Full semantic closure,
variable/provider/name/effect/immutable-IR/preparation/data/executor/lock/serving/
release gates and issues46/15 stay open. Concurrent native two-phase transactions
remain in scope. No full-goal completion follows from these design reviews.


### Fixed column/type facts first implementation verification (not final acceptance)

Implementation `828692023861fdc31a9989658c00f9bb8dab89ad`, tree
`dec7fba8990ac02028cb54fb014f2321baf43546`, follows specification `fbba702`.
The private header/C boundary now copies positive root slots and direct fixed
live-column types under one registered five-heap observation, compares explicit
defining fields, preserves column overrides/type0 dropped layout and exposes
separate scan/selected-array/context costs. Pure hash construction rejects
missing/duplicate identities while observing; complete cardinality/layout/type
namespace/graph validation stays outside raw. Such copying ERROR follows normal
abort-required cleanup; it is never a completed false retry. This clarification
retains the prior raw hash invariants and involves no variable/provider dispatch.

Recorder `record-catalog-discovery-command-v1.py` SHA
`27fc25286818869bd1e43351627620b59828833cc7e3a2d1bc04f3338de67122`
records clean exact before/after source, actual argv/scoped RUSTUP_HOME and
CARGO_TARGET_DIR, raw streams and child exit. Evidence folders use prefix
`logs/native-attribute-facts-8286920-`: `04-grammar` captures both pinned native
grammars0; `05-native-compile`0; strict product/probe PG17/18 builds
`06-product17`,`07-test17`,`08-product18`,`09-test18`0. Both native compilers and
LLVM bitcode keep warnings as errors. Twenty exact product/probe inputs include
packaged documentation. `10-profiles`0 binds all20 inputs and four image IDs;
127 setup commands0. Fresh eight profiles use ports32855..32862. Primary/ordered
native maximum is10; disabled2PC is only a negative dependency. Installed native
headers and packaged README match source. Existing native profiles stay running.
Setup facts SHA `19d1655b39ff43bd8e5d1c4983a26806beb5c9f798ac3b7f98ff93c8f2cf0257`.
Paired grammar facts SHA
`20d6cae0ab7781493838502b8d3de46d22b2d5e4ec306cdd07361a8fff8510fb`;
three-member seal SHA
`346dee7094d3a27f43aa95310d91bb5575ca842399b5f84174efe1bacfe5d535`.
REL_18_6 ConstraintElem uses `NOT NULL ColId` and NOT VALID; no parenthesized
column syntax or guessed effective nullability was used.

At8286920, `11-storage17`/`12-storage18` each pass9 ordinary fixtures,
`13-package17`/`14-package18` each pass19 native binaries/146 tests with zero
failed/ignored (including those9), and `15-owner17`/`16-owner18` explicitly select
and pass3 private-owner checks each, zero failed/ignored and86 filtered out.
`17-format`0 and `18-boundaries`0 (9 packages). These are exact8286920 results,
not current/future-leaf acceptance. New ordinary variants cover fixed-field
SQL oracles, empty/duplicate ranges,63-byte UTF8/quoted/renamed/dropped slots,
removed custom types, builtin-like names and shared type IDs, valid storage/
compression/collation/typmod/dimension/declaration overrides, default/missing/
identity/stored/virtual-generation flags, PG18 invalid NOT NULL with existing
NULL data, initial/warm invocation, own TEMP and both prepared ALTER/type rename
outcomes. Native physical waits hold none of the five catalog reader increments
or module fences. Complete native suites remain the pre-existing ordinary
regression matrix, not new stress/signal/error-injection experiments.

Strict workspace/all-target/all-feature Clippy `19-clippy` actually exits101;
raw stderr SHA `88c633697fe779411174468a6e0db5c3e972cebce21daa05eaee6f1f84644879`.
Rust1.96 Clippy flags the new test's manual `bytes % count == 0` divisibility
predicate. The nonzero-count branch can express the same invariant through
`bytes.is_multiple_of(count)`; use that native predicate without suppressing
warnings or removing the assertion. Native product/probe inputs do not change.
The original actual101 receipt and streams remain immutable. Fresh current-leaf
fixture/static/cost/environment and independent implementation review remain
required. No full workspace test, full descriptor/SQL/admission/executor/MySQL
lock/serving/release gate or full-goal completion follows.

A passive receipt inventory pattern for an assumed `native17` suffix returned
no matches1; a broader known-lane inventory identified `package17` and returned0.
This was filename discovery, not a product/runtime check. No missing path was
read, source was unchanged, and no native failure was relabeled.

### Fixed column/type facts reviewed verification and inherited-column finding

Clean tested `32b4e29a0be641541b6b738bdbaba9a1f3b1e2e3`, tree
`99a30581f292487a5010cd8e5db6578da2e192a0`, corrects only the test divisibility
predicate and preceding ledger relative to built `8286920`. Exclusive input
binding proves all20 native product/probe inputs, including packaged README,
equal the built Git bytes and setup hashes; binding facts SHA
`1cc1c5ead1de22be37bf069547a586fcfe8cdff69c8aea556d17369652fcda40`.
No old receipt or failed Clippy result is modified.

Recorder prefix `logs/native-attribute-facts-32b4e29-` records source/scoped
environment/actual argv and exits: `20-input-bind`0; workspace all-target/
all-feature strict `21-clippy`0; `22-format`0; `23-boundaries`0 (9 packages);
strict tokio-postgres library `24-connector-clippy`0. `25-storage17` and
`26-storage18` each pass9 required fixtures; `27-package17` and `28-package18`
each pass19 binaries/146 native tests, zero failed/ignored (including the9).
Explicit `29-owner17` and `30-owner18` each pass3 owner checks, zero failed/
ignored and86 filtered out. These are32b4e29 results, not later-leaf acceptance.

`31-cost17`/`32-cost18` wrap new exclusive ordinary measurement bodies under
`logs/native-attribute-facts-32b4e29-cost{17|18}-v1`, actual0:32 warmups and128
sequential SET calls per major, one TCP-loopback psql connection, five storage
nodes/thirteen exact references/two selected positive slots/two actual types.
PG17 p50/p95/max are525500/701000/835000ns; PG18 are499000/747000/1031000ns.
Timings include both five-catalog observations, graph/SI/descriptor/native
owner/lock and pure-copy/protocol work. Psql prints1us quantization; the even
sample median has500ns granularity. Final-view attribute/type arrays are208/
416 bytes; each observation context is1204320 bytes on17 and1237088 on18.
Five scanned-row counts remain separate. Only the final view is inspected;
these are not peak/process/allocation-event/throughput/contention, isolated-C,
baseline or full-performance measurements.

`33-environment`0 records85 successful commands with eight exact healthy
running profiles, unchanged image/container IDs and installed native headers/
libraries/bitcode/probe/packaged README. Checked prepared/module/mock refs and
old/new fixture tables/schemas/types are zero. Primary/ordered native2PC remains
10. Final environment facts SHA
`347e1877b8f3de9d5fb5987bb05dbb4fff0962c6c952a37849c2c34e80ab0d69`.
Fresh root source inspection covers the entire C/header/probe and976 fixture
lines. `34-author-audit`0 seals1457 members, facts SHA
`cd99a3d59f2842fc1443bd8dd35f714a606b04ea42df746417c562019ea4369e`,
seal SHA `0074c30215d73cfc5103dfecc670b3f5f68d58632fce6be274f8f320c0eae45e`.

Independent frozen implementation review is
`logs/review-native-attribute-facts-32b4e29-implementation-v1`. Report SHA
`94879cafac6671ab26d19c79467ff25e72c4c20dcd6d625fb98038bb90d565bb`,
facts SHA `03a701313d8c77118005534ce4e1ffb12096717a184bd52ed058132149273d82`,
1762-member seal SHA
`cf22b08e5c493e5e180c0cc5fc6cc64804ff5b62cf7a6b69735723a36f94bb26`.
Actually executed `42-final-invocation.json`0 SHA
`6b33b1538e70b20ebebbed25aa4eb192ad578eef74efb70885b93ea719e214c1`
and separate `43-post-invocation.json`0 SHA
`9a73d3d17ffae3e6ae4a05a504b51a43048b5b18803e8ceb0de3927a4323ea76`
retain three clean Git readers and post facts SHA
`dcff3e3e90e748406f7de28971cff3c712ff32d78a561b56b326b34e312cc3d8`.
Own passive reader failures128/128/1 remain preserved; corrected reads pass.
Root `35-review-rehash`0 verifies1762+companions1777 unique paths, root record
SHA `2151249066a1400b6e235a868b1092d383edfc480d168668b128683203b03f9d`.

Review identifies no C defect, closes C1's layout/override requirement and
confirms P1's selected copying, but withholds final component acceptance for
required C2/P2 inherited-column coverage. The existing fixture creates a child
yet selects only its parent; selected positive slots all had local=true and
inheritance_count0. The next correction selects that actual child without
parent/descendant expansion, asserts nonlocal/positive inherited declarations,
compares complete native column/type oracles and checks fresh/established data
views and release ownership. Current corrected-source tests and independent
C2 closure remain pending. No metadata implementation change follows from
this coverage finding, and no earlier evidence is relabeled as acceptance.

A root ad-hoc commands-manifest schema peek returned1 because the JSON object
contains reader arrays rather than being a top-level array. A key-aware reader
returned0. This is a reader bookkeeping error, not a native/review result;
source was unchanged and recorded full rehash35 actually returned0.
Full closure/effects/preparation/execution/MySQL data/lock/serving/release gates,
issues46/15 and the goal remain open. Standing exclusions are unchanged.


### Verified private fixed column and direct-type facts

Clean tested `1934034e28d7fda64953c1693104b248c37c42ea`, tree
`f33cce9026bba70c196073d7229668b175489488`, adds positive inherited-child coverage
to the independently inspected implementation. Both fresh unset-snapshot and
established RR paths select the exact child, assert local=false and positive
inheritance_count, compare complete storage/root/attribute/direct-type native
oracles, reject parent/descendant expansion and observe no remaining checked
physical/coordination references before ordinary COMMIT. C2 closes here only;
the old `32b4e29` review remains unchanged with its historical finding open.

Recorder prefix `logs/native-attribute-facts-1934034-` records exact clean source,
scoped RUSTUP_HOME/CARGO_TARGET_DIR, actual argv, exits and raw streams:
`36-input-bind`0; `37-storage17`/`38-storage18` each pass 9 required fixtures;
`39-package17`/`40-package18` each pass 146 native tests across 19 binaries,
including those 9, zero failed/ignored. Explicit `41-owner17`/`42-owner18` each
pass 3 private-owner checks, zero failed/ignored and 86 filtered out.
`43-format`0; `44-boundaries`0 (9 packages); workspace/all-target/all-feature
strict `45-clippy`0; tokio-postgres library strict `46-connector-clippy`0.
No current full workspace test total is claimed. Original `8286920` strict product/
probe GCC/LLVM builds remain the native artifact evidence. Input-binding facts
SHA `f93f130cbe3e42d9e0d259d7d5fe77a7bae2ab65121232b9ab23fcd933a1407e`
prove all 20 native inputs at `1934034` equal built `8286920` and original setup.

`47-cost17`/`48-cost18`0 preserve 32 warmups and 128 ordinary sequential SET calls
per major under `logs/native-attribute-facts-1934034-cost{17|18}-v2`.
PG17 p50/p95/max are 670500/969000/3103000ns; PG18 are 884000/4521000/8794000ns.
Five storage nodes/thirteen exact native references/two selected slots/two
actual types yield 208/416 defined array bytes. Each initial/final observation
context is 1204320 bytes on 17 and 1237088 on 18. Full scan rows are, separately,
namespace/class/index/attribute/type: 17 has 160/420/167/3147/619 and 18 has
162/420/166/3182/623. Timings include both five-catalog observations, graph,
SI/descriptor/native lock-owner and pure-copy/protocol work with 1us psql
quantization; only the final copied view is inspected. The historical `32b4e29` timing
run stays immutable. Variance is visible; no cause or performance improvement
is inferred. Allocation events, peak/process memory, throughput, contention,
isolated-C/baseline and full performance acceptance remain open.

`49-environment`0 records 85 successful actions and eight healthy exact profiles,
unchanged native images/containers/libraries/bitcode/headers/probe/packaged README
and all 20 tested inputs. Checked prepared/module/mock refs and old/new fixture
tables/schemas/types are zero. Primary/ordered native 2PC remains 10; no profile
lifecycle action occurred. Environment facts SHA
`058216105187746d5aa8e83332fe6e13e3fcc13e2eeb581e39b47b41c8b06362`.
`50-author-audit`0 seals 2270 members; facts SHA
`20d168abdbdac76ab5cb759a204b71ad196b0dfd618ff1b1fafcebfe416e7a14`,
seal SHA `6a06c6e362af215d2d6e4e77dabd4a48222f5ab5c85e665bbf66bf540003a93e`.
The old 1777-path root review is preserved with two exact Git `32b4e29` source witnesses
for its changed test/ledger paths, rather than current working-path bytes.

Independent correction review from `/root/native_values_review` is
`logs/review-native-attribute-facts-1934034-correction-v2`. Finite implementation
accepted; C2/P2 closed at `1934034`. Report SHA
`0c7dabedbd06b1e892465788ad0a0343dc6eef82d9fa704da7865f05603834cc`,
facts SHA `4048fcb8d60abb8b066c2f8f19f9137db3ade6c7bac34baf929f49aac2007884`,
2528-member structured seal SHA
`cfe8ab52d07fc24030f6d497f5a233dfe88b478def3134a316633ecdf013bc08`.
Actually executed `20-final-invocation.json`0 SHA
`190e505ca91165d94ff9fccb1ce80a0f5d31cfbf8d97fd7578cc7f328dc6fcfd`
and independent `21-post-invocation.json`0 SHA
`675c26d4a7cac77585a1da95279788cf2ca003eacd17889c7f985075c98f12c0`
retain exact clean source and post facts SHA
`08fe2d8b99c78e9f0f02a0e6a22408116545d004ae6f4663a7e5824d234f11f8`.
73 prefinal passive records preserve 72 zero and audit 15 exit 1 (a mistyped
historical evidence-folder prefix); distinct corrected audit 16 exits 0. Final/post
source readers exit 0, live reads released. No reviewer runtime execution occurred.

Root `51-correction-rehash` actually returns 1: its checker assumed the old plain
seal map, while this new manifest wraps 2528 entries in `members`. That original
helper/receipt/stderr SHA
`d1e0579cca09cac7f62119d64bf68a009c618856cbc2762e98b5fdbe538462b8`
remain unchanged. A distinct schema-aware `52-correction-rehash-v3` actually
returns 0, verifies 2528 sealed members plus companions: 2550 unique paths and
preserves the failed 51 record. Root facts SHA
`a5c8612351e640682c1f28d8810d5364b3f8b34e09985888722c4c7ec72950ab`.
Prior Clippy exit 101, reviewer exits 128/128/1 and root bookkeeping failures stay historical;
none is relabeled as success or a product/runtime defect.

D1/P3 requires a status-neutral packaged README pointer after finite acceptance.
The final leaf updates that DOCS/build-context input plus the three unpackaged
status/ledger documents. Relative to tested `1934034`, all executable/test/dependency
entries and 19 other native inputs remain identical; existing tested images keep
their historical README. Do not claim all 20 final-leaf inputs match old images.
A separate exact documentation-leaf audit and independent readback must close
D1 before normal squash merge; the PR records final leaf/tree/publication proof.
No runtime repetition or fresh final-leaf artifact certification follows from
this documentation-only correction.

Acceptance remains limited to fixed selected positive slots and direct types
inside the private whole-invocation metadata point. Variable/transitive/provider/
candidate/effect/immutable-IR closure, native preparation/Parse/Bind/Describe/
reanalysis, data-derived storage, row execution, MySQL data/read-lock semantics,
serving, cache/full performance and artifact/release gates, issues46/15 and the
goal remain open. Security, compiler PR4/resources, stress/forced interruptions,
profile stop/restart/signal, hosted CI/account/admin and release publication
remain outside current scope. No full milestone or goal completion is claimed.

A draft documentation spacing edit surfaced trailing whitespace through
`git diff --check`; the multi-command display ended0 on its later status read,
so that display is not a successful diff check. The whitespace is removed
before commit and the separate diff check must pass. No runtime result changes.


### Merged private fixed column and direct-type checkpoint

[PR #55](https://github.com/samrat-shamim/darmok-proxy/pull/55) is normally
squash-merged. Reviewed final leaf `a493635c3ac9480e425a76f3a06e65f141219b5d`
and public main `6c2b03cf5de5215ed36db9dd5f6ffd63b4c5b9a6` share tree
`d63349455c8d6f474c3843fe1607e6ddb7d49529`. C2 is closed at tested `1934034`;
D1 is closed at the separate final documentation leaf only. Historical reports
remain unchanged. The finite accepted boundary still supplies descriptive
fixed positive slots/direct types, not an executable statement.

Independent final documentation review is
`logs/review-native-attribute-facts-a493635-docleaf-v1`: report SHA
`41f1ee5532c3fa1a8bcc634ecf7e35f0262729b1f84eac13b7de1b68bd504c36`,
facts SHA `47f2f08172ef3a604b6237d61e94126333081a8c7be90a520c6aa85ae0f57ffb`,
2805-member structured seal SHA
`833409f1f43d1274529cddce5efa5f4a9618f3e5e981fa7d3a98bcfb5e9490af`.
35 prefinal readers and bootstrap pass, no new failed executed reader.
Actual `15-final-invocation.json`0 SHA
`242f6cfc33020d934064fb714c128e2fed9169ac0905d5f9baadce6adaeed78e`
and independent `16-post-invocation.json`0 SHA
`b702affb664a94ee54537e53b44721b849ffcc9334db5b671552c0d97cb21a7b`
retain exact clean source and post facts SHA
`c521f2186a8933b579bd6b06803662c78058e8b6db0a6162015b3d856bd01c68`.
Live reads released. Root `55-docleaf-review-rehash`0 verifies 2805 members plus
companions: 2823 unique paths; root facts SHA
`fb03c27114196456f16c4c2e90fedff026a8cff8ca1544fabd882d19f6040b06`.
Four exact Git `1934034` documentation witnesses preserve advancing historical paths.
Nineteen native inputs and all executable/test/dependency entries are unchanged;
the sole changed native build-context input is the packaged DOCS README.
Existing tested images retain their historical README. No runtime repetition,
current full-workspace test or fresh final-leaf artifact claim is made.

Recorder prefix `logs/native-attribute-facts-a493635-` records actual
`56-push`0, `57-open-pr`0, `58-pr-head`0 and `59-merge`0. Merge uses squash and
exact --match-head-commit, with no admin/auto/delete-branch action.
`60-publication`0 independently reads merged PR/main/repository/README and the
filtered eligible PR queue: same tree, public personal repository, generic
description, no experimental README wording and no eligible open PRs.
Publication facts SHA
`63f6e2410adf7547367e95b5a81b9fad14746de73b2dbe1a14af52835ee71eae`,
22-member seal SHA
`e371d88f8230ffd1698b0197d420935ceee3efeea0693fd332c2f21eac827fd4`.
No hosted checks, CI retries or release publication occurs.

`61-issue 46-v6`0 verifies exact before/edit/readback and leaves issue 46 OPEN;
body SHA `e83cb713c548528fe5baed971f18e079ee0d9c67a357252371dde25c2a8dc15f`.
Issue 15 remains OPEN. `62-fetch-main`0 and `63-next-worktree`0 preserve the old
reviewed branch and create isolated `feat/native-definition-closure` at public
main `6c2b03c`, outside the dirty monorepo/personal main. Old source and evidence
remain frozen. Original Clippy/read/helper failures are not relabeled.

Next design work concerns variable catalog payloads and transitive/provider/
name closure. Copying fixed flags/OID edges cannot justify native descriptor
construction, expression parsing, providers or arbitrary detoast/storage opens.
Paired pinned source must establish snapshot neutrality, physical ordering,
variable payload provenance, provider/effect boundaries and complete cleanup
before selecting a mechanism. No implementation or runtime acceptance follows
from this checkpoint. Full dependency/preparation/immutable IR/execution/MySQL
read-lock/serving/cache/full performance/artifact/release gates and the goal
remain open. Concurrent native two-phase transactions stay supported; security,
compiler PR4/resources, stress/forced interruption, existing-profile lifecycle,
hosted CI/account/admin and release publication exclusions remain in force.

### Proposed inline column missing-image boundary

The new isolated branch checkpoints the merged PR55 proof at `ab286db`, tree
`d99a126eafddeca7917c3e2d0e7572d119de7656`. No executable input changes yet.
Recorder prefix `logs/native-definition-closure-ab286db-` preserves
`64-primary` actual1: source capture uses the wrong common/toast_helper.c path
and receives HTTP404 after 12 cached rows and one successful HTTP200 body.
The failed manifest SHA is
`ad8874131f551f9bb197200a9dd78f948b44aacffce1ea797070fd3d3b67d851`,
raw stderr SHA
`7898a9445d1da9cf58627c0b553a3a19a18d62ccc533704e7e28a9d3edb86a08`;
the original helper, partial folder and receipt remain unchanged.

Pinned official Git tree inventories establish access/table/toast_helper.c on
both majors. Distinct corrected `65-primary-v2` actual0 verifies 48 primary
source records, 24 per major, with 25 cached and 23 HTTP200 fetches. The partial
successful body is reused only after byte/hash verification, not as a successful
complete v1 capture. Facts SHA
`53b8ad887e0e8484ab386765694ed75687c2e188c29bb90a4afddf50e23bf69f`,
30-member seal SHA
`7899b30d884fd6f253881c8385cfa17f707439a9e357c23126b842ec43d1dd35`.
Distinct `66-primary-v3` actual0 extends that inventory with native accessor,
descriptor and builtin compression sources: 60 records, 30 per major, 48 cached
and 12 HTTP200 fetches. Facts SHA
`ad5b7ff00d3292f5ac0b1a8eb5dffc640bb892e6c11051b66bd49c00cc0a274c`,
19-member seal SHA
`f8d0b3eaf20f7f452d9c5765306a98b027e3d2680525ff9233fbffa99f838a29`.

`67-primary-audit` actual0 rehashes the two manifests/seals, all 60 current source
records, exact tree path inventories, original failed helper/manifest and all
three outer receipts/raw streams/source reads. It preserves the failed 64 exit.
Audit folder `logs/native-definition-closure-primary-audit-v1` contains 28 paired
function excerpts and verifies 153 paths. Facts SHA
`aa1c94cf5aa1e6716b2d2bc8f73e86eb880e1186409fb9f8dff199749ae74416`,
29-member seal SHA
`52a1f07886c1dc755700ece9f2766392ddea25ae2828594ac123136180e7f270`.
Byte verification is not a claim that all 60 source bodies were semantically
read. Selected writer/accessor/array/normalization/compression and snapshot
paths were inspected. Source remains exact clean `ab286db` during these helpers.

[native-missing-values.md](native-missing-values.md) proposes a finite next step:
coherently copy actual selected inline attmissingval carriers with a checked
preprepared native accessor, normalize only initial owned bytes outside raw/S
and before physical waits, compare exact carriers and all fixed/identity/storage
facts at S, then supply opaque native images to the sole pure C consumer.
No default/expression/provider or external TOAST fetch is admitted. Actual
pg_attribute class/profile, physical target presence, external-tag rejection,
array envelope/type and preallocation expansion bounds must be proved in code
and ordinary fixtures. An independent design review precedes implementation.
All implementation/runtime/native-image/performance/release acceptance for this
proposal remains open. Existing fixed-fact test artifacts remain historical.
Concurrent native 2PC and every previously recorded exclusion remain in force.

### Inline missing-image design review and implementation candidate

The design is frozen at `9c9bbf4a823ec028b665859ed87b2e0a1f55d7dc`, tree
`5016d3beeeb57f225c0d11582f07276eafb54304`. Its independent review is a
conditional design recommendation only, with no structural blocker in inspected
ordinary paths. C1/P2 requires a fixture proving an old absent slot differs from
a later physically present explicit NULL before and after DROP DEFAULT. It is
not an implementation or runtime acceptance.

Recorder `logs/native-definition-closure-9c9bbf4-68-primary-v4`0 adds paired
tupdesc.c after verified path inventory: 62 distinct primary records, 60 cached
and two HTTP200 fetches. Facts SHA
`c984d232185fd7e82d26d57199cf78b32f50648502415d9e49bebe9422c83203`,
nine-member seal SHA
`d75d7a2fa86f14574ff76cd6246c12fca302b90f184512c47b90895e03149b6c`.
`69-primary-audit-v2`0 verifies 181 paths and 30 function excerpts, preserving
the earlier failed capture and separate inventories. Facts SHA
`69e70627363257f8cd025b14b3c6109d751bc09842498f52cfcc177b7c2f69ce`,
31-member seal SHA
`1e3150728802936e0fbed24605fa8883a8424728d141889f1a3c92169b453239`.
The PG18 assertion-build accessor classifier and DROP DEFAULT bodies also use
separately verified existing paired classification/default-removal inventories;
those are not relabeled into the 62-source bundle.

Review `logs/review-native-missing-images-9c9bbf4-design-v1` report SHA
`185fd2e47107b6d6a4a91716b4e34943a3d97f3a099c8662ee50e2fa08b3687f`,
facts SHA `c899ef8907f99a5dfd997cff3dcf2c11d55a877c2348e5d7c7464b5476d158d4`,
702-member structured seal SHA
`5227230191f05f27ef8ad51fb14c848cf3491f3a4dde926886c9a93decc90ac7`.
Executed finalization113 and independent post114 both exit0 with exact clean
source; live reads are released. Five own passive actual1 records and root64's
HTTP404 remain unchanged. Corrected reviewer audit112 verifies 645 paths and
16 Git reads0. Root `70-design-rehash` actual1 preserves its bootstrap stream
prefix assumption; distinct schema-aware `71-design-rehash-v2`0 verifies 702
members plus companions, 716 paths. Root facts SHA
`a51e10672562e85ef8c6c47e6bef4de0a34ecd23d9a16c14b96461324f1d3966`.
Wrong-filename/source-key/window passive reads remain failed displays, separately
from product verification. `72-next-worktree`0 creates
`feat/native-missing-images` at exact design HEAD and leaves the reviewed design
worktree frozen. Dirty personal main and the monorepo are untouched.

The candidate implements checked physical access to selected inline carriers,
native singleton-array normalization only after initial reader cleanup and
before physical waits, cumulative/requested allocation bounds, exact final
carrier equality, and opaque copying under the existing pure consumer contract.
Missing count, stored bytes per observation, normalized image bytes and record
array bytes are separate costs. No extra observation, round trip, provider,
external storage or element codec is admitted. Ordinary test-only oracles use
core array normalization and a bounded active-snapshot heap scan independently
of the product copier. Fixtures add C1's physical absent/present-NULL distinction,
actual short/four-byte/pglz/LZ4 forms, domain/enum/array/composite declarations,
inheritance/drop/default histories and nonempty normal retention/2PC outcomes.
Paired heap_form_tuple source confirms physical natts; fixture results remain
pending. The packaged README is part of the candidate's native build inputs.

Fresh strict product/probe builds, required paired native suites, source/input/
artifact/environment binding, bounded ordinary costs and independent
implementation review are required next. Existing native images certify only
their unchanged historical inputs. C1 runtime closure, full descriptor/value/
default/TOAST/provider/effect/preparation/IR/execution/MySQL row/read-lock,
serving/cache/full performance/artifact/release gates, issues46/15 and the full
goal remain open. All previously recorded exclusions remain in force.

The native candidate is committed at
`d0f607ec4861fabe64f362da933199c1590a7f0d`, tree
`7af99eb2ffabfbf612c2b0789859348f621cfbf0`. Recorder prefix
`logs/native-missing-images-d0f607e-` records `01-product17`, `02-probe17`,
`03-product18` and `04-probe18`, all actual0, with GCC `-Werror`, LLVM bitcode
and pinned official bases. Linux/arm64 image IDs in that order are
`c0b5e05fac11e81f0ef42ffaea2c2829fbb16329d3091ce825ad406e2cfc2a23`,
`21db83bcdd544e055e2a548433e52a64012ab5d8d1035294ef3c155da7d24be9`,
`7748c14b3b9c649503c20cae8f6fcf90d1042c75d8d9661e5256e80214135bcc`,
`0131b90bab3994cdd9b6e3e91f70586892f3694904a343a80a51625eb4271e5b`.
`05-profiles`0 creates eight fresh profiles on ports32863–32870 without any
existing-profile lifecycle action. Primary/ordered native 2PC limits are10;
zero is only the negative profile. Setup facts SHA
`854498b2604835c3910a0f607ec23929e95073ec9e24530c042320e413788952`
records127 actions0, 20 tracked native inputs, exact17.11/18.6 versions,
available LZ4, installed libraries/bitcode/headers/README, product probe absence
and healthy profile identity.

Self-review then strengthens only the Rust fixture: independently compare every
selected root's complete graph and requested mode mask, and verify singleton
dimension/length/lower-bound with ordinary core SQL array functions. The test
oracle traces native expected edges rather than product facts. Native build
inputs remain unchanged; current input binding and runtime checks are pending.
These build/setup results do not accept the implementation, C1 or full goal.

Input binding at `f9720c02523eb12facfe53648c35d29454b38f62`, tree
`006c8ab5e83abaf266601d0b702bb36cff3620be`, records `06-input-bind`0:
20 native inputs equal built `d0f607e`, with22 Git readers0. Binding facts SHA
`fc736363e1bc8f8f0de30e510ae39eee5c8b3fed9d5aa616aca84aec6485117a`.
The first ordinary `07-storage17` run actually exits101: all10 tests fail at
the same new descriptor check before a capture completes. Raw stdout SHA
`6d4e596b2778d0f7ba883a0745089492d0980c0ea6dceb4991a24e0a41d5559e`,
stderr SHA `b67ae81174ad66b3e0230427b1ae21ff4ef42c45d7204564aca4d7fe7bfc8081`.
This is a product-check defect, not a passed run or an injected error. The failed
revision and fresh profile artifacts remain unchanged.

Root cause: the new check assumed integer alignment from an array's four-byte
header. Builtin anyarray declares double alignment. Separate source capture
`08-alignment-primary`0 verifies two pinned pg_type.dat bodies after exact Git
tree path inventory, generated schemapg target rows and ordinary actual catalog
fields on both native majors. Actual target ordinals are26/25, types2277,
lengths-1, by-reference and alignment'd'. The selected user column's alignment
is independent. Four read-only native actions also confirm no checked prepared
or schema fixture residue; no old profile is stopped or restarted. Facts SHA
`5bd6922ef4b2b7f1992eee20d94f629da8a2dbf14bcbf3c255ddf109f1a1cd6a`,
17-member seal SHA
`7cd60058ad47cf538d996709b8bf565b63d4cc20c747d459a2bf3c158369fea1`.
The two-source alignment inventory stays separate from the62-source design
bundle. The correction checks TYPALIGN_DOUBLE and retains every descriptor,
physical-presence, pointer/header/range and envelope check. Changed C input
requires fresh strict builds/profiles and runtime evidence; none is yet claimed.

Corrected native leaf `954c5dedf62564fae5d4b60d22b8bce8aebbef3e`, tree
`8eadeaad1e7b011ef62ceae99a316c26de42458b`, records strict fresh builds
`09-product17`, `10-probe17`, `11-product18`, `12-probe18`, all actual0.
`13-profiles`0 records127 setup actions0, exact20 native inputs, eight new
healthy Linux/arm64 profiles on ports32871–32878 and available LZ4. Setup facts
SHA `fae326a8a658083c1d9feb4a836b6a5ad138bf3cb21109ea6b90cd9444d8d701`.
Existing failed and historical profiles remain untouched.

`14-storage17` actually exits101: nine fixtures pass, including the new missing
forms/physical NULL/default-drop case and nonempty prepared/retention outcomes;
one existing multi-root fixture fails in the newly generalized Rust oracle.
Raw stdout SHA
`1684cd2dc3bcd605d22c128b997592a90f1eee546eb4d773aab363aed5fd3948`,
stderr SHA `471b607eea6552787f3eede738843ad05653d737ce85b66e03030f095ab89dc8`.
Root cause: the uniform-mode wrapper built a map for only its first root, while
the existing fixture supplies two. The helper now declares the requested
uniform mask for every independent expected root; mixed-mode roots still use
their explicit complete map. No mask fallback or product code change is made.
Only Rust fixture/ledger inputs advance; all20 native build inputs remain
unchanged. The failed run stays failed; current binding and paired suites must
run before finite verification or C1 acceptance.


### Verified missing-image candidate awaiting independent implementation review

Current executable/test leaf `5a82a9643bcc3e43a7a122a72cc6621fb5034482`, tree
`392fe72b2fb7129292edfe8ff73d0529cf9464bf`, keeps all20 native product/probe
inputs byte-identical to corrected built `954c5de`. Recorder prefix
`logs/native-missing-images-5a82a96-` records `15-input-bind`0; binding folder
`logs/native-missing-images-5a82a96-input-bind-v1` facts SHA
`d2caa3114d3c1f1b6c1acc9a681bf9f672dbdc9148665092dcd53f5bc9f457e9`
contains22 actual-zero Git readers and36 verified paths. The four corrected
Linux/arm64 product/test image IDs, in17 product/test and18 product/test order, are
`d6dbb5317bd65822ba4067ee661e142d4640358bb5e0a921be4f6cdc5b4573ac`,
`7a222332d3ac25518d8acf7e0c95464e89ad20d9cb9b8d62c2239805f6a0c83b`,
`0c88b3ab4a6c71f5613a9eb3534b2fbcb1240e176bf7f1bf9cbd9e10d7ebee5f`,
`28bf46415948ca6e6b57626a3aecec1be004ac0f494b734a37bf139658d9c0e0`.

`16-storage17` and `17-storage18` actually exit0: ten fixtures each, zero failed
or ignored. Exact paired ordinary tests positively witness short/four-byte/pglz/
LZ4 carriers, user compression declarations differing from stored methods,
opaque domain/enum/array/composite identities, default removal, dropped columns,
literal inherited relations, duplicate/mixed-mode complete graph bindings, and
C1's old physical natts1 versus later present-NULL natts14. Both native rows
retain their respective missing-value/NULL behavior before and after DROP DEFAULT.
Nonempty normal subtransaction/retention/prepared commit/rollback outcomes pass.
C1 independent implementation closure is pending, not inferred from author tests.

`18-package17` and `19-package18` actually exit0:19 binaries and147 passed tests
per major, zero failed/ignored. Child argv records the ordered-2PC URL explicitly.
Primary/ordered endpoints are32871/32874 (17) and32875/32878 (18), native limit10;
no2pc negative endpoints32872/32876 stay zero; product-only unpreloaded endpoints
32873/32877 stay ten. `20-owner17`/`21-owner18` exit0 with three selected native
owner groups each and86 filtered tests. Commands are `cargo test -p
darmok-postgres-tests --locked -- --nocapture --test-threads=1` and the selected
`cargo test -p darmok-execute --lib native_backend::tests::native_catalog --locked
-- --ignored --nocapture --test-threads=1`. These pre-existing required checks
do not authorize new stress/error/interruption experiments.

`22-format` (`cargo fmt --all -- --check`), `23-boundaries` (`python3
scripts/check_repository.py`, nine packages), `24-clippy` (`cargo clippy
--workspace --all-targets --all-features --locked -- -D warnings`) and
`25-connector-clippy` (`cargo clippy -p tokio-postgres --lib --locked --
-D warnings`) all exit0. All recorded chains use scoped RUSTUP_HOME and the
native-values target directory, execute sequentially and retain exact clean
source reads before/after. No current full-workspace test count is claimed.

`26-cost17` actually exits1: its summarizer compares an OID to an `[OID,mode]`
pair. The child ordinary SQL/sample and cleanup actually exit0. Original helper,
raw sample, native receipt and failed outer stderr SHA
`1698f7972f1045de96819c6dce978e223ec648942b6c929b9da0dc03f6fdeea3`
remain unchanged. Distinct `27-cost17-summary`0 uses a corrected offline parser
of that verified sample, never re-executing the workload. Summary facts SHA
`cd58311b9774090050778cb8a62b34b6de1368555d9923fc3b86f5ef2bf3d4e5`
in `logs/native-missing-images-5a82a96-cost17-summary-v1`. `28-cost18`0 uses the
distinct corrected v2 helper once; facts SHA
`8b784867993f5154cbd3628dbd9c207c0023a74e2eafeec41158c652a480bbd1`
in `logs/native-missing-images-5a82a96-cost18-v2`.

Each ordinary cost sample has32 warmups/128 sequential calls, three duplicate
root bindings, five nodes/thirteen exact references, six selected attributes,
two actual types and four positively witnessed s/u/p/l images. p50/p95/max
milliseconds are0.5655/0.876/1.081 (17) and0.556/0.740/0.930 (18). Both stored
carrier totals are380 bytes per observation; normalized images total8504 bytes.
Attribute/type/missing record arrays are624/416/160 bytes. Initial/final context
allocations are1245280/1212512 (17) and1245280/1245280 (18). Full scanned row
counts and160 raw printed timings per major remain in facts/stdout; only final
copied status is inspected. This includes full metadata and pure JSON copying
plus TCP loopback protocol/utility overhead with microsecond quantization.
No baseline, comparative improvement, isolated C timing, throughput, contention,
allocation-event count, peak/process-memory or full performance acceptance.

`29-environment`0 records85 actions0 in
`logs/native-missing-images-5a82a96-environment-v1`, facts SHA
`81763bf8dd17ba77d7dcceeec58dbb0bf570b188880f3bbff25ce27693e89bbf`.
It ties eight exact healthy profiles, four exact images, all20 inputs and
installed libraries/bitcode/headers/packaged README to127 corrected setup actions;
product-only profile lacks the probe. Native prepared/module/mock refs and
checked heap/attribute/missing-image fixture relations/schemas/types are zero.
Old and failed profiles remain untouched. Passive wrong stdout.log/seal-filename
reader exits1 remain bookkeeping failures, not native test results.

This documentation-only checkpoint updates two unpackaged docs; packaged README
and every executable/test/native input stay unchanged. Author evidence audit
and independent implementation review must verify the exact final candidate
before finite acceptance and normal squash merge. Original f972 product-check
failure,954 oracle failure,26 parser failure, earlier source/reviewer/root failures
remain failed. Full variable/default/TOAST/transitive/provider/candidate/effect/IR/
preparation/data/executor/MySQL row/read-lock/serving/cache/full performance/
artifact/release gates, issues46/15 and the goal remain OPEN. Security, compiler
PR4/resources, hosted CI/account/admin, new stress/forced interruption/recovery,
existing-profile stop/restart/signal and release publication remain excluded.

### Independent finite missing-image acceptance and documentation correction

Frozen implementation checkpoint `2848c06c3dab9573d20ecfac1fbaa3d1533a74b1`,
tree `220d33ff80521ebd27c301189876ddedd2c0cbd7`, changes only the two unpackaged
docs from tested `5a82a96`; all341 other tree entries and all20 native inputs
remain equal. Author `30-author-audit` actually exits0 and seals2381 paths;
facts in `logs/native-missing-images-2848c06-author-audit-v1` have SHA
`bc2096cb987bd8e48d98d4c91c230cf6c493847231b16ff68108baae509c39fd`,
plain seal SHA `cf4e1dd16dac029acb5d7eb164f26145b6fd6541a823808bb6a81ec4ae36ebca`.
It verifies29 outer outcomes (26 zeros, two101 and one1),174 source readers,
exact127/127 setup and85 environment actions, native inputs and the preserved
primary/design graph. It preserves the measured cost and interval receipts.
Independent audit50 verifies the nonoverlap of29 root intervals and12
Cargo/helper intervals and recomputes the bounded costs.

Independent passive implementation review in
`logs/review-native-missing-images-2848c06-implementation-v1` accepts the finite
implementation and supplied evidence. Report SHA
`e1d12fee74bde778fbb21a69de84e39fd37a500784cbd420132fc587e2ac3df2`,
facts SHA `81fa4b39c8da55c2dd55bdab9aaedcb8388259604402d18c70103800801c53e5`,
commands SHA `e3d713bbe8fd47a32bcdf9c5526bb002c9a794d4cbff7c0402f04cda3dd9e002`.
The fixed structured3531-member seal SHA is
`0dc1928c4bcca70c41027db640d1782415df736939b22c1e44e05a3ed993730b`.
Actual `75-finalization` and `76-independent-post` exit0; their receipt SHAs are
`ecfc4618272d9f404945b0dab7cb6041f69644569b0feb04947977c083454868` and
`5ff9d0b875811d4e4833ceb93a558162e229fac4feeb4223e1519f2dba5b3dd3`.
Post facts SHA `f8031c7c4efe6138b8de3d9d09105ba90d6732e8abc1eb1b8232411e4cc8ecd5`;
final/post/metadata companions are separate from the nonrecursive fixed seal.
Live reads are explicitly released. C1/P2 is CLOSED: the independent physical
natts and native old missing-value/present-NULL oracle is inspected and executed
by both paired current suites. D1/P3 remains OPEN at that historical checkpoint:
the preallocation sentence must name the missing-record array, not all metadata
arrays. Fixed attribute/type arrays retain periodic/postallocation checks;
allocation-block overhead and peak/process memory remain outside this proof.

Reviewer readers45/47/65–68 remain actual1 bookkeeping failures; corrected50
actually exits0 and verifies the supplied graph. Root `31-review-rehash` also
remains actual1: its reader incorrectly assumed every passive command had the
same working directory. Exact absolute-path reads used two recorded directories.
Distinct `31-review-rehash-v2` actually exits0, verifies all3531 seal members and
companions (3560 unique files), final/post/metadata actual0, all raw stream hashes,
the six preserved reviewer failures and six fresh clean source readers. Facts
in `logs/native-missing-images-2848c06-implementation-root-rehash-v2` SHA
`2b7d14ba32168242b6c72dc7cee19bd2bce4bbdf4eae81bc4302e07ce0d1bcb1`.
No native test, build or workload is repeated or relabeled by this readback.
A passive optional filename search exit2 is also bookkeeping, not product evidence.

`32-doc-worktree` actually exits0 and creates a separate documentation leaf at
the frozen checkpoint; the sealed implementation worktree stays unchanged.
This leaf fixes D1's sentence and records finite implementation acceptance in
these two unpackaged docs only. Packaged README, every executable/test/native
input, exact runtime/build provenance and historical failures stay unchanged.
Independent documentation readback is pending; D1 is not retroactively closed
on `2848c06`. Normal reviewed exact-head squash merge remains required. Full
variable/default/TOAST/transitive/provider/candidate/effect/IR/preparation/data/
executor/MySQL row/read-lock/serving/cache/full performance/artifact/release gates,
issues46/15 and the goal remain OPEN. The existing excluded scope remains excluded.

### Missing-image documentation readback and attribution correction

Frozen leaf `8030205146140e55d1bd479ccf8a9e4a59391067`, tree
`b32b0eda568dd8c036c95eb4f8b684111b3444a5`, receives bounded independent
documentation readback in `logs/review-native-missing-images-8030205-doc-leaf-v1`.
Report SHA `45e1fe9ddd1aa1eb01e58a0cb5e2a9040fd3fc43cc1e39e810ca5f244adb7ce9`,
facts SHA `1cb93c017c39b0f41070d1b9c231d6b521da5f702475ebff60a480f5cd9b8034`,
commands SHA `5ee24d22ba2a13ad6b9bf8bb5d648470061656872649af0d6d4fbfa7827dcefb`.
The structured4240-member seal SHA is
`46afa459b12f9d56fc3ee87148ca58338262751ffc97384f6c98bcc2f2f2c6fd`.
Executed `20-finalization` and `21-independent-post` both exit0, receipt SHAs
`e10b99dd84d5e35f4255814739940c35a98275669784898520713c74572d094c` and
`d5a75305abe770b4b02ca081765288b24eb551af9df42e2de2e6301625759f32`.
Post facts SHA `229ac705df15a2fd13b26f89a79dc6a0272843828284eb36031ced8e7e16349d`.
Actual before/after sources are exact and clean; live reads are released.
D1/P3 closes only at803. D2 remains OPEN at that historical leaf: author30
preserves interval/cost receipts, while successful independent50 proves interval
nonoverlap and recomputes costs. This leaf corrects that attribution in the new
acceptance paragraph; the historical803 bytes remain in frozen source and witnesses.

Own reviewer11/12/18 remain actual1 passive schema/display/builder failures;
corrected15/19/20/21/22 exit0. Root `34-doc-review-rehash` exits0 and verifies all
4240 seal members plus separate companions (4270 unique files), raw streams,
exact clean source and the preserved failures. Root facts SHA
`543be44490426508796fc593346eb1f00d9b4196eda03048e70f06d61ad26b27`.
`35-final-worktree` exits0 and
creates this separate correction leaf; both prior worktrees remain frozen.
This leaf updates only these two unpackaged docs. All341 other tree entries stay
equal to tested5a; all20 native build inputs also equal built954. No runtime,
build or cost sample is repeated or relabeled. Bounded D2 readback and normal
exact-head reviewed squash merge remain pending. All previously open full gates,
issues46/15 and the goal stay OPEN.

### Merged inline missing images and combined database setup selection

PR #56, “Copy native column missing images in private metadata reads”, was
normally squash-merged at reviewed head
`4b0f3e3d09f6991b77256ebaa41c0010afd9c86d`. Public main
`19d4a9b1a44303fa334a1fc3cab31fb3174fa80b` has the exact reviewed tree
`e4e55772c460e7dabad4cdd8be3e21111ed50bea`. The accepted final review closes
C1 at2848, D1 at803 and D2 only at4b; historical leaves retain their findings.

The final independent review is
`logs/review-native-missing-images-4b0f3e3-final-doc-v1`: report SHA256
`36ff378b89e5640b5060daceeaa230e69c6b1fe5c292d25b7f37f32c93311413`, facts
`be5eb421e37935d6b3983042575bb59f904f8251759557d2f056308ea38f384d`, commands
`bb3bc6eb931f794bb2594ad2fd57138b74ee2d8a8487e1314d1d583d359e3467`, and
structured4893-member seal
`c9ea84e5c3a57b44d2c94d0221f24454ec9fca2ebe37fbbf54ba7df2c55975a3`.
Actual0 finalization08 and independent post09 have receipt SHA256s
`43cbe68d042521a8274a79d42826b6b1e8f8f3c300a599cf15a04c4170c4fd51` and
`c1211d67742eed773d4d9b7e207690ae9c0ec360b2b4a7dd5ca9a6dd06f58ee5`.
Their stdout hashes are
`31304c73f0e356b54b3776bb9e9497268c2ec023e8041422ee51dd10c2a2aa88` and
`220ca471645270d652f3feeec73af2353740466c08a8731a64e4b774360fa60f`;
post facts hash is
`57557b965afc1f3f8ab30f36761677356078440e1d42839da857fa0859f510e2`.
Root37 actual0 rehashes all4893 fixed members plus separate companions,4923
unique paths total; its file map SHA256 is
`776d3e1c26f2731bf3051cf6c24f4032b8aa28113d21dfe64f2e83f3f256b5c5`.
The frozen tested/build/worktree roles and paired10/147/3 evidence do not change.

Root38–46 all exit0: exact public-main/branch/body preflight, normal branch push,
PR creation, exact head/base/body readback, ordinary squash, public repository
and tree readback, issue46 exact body update, main fetch and creation of the
separate `native-database-init` worktree. Publication facts/seal hashes are
`51d1421c8dee87b12c8757d9fcb6d9981fcd5323cdeadb7f924983eb772e3035` and
`fcd1b8d36e041c2de846c4aeedd982a3cbd51bd66b2a277b6d45515d3ca4eba0`.
The personal repository remains PUBLIC with description “A MySQL-to-PostgreSQL
compatibility proxy”; its README has no experimental narrative. No other
eligible open PR remains. Issue46 remains OPEN at exact v7 body SHA256
`0cd2a6df4c1139542fcfc20dbe5f87144eb230a0d30397b25966fa2beae78a88`;
issue15 and the full goal remain open. Excluded compiler PR4 is not inspected.

Passive display bookkeeping failures this cycle are retained as failures:
wrong final-reader key and treating a dictionary of seal members as a list;
the corrected schema reads and root37 pass. A publication-helper construction
initially replaced its filename prefix before a later assertion; the existing
body/preflight were preserved and the separately completed helper was read
before use. None is a native runtime or product result.

The next finite component is [combined explicit database setup](native-database-init.md).
The old initializer checks the compatibility schema but not the native extension
and live utility response. The selected boundary checks both inside an owned
transaction, drains and decodes the exact fixed native response, then submits a
separate checked COMMIT. It adds one setup round trip and no per-row round trip.
The API/CLI are renamed cleanly; no schema-only success alias remains. This M3
setup work can overlap the open M2 dependency/executor work, without closing it.
The new design is proposed, not implementation or verification acceptance.

Primary installation source capture root01 exits2 because a preceding passive
reader wrongly assumed the saved tree's `tree` key; it actually contains a
`paths` list and that failed display prevented helper creation. The missing
helper invocation and raw streams are preserved. Corrected root02 exits0,
capturing paired pinned extension source files and the two official CREATE
EXTENSION pages in `logs/native-database-init-primary-v2`. Its manifest/seal
hashes are recorded in the design. This is functional provenance only. No new
native profile, native build, forced-error experiment, security work, hosted CI
poll, account action or release publication occurs here.

### Combined database setup design review and provenance correction

The independent review at clean design head
`f01e2aaa67fe2f5f251f45e2b8de62b35c078ac2` / tree
`7b921f589c304c5688e2bb7657eaa446ba669e2e` conditionally recommends
implementation with no architectural blocker. It does not accept implementation
or runtime. `logs/review-native-database-init-f01e2aa-design-v1` contains report
SHA256 `a9b15f8d68233c1b318105937ff4a493d14b14e4188970f292c894d196818a6a`,
facts `1ae6f8e4731c74af78ec3ec480c33ddadd8db8831a25f3089feda2211fc20db8`,
commands `6838353d6a72a5394d63aacf10f9f3b79f2e2b2e4c882ae87e6d08f2fb68d052`,
and 217-member nonrecursive seal
`5e4b3130ba54971636e6bb65e82526f35fce61d45855aa1962ce38932d332cc1`.
Actual finalization38 and independent post39 both exit0; their receipt hashes
are `70cdc0068bc43fe98e48a83fed7137f5c0bc2e79d02c269a4f00aa1ae2d10306`
and `26b153815a915ac46b7bdba4a30c7beb4870df65139eea5762cf82f42c5f189d`.
Passive reader20 wrong-path failure, reader31's real primary self-seal failure,
and finalizer-builder36 quoting failure remain actual1. Corrected readers and
final/post checks do not relabel them. No runtime command ran.

P1 remains OPEN at the original f01 design: root02's actual0 HTTP capture
created its seal file before constructing members, so it recorded itself as
empty. All six sources and seven nonself members are valid. The original
helper, files, seal and exit remain unchanged. Root03 exits0, rehashing all217
review members plus separate companions (233 unique paths), and constructs a
distinct 11-member primary-v3 seal with the manifest evaluated before opening
the seal. Corrected seal/facts hashes are
`1aad1279e0b23ffadbbfe7170747a3ec98e86200b58632b566bf3d12e605028c` and
`6cebe6b5f618eb472b23551f61844b2938ffa9c0f85b6133682e774db3d3d8a9`.
This corrects saved provenance only, without a new HTTP capture, Git-blob
identity claim, native build or runtime result. Root04 exits0 and creates this
separate docs correction worktree; the reviewed f01 worktree remains frozen.

The design explicitly carries all three mandatory ordinary coverage conditions:
positive advisory-lock observation after checked SHOW before COMMIT, atomic
rollback of both new components after unpreloaded placeholder decoding failure,
and acceptance of nonmembership application dependencies while rejecting extra
extension members and reserved-namespace objects. Bounded independent P1
readback is pending at this correction leaf. Implementation, runtime acceptance,
M2/M3, issues46/15 and the overall goal remain OPEN.

### Combined setup implementation candidate

P1 is independently CLOSED only at corrected design head
`02240773b674894f1b2ea88409832cd8aadbc718` / tree
`3d854213c13fd14e7320e8a5e6837898381e89d3`; historical f01 stays OPEN. The
bounded review `logs/review-native-database-init-0224077-correction-v1` has
report SHA256 `6387cd2bab0b47b5fa2e79c10309cbab9238bd31abde538dede65c8d343e938b`,
facts `9ecaa9b8f16a68f8fe63dcdb664d7a2e0b301b35791e40787e9183442757e594`,
and 139-member seal
`95bfdcf0e7f8acfc9987b25b99a49c67f8b764ca9069749053734ea9a222189b`.
Final10 and independent post11 exit0 with receipt hashes
`7cb24805fcd2b7ebe70ff4092faf3a49c70fea07759d9d80c9db6cb6d580e208` and
`3d160d1bb63343eecfa11ae5d8f95ce8702d1f28c554fb923fff540d96577aaa`.
Root05 exits0, rehashing139 fixed members plus separate companions (149 unique
paths); map SHA256 `9f080edbd8480ad1cb0faf5a6c1223c6d35aba0991e2693a7a5e63eea97dfe20`.
Root06 exits0 and creates the separate implementation worktree. Both design
worktrees remain frozen. No design review ran runtime commands.

The candidate cleanly replaces schema-only API/control variants and nested CLI
with combined `initialize_database` / `verify_database`, `init` / `verify`.
It validates both functional inventories and the exact native empty response
before a separately observed COMMIT, retains original errors and actual cleanup
receipts, and requires confirmed idle before owning a transaction. The shared
private SET/SHOW checker has a fixed prefix; public empty discovery still does
no SQL. Tests cover all three required ordinary cases alongside existing
fixtures. SQL kernels/native product/probe build inputs are unchanged. Current
compile/runtime checks, artifact reuse and independent implementation review
remain pending; no earlier pass certifies this candidate.

Root passive reads guessed nonexistent nested native_control and connector
tests paths; stderr/missing-path outcomes precede corrected inventory/source
reads. An apply_patch delete/add targeting one filename was rejected before
writing; the subsequent explicit replacement completed. These are tooling
bookkeeping events, not runtime or product results. Compiler PR4, security work,
new stress/forced-error/interruption/recovery experiments, hosted CI/account
work and release publication remain excluded. All full gates, issues46/15 and
the overall goal remain OPEN.

### Combined setup verification and packaged instruction correction

The implementation is tested at clean
`07647b50af46bd215616a154bd332855d630d7c8` / tree
`9d001170be930f4f09a396d0b5bd9e2ded4ffdef`. Root07 compile exits0. Root08's
read-only environment audit exits0 with85 nested commands,8 unchanged existing
profiles and20 native inputs matching built954; facts SHA256
`4aab989e56e52938c8b6f365dd1ce521b49db769e0e7c669a82f57ef8a2947fb`.
Primary/ordered profiles still use max_prepared_transactions10, the existing
zero profile remains negative, and unpreloaded profiles remain unchanged.
No profile is started, stopped, restarted or signaled.

Root09 `check-native-database-init-07647b5-v1.py` exits0 with all12 sequential
checks0: format;9-package repository boundaries; affected execute/catalog/CLI
offline tests with default and sqlparser/bigdecimal; strict workspace
all-target/all-feature Clippy and separate connector Clippy; then13 database
setup groups,3 affected catalog-owner groups and4 CLI process groups on each of
PG17.11/18.6. Required database groups run explicitly with --ignored and have
zero failed/ignored results. Offline results retain their unrelated ignored
fixtures; they do not certify a whole-workspace runtime test run. The matrix
records exact source, env, argv, raw stdout/stderr and six source readers per
check in `logs/native-database-init-07647b5-checks-v1` and its case folders.
Matrix facts SHA256
`8a0174faf7233e0ae13cba0b40b04194431c9b85c07d922c5ef0d1e7fc1161d6`.
It compares exact20 native mode/type/OID/path tree entries against built954.

The three mandatory ordinary cases pass on both majors: a second owner
positively observes the setup advisory lock after checked SHOW before separate
COMMIT, then acquires it and observes both committed components; a fresh
unpreloaded selected database retains the strict placeholder decoding failure
and real rollback receipt and has neither namespace nor extension afterward;
a public function's ordinary nonmembership DEPENDS ON EXTENSION is accepted,
while added extension members and reserved-namespace function/collation objects
are rejected without repair. Existing repeats, read-only defaults, caller
transaction preservation, temporary builtin-type qualification, private
postcreation metadata mismatch and810 typed helper cases pass.

Every current CLI child has an exclusive receipt and original streams:32 per
major (15 actual0,17 expected1) and15 per offline feature set (4 actual0,5
expected1,6 usage2). These94 children cover URI/keyword explicit physical
databases, both component snapshots, unchanged application data, actual output
and ordinary driver/server connection closure. This is not configured logical
routing or serving acceptance. No new stress/forced-error/interruption/recovery
experiment, security work, hosted CI or release publication is inferred.

Self-review found two current installation paragraphs still describing the
removed schema-only commands: the packaged native README and the server lease
doc. Root10 exits0 and creates this separate correction leaf; the tested076
worktree remains frozen. This changes only documentation and preserves all
Rust/SQL/fixture/helper implementation bytes. Unlike unpackaged docs, the native
README is one of the20 build inputs. Therefore this leaf has19 unchanged native
mechanism/header/control/script/build inputs and one deliberately changed
packaged README. The earlier exact20/runtime claims remain bound to076, not
relabeled as this source. The spec now requires rebuilt current packages and
compares their actual library/header hashes before reusing existing running
profiles with their explicitly historical README. No running profile lifecycle
change is needed or authorized here. Current package checks and independent
implementation review remain pending. Full gates, issues46/15 and the goal stay
OPEN.

### Combined setup independent review and error-tail correction

Root11 package checks at clean6217c0e exit0 with58 nested actions. Four current
arm64/Linux product/test images include the corrected packaged README; all
native libraries, bitcode, three headers and test probe binaries match built954.
Product images exclude the probe. Eight existing server profiles retain their
IDs, images, healthy state, preload and max-prepared settings, with no native
prepared transactions or leftover fixture databases. Package facts SHA256 is
`39b6bb60bf40ddcecbd37774c827846bf78c5051afc0b1929a05cce45c143c04`.
No native server was started or profile lifecycle changed.

Independent implementation review at6217c0e does not accept that candidate:
C1 remains OPEN there. The shared event checker replaced the original backend
error and ignored subsequent nonterminal events, allowing malformed tails to
reach a reusable failed-transaction classification. Concrete connector framing
already rejects such post-error events; this is a private-checker contract
defect, not an observed ordinary native-wire failure. The new correction keeps
the first backend error, records every post-error event except terminal
readiness as malformed, preserves any earlier mismatch, and drains without
changing transaction round trips. Offline public Config parsing errors provide
distinct inert error carriers; no new forced native error experiment is used.

Review folder `logs/review-native-database-init-6217c0e-implementation-v1` has
report SHA256 `c0a8da6deb911c68516164df9128144b7d50c6c20311ffeda809c2f3383b46bd`,
facts `effd075c8621c7908b7c3375c86c56bd3d8c0d8ab8f3144d75876d3bab4c0436`
and1383-member seal
`3bdc12d0a5294586aac053a73c735161f5b60512ba467a48a39b726b0e7555ea`.
Actual final30 and independent post31 exit0; their receipt hashes are
`1baad2255311e3aaf1e64bf3102fa7fe039e2a731635d3a80517aaa35b678326` and
`f6b44ee90d129abd9c0687e581f71b6866b3a32f4097d9e2ded651442d31c1a1`.
Reviewer20's absent-path actual2 and audit26's incorrect child-serialization
assumption actual1 remain preserved; distinct audit28 exits0. Offline child
groups may overlap while root Cargo/check intervals remain sequential.

Root12 exits0 rehashing1383 fixed review members and separate companions
(1394 unique paths); facts SHA256
`3c247904950f9721f6acdd500c5c134c90e33f28920b697488fb5ff8202d773b`.
Root13 exits0 creating a separate correction worktree. Earlier tested076 and
reviewed/package6217 paths stay frozen. A passive root read first guessed the
absent commands.json and exited1; inventory corrected it to commands-v1.json.
That bookkeeping failure is not a runtime result.

This correction changes Rust, offline tests and unpackaged documentation only.
All20 native inputs must match the actually rebuilt6217 packages before their
reuse. Current affected checks and independent C1 correction review remain
pending. Full closure, executor, serving, MySQL locking, performance and release
gates, issues46/15 and the overall goal remain OPEN. Security, compiler PR4,
new native stress/error/interruption/recovery/profile experiments and hosted
CI/account/release publication remain excluded.

### Combined setup accepted implementation and final instructions

The corrected implementation is independently accepted at clean
`3b9a15e2523a15624efb35e186bb20a4401061c8` / tree
`1b7f4a1020e2c1d5ccf688469c556d46e8038446`. C1 closes only there; the old6217
rejection and saved076 checks retain their original scopes. Root14 exits0 with
all12 sequential affected checks0. Default and explicit sqlparser/bigdecimal
each pass5 catalog/3 CLI/22 execute, retaining4 CLI and76 execute ignored
unrelated fixtures. PG17.11/18.6 each pass13 setup/3 catalog-owner/4 CLI process
groups, with zero failed/ignored required native tests. Every actual CLI child
has its original receipt and streams:32 permajor (15 actual0/17 expected1) and15
per offline feature (4 actual0/5 expected1/6 usage2),94 total. Matrix facts SHA256
`a1e69d1546c9cef4536d5fa136552d01c25b9b3e3e20f807183fe66eded4fe47`.

Root15 exits0, rehashing current check/child records and running28 ordinary
read-only package/profile observations. Four built6217 package IDs match; all20
native inputs equal that source, including its current README. Eight running954
profiles keep their IDs/images/healthy state/settings/native libraries and
historical packaged README. Prepared and fixture-database counts are zero.
Primary/ordered max-prepared remains10; the zero profile stays negative.
Audit facts SHA256
`20bcb353155f8cea36948236072f20488b2db22100985cb848f3ba6c2783ef3e`.
No new build, native server or profile lifecycle operation occurred for C1.

Independent folder `logs/review-native-database-init-3b9a15e-correction-v1` has
report SHA256 `699a5b3cc826889bd93fd9a47849eff68c8f3911b49c486007051a70de4a3264`,
facts `d4ec2a1ccc11ad153400f4f3fe9895ee6ee1bc2a01bfc705519786459779e1aa`,
and732-member structured seal
`e8f20efafb86eb65d4953776e3ced56ae99b59a9954ac1cbf2a19b273980e4d1`.
Actual final13/post14 exit0; receipt hashes are
`293e727cfce951dfed4d117ce9864d8e8c99f7754ef7003363206b9bfaa5469f` and
`9518ef855241cbb1508bc1653249f7623c9e7c80d109becf74909ac44cffef61`.
Reader07's actual1 wrong-file no-match is preserved; inventory09/full reader10
correct it. The review independently accounts for25 offline vectors and the
common row-event branch, preserving first errors/mismatches and actual readiness.
No new implementation blocker remains within this finite component.

Root16 actually exits1 because its new rehash helper assumes flat anchor JSON;
the review's observed anchor format is a keyed map. Original helper/raw/partial
output stay preserved. Distinct helper v2/root17 exits0, verifying732 fixed
members plus companions/failure witnesses (747 paths); facts SHA256
`41bcfb9186a0565cbce4dd42d5d0a89b31a34d090fcc7f2d2a28b1341a9ec3ed`.
Root18 exits0 creating this final documentation worktree. Tested/reviewed3b9
stays frozen. The reader failure is bookkeeping, not a product/runtime failure.

D1 is historically OPEN in the3b9 review: CLI instructions omit the fourth
extension-dependency/conflict group and still label076 checks current/package
checks pending. This leaf corrects the general four-group description, marks076
historical, records exact current3b9 verification and6217 packaging, and updates
the database-init status. It changes only three unpackaged documentation files;
executable/test/dependency and all20 native inputs must remain exactly equal to
the accepted3b9 source. A bounded independent final-doc readback is required
before merge, with its result recorded in the PR rather than fabricating a
runtime execution at this leaf. Full closure/provider/default/missing/constraint/
effect/preparation/admission/executor/serving/MySQL locking, performance,
hosted CI/artifact/release gates, issues46/15 and the overall goal remain OPEN.
The standing security/compiler/native-experiment/CI/account/release exclusions
continue to apply.

### Combined setup merged and variable-payload design

PR57 is normally squash-merged at public main
`41b929b7f35d893370c9ac1baefbf8b281068f58` / tree
`02510bb377033f4efd2d79978e1c18fbab8f1d92`, exactly the independently accepted
final documentation tree. Final review at9da393a accepts the finite leaf and
closes D1 only there; C1 remains bound to accepted3b9. Its report/facts/179-member
seal SHA256 values are
`70cb87021036123c995a4f5822007fab28915ea95556c87fe2d5bd4e7d84199b`,
`c87e83aaf4a6e6b2df8f4b4e912b2890f61abb0c98e018fd5c5f796bbaca654e`
and `493e20424f6366284f00cf06de52649de5ed6876f5f438c4958c262644cf737c`.
Root20 rehashes179 fixed members plus companions (190 paths), actual0, facts
`790b532347d56d6f02cb6d4c2bc62cf61287d3b2fbfb52f11860779674883248`.
No new runtime result is attributed to this documentation leaf.

Root21–28 publication/issue actions exit0. Normal squash checks the exact
reviewed PR head; readback confirms the public personal repository, description,
merged PR/body, exact main tree, issues46/15 OPEN and no eligible open PRs after
excluding4 before output. Issue46 bodyv8 SHA256 is
`93b8d61abc1e8dad582ceb19405b2d21dd81a2fd8e446218f459b3f681bc6f10`.
Root29's finite publication record has171 nonself members, facts SHA256
`ac2177f21f583215824c4b623c1a51d60112b463d6c7fd2bf720e63f07da41a1`
and seal `c42f54d321f5124f94d422731927cb907779e122647a67fc01390239c1ee6fcf`.
Root30 fetch and31 isolated worktree creation exit0 at merged41b. Earlier tested,
packaged and reviewed worktrees remain frozen; dirty shared checkouts are untouched.

The next finite work is specified in
[native-variable-catalog-payloads.md](native-variable-catalog-payloads.md): selected
column default/generation and direct-type default carriers, including ordinary
catalog TOAST values. The choice uses three coherent observations, a fresh
registered payload-source snapshot after complete physical acquisition, direct
builtin heap chunk assembly outside raw/S, and final carrier/definition agreement
under S. Copying does not parse/evaluate expressions or admit their dependencies.
Extra catalog/TOAST scanning and retained catalog-reference costs are explicit;
new implementation/build/runtime/performance acceptance remains pending.

Recorded primary commands `native-variable-catalog-payloads-41b929b-01-primary`
and02-primary both exit0 at clean41b/tree025. They use the already read universal
recorder, exact argv, scoped Rust homes, original streams and six source readers
per command. V1 checks34 cached bodies against the pinned prior181-member audit
and captures6 new HTTP-200 bodies, with40 total (20 permajor). Its facts and
53-member nonself seal SHA256 values are
`8c7671910d4d0cc8083d7474662311f2c7419f5331a8aa40b1f4f49d2b3e9e9b` and
`ea11787b3a90df4396e50b86a1dea7bd9da2d274d1ce814408385bed59d29675`.
V2 rehashes all53 members and adds8 HTTP-200 bodies:48 total (24 permajor).
Its facts and72-member nonself seal SHA256 values are
`f6922882d442cf81751b00d8a57e6868e60903fc2fad657fa9016f7c278dc70c` and
`9209d7c03637577f8866a8dfb915eb43bb76f3327c5e0b711287a5dc66d40fc0`.
Both maps are constructed before opening their own seal. Captures establish
source bytes/provenance, not Git-blob, binary or runtime identity. A passive rg
for scan-option names in relscan.h returns no-match1; the actual heap scan flags
are read from current heap_storage.c rather than inferred from that file.

This leaf changes only unpackaged documentation. All executable/test/dependency
entries and20 native package inputs must stay byte-identical to public41b.
Independent design readback is required before normal PR merge; its actual
result belongs in the PR, without relabeling historical runtime evidence. No
new native experiment, profile operation, security/compiler4 work, hosted CI,
account work or release publication is included. Full gates, issues46/15 and
the overall goal remain OPEN.

### Variable-payload descriptor admission correction

The first design at clean `8884e99932ec172c046a33286a4e7747e51a26a8` / tree
`08948d9d5064f5c708c2ea7c21c12f5a1e38314e` is not accepted for implementation.
Its independent review identifies C1: selected TOAST descriptors were opened
before fresh actual profile validation. Native attribute construction can fetch
missing/default/constraint metadata before AM initialization; a post-open check
or later generation mismatch cannot undo provider execution. The original
reviewed leaf remains frozen. A separate correction uses B's fresh raw capture
as the pre-open checkpoint, then prepares descriptors outside raw/S with B's
source snapshot alive and all prerequisite relation tags already owned.

The corrected design names the pinned 2830/4171 bootstrap metadata TOAST heaps,
their complete three-attribute layout, NULL options and absent descriptor-loading
branches. It specifies native critical-index initialization/nailed support,
cold/warm TOAST construction and minimal critical-index reload prerequisites.
Structural/options/provider mutations of those pinned descriptor prerequisites
remain outside the continuous builtin profile; ordinary external defaults and
application DDL stay required. The three observations and concurrent native
two-phase support remain; no additional full observation or provider fallback
is introduced. Independent correction acceptance remains pending.

Root03 documentation checks exit0 at888: exactly three unpackaged docs change
from public41b, 342 tracked entries and all20 native inputs match. The facts
SHA256 is `b3606cdd91d12c752fe6292e6b07b064142bd2fbbeb1d4fbf09ea05f8fce6f96`.
Review `logs/review-native-variable-catalog-payloads-8884e99-design-v1` has
report/facts/330-member structured-seal SHA256 values
`2eee150240375a356f741ed984181b6fc4c60a7e50c54f6e6a55d7561d3ce23d`,
`31a088185baa244de9fa749853d358f3d552b4258d27a40b8adb6b70d9020fc0`
and `f466a75cdb56200da0e96ad3fbd1c7441b6bd0b29828c9a335339a3008aa248f`.
Actual final25/post26 exit0; their receipt hashes are
`fdd817a36b56cc18eedacf83315603dfa52ff0a6489c5309cdbf5f771e346fa4`
and `cd6896a697a41892645446b9891d641bad34c9176202c312aaf741c58da078fd`.
Reader14's actual1 request for the removed PG18 GetOldestSnapshot is preserved;
distinct corrected reader17 exits0. Review lifetime/heap/2PC conclusions are
conditional source conclusions, not runtime results or C1 closure.

Root04 exits0 rehashing330 fixed review members plus companions (344 paths),
facts `2110142939401edd595b436d486ac21efc15ad24ebf36ca67d02bcffc9f6842d`.
Root05 exits0 creating the separate correction worktree. Root06 exits0 there at
still-clean888, rehashing all72 prior source members and adding16 HTTP-200 bodies:
64 sources (32 permajor), v3 facts
`c6bf6baf6b4b09c49790f214488b12dd1e2409eebfc7cd29cfe619ce016abc7a`
and107-member nonself seal
`89d75367952654a44b252bc39eb8a6fdd7af32826327190a242957afeee0b8f6`.
Exact root06 receipt SHA256 is
`1a7298b01e69fead097e910ce4255026192128f24c03cbc04b0865c3f2fe6016`.
These source/provenance records do not certify native artifacts or execution.

This correction changes only unpackaged design/ledger documentation. Current
documentation/source checks and independent C1 correction review are required
before a normal PR merge. Native implementation, builds, ordinary paired suites,
performance and full closure/executor/serving/release gates, issues46/15 and the
overall goal remain OPEN. Standing work exclusions remain in force.

### Variable catalog payload implementation: paired local verification

The corrected design at `91f6f8035bce39b5e55ce4b87c84aeafe963ea71`
was independently accepted within its stated builtin profile and merged through
[PR58](https://github.com/samrat-shamim/darmok-proxy/pull/58) at public main
`012be29d806c4928b328ec06abd58c831c0b93da`. C1 closed only for that corrected
design; the original888 rejection remains preserved. Design acceptance did not
certify the implementation described in this checkpoint.

The private C invocation now copies column default/generation and directly
referenced type-default carriers through three coherent observations. All six
fact-catalog storage graphs join application graphs with exact mode union and
per-root provenance. A closes before native physical waits. B retains its
registered source snapshot through builtin-profile descriptor admission and
direct selected-TOAST heap copying, outside raw/S. It normalizes actual native
short/plain/PGLZ/LZ4 carriers without parsing expressions or loading type codecs.
B closes before C; C validates exact facts/carriers and presents owned B images
to one pure consumer. This adds no SQL/frontend storage entry point.

Tested source is `ed847973888af48c5d36c73018614a2280d624a2`, tree
`b5d3a0845aa4111ceb25a5d7bd8f5699f67c67cc`. Strict GCC and LLVM product/probe
packages were built at `c43443d27d8fa70d38da9ac7e52c128b31907c43`, tree
`778d72febd5a4e9cf171ed6d9e2014b2c275a535`. All22 actual native inputs
(16 product,6 probe), including public headers and packaged README, are
byte-identical at tested ed847. The later change only simplifies two fixture
assertions and passes full lint/runtime checks. Packages use pinned official
PostgreSQL17.11/18.6 Alpine bases on Linux/arm64. Four image IDs, in17
product/test then18 product/test order, are:

- `sha256:6673c7a74a003a6a018c9bf5ed2a4a8dcc4e055a7e18ae5a459ac5a51f3cf9e8`
- `sha256:58729feee3045f28da74562fffa6490016fc172d0a9c11151f29746467ee600c`
- `sha256:0b885335f52f5742463e6336085527ec473d61a3cada25627691c1141e41b424`
- `sha256:c63f389ba04b5fa32c337ba96d7dfe253158ac1b0a5bc5dcc90c04add8ce020e`

Root30 packages actually exit0 with38 child commands0; product/probe product
library hashes match and product images contain no probe. Package facts
`logs/native-variable-payloads-c43443d-packages-v1/facts.json` SHA256 is
`af69cd288024cfd75a46716a3319c3fee04c2f8e47132ed238d77cf4de1dcc52`.
Root31/34 start eight fresh required verification profiles and bind their actual
IDs/images/settings/libraries, with45/147 child commands0. Existing profiles
are untouched. Primary/ordered profiles require native maximum10; zero is only
additional coverage. The ordered callback preload is explicitly
`darmok_catalog_probe,darmok_server`. Profile manifest SHA256 values are
`fbcff1a0c5d280fe0a5557817aeb593c288d29a0b4ce0d4853c496b91db745a4`
and `bc58c75325b2786f152de6bafda6dd3cb5d983c775946c2f06823c869f07081e`.

Sequential root records use `logs/native-variable-payloads-<source>-<number>-<label>`:

| Records | Actual command/result |
|---|---|
| c434 `32-storage17`, `33-storage18` | `cargo test --locked -p darmok-postgres-tests --test server_heap_storage -- --test-threads=1 --nocapture`:11 passed per major, zero failed/ignored |
| ed847 `37-package17`, `38-package18` | `cargo test --locked -p darmok-postgres-tests -- --test-threads=1 --nocapture`:19 binaries/148 passed per major, zero failed/ignored; includes all11 storage cases |
| ed847 `39-owner17`, `40-owner18` | `cargo test --locked -p darmok-execute --lib native_backend::tests::native_catalog -- --ignored --test-threads=1 --nocapture`:3 selected required groups passed per major, zero failed/ignored,95 filtered |
| ed847 `36-clippy` | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`:0 |
| ed847 `41-format` | `cargo fmt --all -- --check`:0 |
| ed847 `42-boundaries` | `python3 scripts/check_repository.py`:0,9 packages |

All use scoped RUSTUP_HOME/shared native-values target, exact clean source reads
before/after, and preserved child streams/actual exits. Matrix binding manifests
and child argv identify the actual ordered endpoint. No current full-workspace
test count is claimed. Package17/18 receipt SHA256 values are
`46daf0bdd0a338a3585a2c2791a31b6d7a1641e79f0cc3c668f41b615230d5cc`
and `7978694bbe78b23e72421e30e1767d92b161e90974cfa7585829ab23d2e9e024`.

Bounded ordinary fixtures use independent native detoasting/carrier oracles,
including independent absent/text-only type fields, NULL/empty values, actual
inline/external PGLZ/LZ4/uncompressed carriers, equal images with distinct source
identities, unrelated TOAST rows, default changes/removal, renamed/dropped slots,
missing-value retention, stored/PG18 virtual generation, and fresh/established
RR. Prepared column/type changes and literal replacement verify both normal
native COMMIT PREPARED/ROLLBACK PREPARED outcomes. Waiting captures own all six
catalog AS references, retain no A catalog horizon, and preserve entered data
snapshot state. Continuous pinned bootstrap prerequisites remain explicit;
arbitrary catalog/provider histories are not admitted by a post-open check.

Failures remain immutable: root25 at10443 actually exits101 with0 passed/11
failed. The implementation assumed eight-byte `text[]` reloptions alignment;
native17/18 declarations both require four-byte alignment. A fixture also
incorrectly required both domain-default fields to use PGLZ: native binary is
external/PGLZ while its independently stored text is external/uncompressed.
Root26b/28 native declarations and root27/29 bounded native carrier observations
exit0; c434 corrects exact checks without a fallback. Root35 atc434 exits101 on
two fixture style lints; ed847 corrects them and root36 exits0. Earlier10443
packages/profile results and bff local compiler feedback retain their original
scopes and do not certify the corrected implementation.

The c434 ordinary storage fixture reports12 cost samples per major: all one
attempt,35/37 exact references,78..254 selected chunks versus209..337 scanned
TOAST rows. B context allocation reaches3,006,056/3,038,824 bytes; complete
SQL-and-SHOW elapsed ranges are8.82..29.44/8.03..32.84ms. These are finite fixture
observations, not allocation-event, process-memory, throughput or contention
acceptance. A/B/C scan/copy costs remain separately recorded.

Root43 is read-only:82 actual commands0 rebind all22 inputs/four image IDs and
eight unchanged healthy profile IDs/settings/libraries/headers/README/probes.
Prepared counts, coordination/synthetic-reference counts and matching
storage/attribute/missing/payload fixture objects are zero. Environment facts
`logs/native-variable-payloads-ed84797-environment-v1/facts.json` SHA256 is
`37870842d96ec75c7a990bdabc37455221d4cffba9c7a47bac0ff4aeea49b4ce`.
No new stress, forced-error, interruption, recovery or existing-profile lifecycle
experiment was introduced. Independent frozen-source implementation acceptance
is required before normal PR merge. Full transitive descriptors/providers/default
dependencies/constraints/effects, neutral admission/reanalysis, immutable IR,
row execution, MySQL locking, performance, serving and release remain OPEN;
issues46/15 and the overall goal stay OPEN. Standing exclusions remain in force.

### Variable catalog payload implementation: independent acceptance

Independent implementation review accepted the finite builtin-profile component
at `82c72068716545da8cb6dc964ee6ea8238b5a4c6`, tree
`bf81af33018a08b30d0f75621e6e40031d59a502`. That revision differs from tested
ed847 only in three unpackaged documents; all344 other tracked entries, the
Rust fixtures and all22 native build inputs are byte-identical. It does not
relocate runtime results from ed847 or native package results from c434.

C1 closes only for this implementation: fresh B class/attribute/physical-NULL
facts and exact owned AS references admit selected pinned metadata TOAST before
its descriptor is opened. The critical descriptors must already be initialized
within the continuous builtin bootstrap profile. Post-open validation is
supplementary and does not admit arbitrary catalog/provider histories. Review
also checked owned-carrier bounds and decoder lengths, all three snapshot
lifetimes, exact mode/provenance unions, error ownership, independent native
oracles, paired package/profile bindings and retained failures.

The review is `logs/review-native-variable-payloads-82c7206-implementation-v1`.
Its report/facts/command-manifest/1993-member nonself seal SHA256 values are:

- `96a95b453b9fcf1c1cbbe4ef7300a3abc0fde856df8d1881583bdee3179f0624`
- `3878f03471ceccaf643144cf843ae244cc7794a0e94176e22632bf97270f2662`
- `4980a99cb96750adf53fb63857b721225c2be1cef1044b9c91776cf063eccf72`
- `826eaa856216599420190f8c69ac520318afed063bc161e2228e31173f185913`

Actual final31/post32 exits are0; receipt hashes are
`76a7178e06bf41ae68faa73854123f2a1621be68f398963a565b91dff2942640`
and `5663665aaef22736bf77f0dd220d0d3f1552b5df7a8368cc6049752f937eed42`.
Review reader20's wrong-path exit1 and audit28's overstrict phase-two TOAST-heap
assertion exit1 remain preserved; distinct corrected readers21/22 and audit29
exit0. These are reader/audit failures, not native runtime failures. Phase two
legitimately selects only one metadata TOAST heap. Root45 exits0 rehashing2010
unique fixed review/companion paths; its facts SHA256 is
`9ab247519c01719df12af677d94ec297839558ebda82f8163a92889469442513`.

D1 remained open on the frozen82 documentation: ten catalog scans described
the prior mechanism, not the current implementation. The later documentation
leaf labels that count historical and updates these status records. Its exact
leaf and independent D1 readback are separate merge evidence and do not
re-certify executable code. The original implementation/evidence files stay
frozen; the correction changes only three unpackaged documents.

Acceptance does not supply runtime witnesses for malformed carriers, forced
errors, arbitrary chunk order or shared exact-pointer groups. Those branches
received source review; ordinary fixtures prove their stated finite cases.
Full transitive descriptors/providers/default dependencies/constraints/effects,
neutral statement admission, immutable IR, row execution, MySQL locking, complete
performance, serving and release remain OPEN. Issues46/15 and the overall goal
stay OPEN. Concurrent native two-phase transactions remain supported; the
standing work exclusions remain in force.

### Variable catalog payloads merged; transitive type investigation begun

[PR59](https://github.com/samrat-shamim/darmok-proxy/pull/59) was normally
squash-merged from independently accepted final documentation leaf
`b103f3995e928507b900927f106a73acada0b851` to public main
`6ad8d1314a894eeb05eafdf57a428fcc03a9afad`. Main has the exact accepted tree
`0d0a5d4d01ea8131365cad206c1df05be7be813c`, with parent012be. C1 implementation
closure remains at82; final documentation D1 closes only atb103. Test and build
results retain ed847/c434 source attribution. The three final doc changes leave
all344 other entries,22 native inputs and tested Rust fixtures identical.

Root46 creates the separate final-doc worktree without altering frozen82.
Root47 verifies the exact three-doc delta and all22 native inputs; root48 reports
repository boundaries0 for9 packages. Both actually exit0. Final-doc binding
facts `logs/native-variable-payloads-b103f39-final-doc-v1/facts.json` SHA256 is
`449720626f77adffcd8050ebb3c451e2a00145ea15dadcd4dca82393d83538e7`.
Independent final-doc report/facts/command-manifest/172-member seal SHA256 values
are, under `logs/review-native-variable-payloads-b103f39-final-doc-v1`:

- `2a84608b74a189a244f4839c85311d6ce812764c977e0ac48f4edc63205ccf77`
- `a9df80eec2aafb3aa3e1acef411b57b46c46ca19e1cd1ee5157fd0def9796c97`
- `10a309c287b6661c744c4fd7dc21db1c6037ff948ce818c6da3bcc98000def53`
- `427fd65fa5a974253e1966e35e8cd143255997338b5511e77dd11c43569d6964`

Actual final03/post04 exits are0; receipt SHA256 values are
`293f7f96a598d2dac98aea81a38064b93c3a341878429b77affe630dbd607842`
and `a62b9f08bc83bccb2602dfb29800f7b9be123bc4330abad8dbf7967de289c5f1`.
No new review reader failed. Root49 actually exits0 rehashing172 fixed members
plus completion companions:189 unique paths. Its facts SHA256 is
`4c1c7bc8362fa74a0a0c84e2457a211a46ce1fb626acc79276a6ccad31741db9`.

Root50 push,51 PR creation,52 exact source/body readback and53 normal squash
merge actually exit0. Root54 independently checks merged head/tree/parent,
PUBLIC personal ownership, unchanged generic description and an empty eligible
PR list, with PR4 filtered before stdout/evidence. Merged readback facts SHA256
is `3eb5888cd28e8f8b58dcf2271338f3a4c214d341d4ce38e468c72a446a52751f`.
There is no hosted-CI or release-publication result.

Root55 issue-body preparation actually exits1: its required OPEN-state check
caught GitHub's automatic issue46 closure during merge, before creating a new
body. The original returned JSON and raw failure remain immutable. Root56
removes the negated closing-keyword phrase from the PR body; root57 reopens the
unfinished full gate. Distinct root58 prepares bodyv10b and root59 updates it;
all four actually exit0. Root60 independently observes issue46 and issue15 OPEN,
the exact updated body, merged PR59 and its corrected wording. Its facts SHA256
is `d7a934ca262ac765b196d4739ce0dec63aa39c32cf97782c843369da4dbe5727`.
The issue46 body SHA256 is
`4f4e62f7ce36b38d6549f32856e696387f77e52ca4bcedb791e4ad36c006b0f7`;
corrected PR body SHA256 is
`6c10291086f24d4bc7c72dcc4b6b5c05f7bde7dff203a5f8c46ce01a8fb7aaa7`.
This bookkeeping correction changes no accepted source/artifact/runtime scope.

The next [transitive type investigation](native-transitive-type-closure.md)
begins from merged6ad8. Separate root01 fetch and02 worktree creation exit0
without altering accepted worktrees or existing native profiles. Root03 primary
capture actually exits0 at clean6ad8:34 pinned bodies,17 per major, with8 selected
cached bodies rehashed and26 new HTTP-200 bodies. Facts and64-member nonself seal
SHA256 values under `logs/native-transitive-type-closure-primary-v1` are
`a6cbe480715de1edfa3b15184014d98f1152e3b3f0ff916bce9e80e62bdfa009`
and `04346a5952b1cf202851e237cc2fc84ded598984909d5379331b4051d2180773`.

The investigation distinguishes structural domain/element/companion/composite/
range edges, enum labels and opaque domain constraints from executable provider
and expression admission. A candidate coherent map/selected-payload collection
requires33 definition-catalog passes plus an unresolved bootstrap sequence;
neither total scan count nor descriptor writer coverage is accepted. New
metadata heap/TOAST descriptor admission, guard-mode conflict proof and cold/
invalidated paths remain OPEN before native implementation. This checkpoint
changes only unpackaged documentation and claims no new runtime or artifact
certification. Independent design-gate review is required. Full semantic closure,
neutral statement admission, immutable IR, execution, MySQL locking, performance,
serving/release, issues46/15 and the overall goal remain OPEN. Standing work
exclusions and concurrent native two-phase requirements remain in force.

Targeted root04 source capture actually exits0 at the still-clean acceptedb103
worktree; its tree remains identical to merged6ad8. It rehashes the34 v1 bodies
and adds four HTTP-200 lock/table-writer bodies, for19 per major. V2 primary
facts/46-member nonself seal SHA256 values are
`9496d7a04ac4211a5062e19076314dca298de00b5356e3f302038de1a84f5422`
and `a7af45690161cb86f44560ae6c7ba47b7bfb6e0ca9a3f204500ec2265c212d17`.
Root05 actually exits1 on an overstrict source-reader assertion that expected
one occurrence of a native option; it occurs in both definition and parser
tables. Its empty child folder and raw streams remain preserved. Distinct
root05b rehashes the two pinned reloptions bodies, captures the lock-level
function and both option occurrences, and actually exits0. This evidence is
separate from the38-body v2 primary list and no runtime is inferred.

The paired option/writer/matrix source chain shows why AS plus RX alone cannot
freeze the proposed NULL-options bootstrap profile: selected ordinary heap
options can use ShareUpdateExclusive, compatible with those reader modes.
The investigation records this counterexample and leaves stronger-guard versus
complete descriptor/provider-path admission, writer coverage, ordering,
retention and performance OPEN. No native implementation or catalog mutation
is introduced to bypass that unresolved proof.

### Transitive investigation merged; metadata bootstrap paths narrowed

[PR60](https://github.com/samrat-shamim/darmok-proxy/pull/60) normally
squash-merges independently accepted documentation source
`75de1386f4a5b2cb4645a3f382eef4e55c31fc8f` to public main
`7de81ccf392105e85f888bfd7ea9845f9dff3d80`. The exact accepted/main tree is
`a573edc282c675ef7fa514d93be88e3357371de0`, parent6ad8. This closes only the
finite source-investigation checkpoint; the new descriptor bootstrap and full
type/provider/statement/runtime gates remain OPEN.

Root06 investigation binding actually exits0:348 entries versus347 in its
merged parent, one new and three modified unpackaged docs,344 unchanged prior
entries,22 unchanged native inputs and unchanged tested Rust fixture. Root07
boundary check actually exits0 for9 packages. Binding facts SHA256 is
`0278d1831f2fee031b0a90f562d90e21fa25b1087010d65129b4601576aab891`.

Independent review under `logs/review-native-transitive-type-closure-75de138-investigation-v1`
accepts only that investigation. Report/facts/commands/616-member nonself seal
SHA256 values are:

- `9d14d8c7945bfd4a56c1474f9852928cb3753b5a8663c8da169c4fb1f414c389`
- `fe877c01f381c520e84f51dcfedd59b95d9604f5e5820e510212bc02d920839a`
- `c152a240e68b8e6b43675d04ef139b5b650be531459284e33d7109761008b5b5`
- `f036de72177467656123e7015e6e01f0a88f51732af430acc3bcf3ff15488f43`

Actual final14/post15 exits are0, with receipt SHA256 values
`d8badbe569b16c0653cc7ff459d3f69b405383272df5ab2f7caaaa6f6442b04a`
and `14631e00ca8c3d22ba533ba78b31b9d3a601499fc72135669d5a11f736393a3a`.
No new required correction is found; all live reads release before root mutation.
Root08 actually exits0 rehashing616 fixed members and16 separate completion
companions/637 unique paths, without recursively reopening earlier runtime
review graphs. Its facts SHA256 is
`0c3432690431563c204f214df62ab3666b209e46fe4c7c611ce2017f69f2d0c6`.

Root09 push,10 create,11 exact source/body/base readback and12 normal exact-head
squash merge actually exit0. PR wording relates to issues46/15 and contains no
closing directive. Root13 independently reads merged source/main/tree/parent,
PUBLIC personal ownership, unchanged generic description, an empty eligible
open PR list with PR4 filtered before stdout/evidence, and both issues OPEN.
Its facts SHA256 is
`7c004c882993faf85c49d50cbe7d0153194c55539a246399522020a3724eb490`.
The PR body SHA256 is
`de4b2d985e63e99887c400d8bb92deba9beae14123c8c15d03068b52ce64f57b`.
There is no new hosted-CI, native runtime/artifact or release result.

The [metadata bootstrap follow-up](native-catalog-bootstrap.md) begins from
merged7de8 in a separate branch/worktree. Its distinct root01 fetch and02
worktree creation actually exit0 without changing frozen accepted sources or
existing native profile lifecycles. Root03 actually exits0:26 selected cached
bodies rehashed plus eight HTTP-200 trigger/index/vacuum/analyze bodies,17 per
major. V1 primary facts/47-member nonself seal SHA256 values are
`7a5dbd8882cb8c58e27da3b403ffc75a275373ad363d4010eac1953de32fd2b5`
and `944c1b1126b165ea07edb61160c1542e83cf8d498a8c063a012fb7949751b3fa`.
Root04 passive path inventory actually exits0; its complete raw output is
preserved even though the wrapper displays only its tail.

Root05 actually exits0 capturing38 exact selected cold/options/writer bodies.
Facts/41-member seal SHA256 values are
`3ba8fc22f68310b2b81871c1748db489b37aa34a5192442af787768408a836e1`
and `82b491a77259e8f3410e63d6c68f058599b3397758735080a6b45860572c0d2d`.
Distinct root06 captures11 additional native rebuild bodies, including18's
separate rebuild function, and actually exits0. Its facts/14-member seal are
`dbf0816b616b7d0c6ca4cfb9bb1f4307c8a5bf886c80c8679829d6cc3c9f28aa`
and `d650f1b5e5d83acaad4fe222a6cc502f7578c8936f50e24e98b5576f84c666d2`.
No native invalidation or error experiment is inferred from these source reads.

Root07 actually exits0 rehashing the34 v1 bodies plus four selected array/btree
bodies and capturing two HTTP-200 btree utility bodies:20 per major. V2 primary
facts/48-member seal SHA256 values are
`a13755f9ec985ccddcb06df32197d784e1e0dc8ef0ebd38858cacf90452e6bc4`
and `78b61856fca1b4cbeb7f9a8b771f666ed522d2898710eea46468b06891e87ea7`.
Root08 actually exits0 capturing16 selected global/local/array/btree bodies.
Its facts/19-member seal are
`8e49bf80aea0a3b7a196a712e827ff65e7991ff2b6c42f33cc3c440bc545054a`
and `e5313ad065aab8917f3ca7c03f9716daf1814331e5b70e4cf8ccd1764e6063b9`.
Root09 saves five actual native-major source differences and actually exits0;
facts SHA256 is
`dcd790588ea2628063f5394be446376cdff04919bb7efda6123a1e14583f5d2d`.
These are source byte/comparison facts, not complete semantic reviews of all
40 files/65 bodies or new runtime results. No helper/source mutation occurs
during any recorded root command.

The follow-up prefers investigating complete supported global heap/builtin-btree
option admission with AS/RX over adding global SUE/Share coupling. It traces
global NULL fillers versus callback-capable local registrations, major-specific
descriptor rebuilds, and the missing entered-cache/owner witness. This does
not close registry/profile, writer, bootstrap, whole closure or implementation
gates and does not select a final observation/pass count. Independent review
is required before changing C. Concurrent native two-phase support remains
required; issues46/15 and the overall goal stay OPEN. Standing exclusions remain
in force, including security work, compiler PR4, hosted CI, publication and new
stress/error/recovery/existing-profile lifecycle experiments.

### Bootstrap investigation merged; entry and builtin dispatch source follow-up

[PR61](https://github.com/samrat-shamim/darmok-proxy/pull/61) normally
squash-merges independently accepted source investigation
`0b39063df91d841fadc9b8f5ef2f0ec8fa4d995e` to public main
`2bfd24534b12a48fbc1f51ed72037f0c355a86ae`. The accepted/main tree is
`94c60ad152299dc373c63c11b129685c4c5ff693`, parent7de8. The checkpoint leaves
bootstrap, registry/provider/owner, complete writer/field, sequence,
implementation/runtime/full statement and release gates OPEN.

Its root10 investigation binding actually exits0:349 versus348 opaque Git
entries, one new and two modified unpackaged docs,346 unchanged prior entries,
22 unchanged native inputs and the unchanged tested Rust fixture. Binding facts
SHA256 is `e0ec5abba1cb5698d5fd306335845255f96f2ecf400cc56416c521536e2709e2`.
Root11 boundary check actually exits0 for9 packages.

Independent review at `logs/review-native-catalog-bootstrap-0b39063-investigation-v1`
accepts only the source investigation, with no required correction. Its
report/facts/commands/602-member nonself seal SHA256 values are:

- `3fa020e054ac62100cb7552471ac767e98d1f5f0e25a6c306cf7412f787a5abc`
- `24e657276f4e3ce4687d7bc38a1ccea941a5793b676654c8bc1b46e7b5e6253b`
- `f43e6f779dd4b6f49f603e9cb5295b8c9a5976e771c6e86c3fdcc1b2349dab11`
- `b8286f8a3c437e2aae19210636e57cb2fcda8379880d7264dacfecc4837e6a3f`

Actual final11/post12 receipts have SHA256 values
`c287068333b50c4fcafa4951171d10c704103163d913f3547d7ba3452a8f861b`
and `4ee946f334aba5513df54247fb68b1d7d2a90666c4e727766175b42d7c66a549`.
Audit, final, post and completion invocations actually exit0; post rehashes602
fixed members and observes exact clean source. All live reads release before
root mutation. Root12 actually exits0 rehashing those602 fixed members and16
separate completion companions/623 unique paths. Its facts SHA256 is
`f2bf21b4062dfe5296c66fd77811b9778b5dfe9bba43859e74567bfb62a44429`.
Prior616/637-member and older runtime graphs are not recursively reopened.

Root13 push,14 create,15 exact head/body/base readback and16 normal exact-head
squash merge actually exit0. Root17 actually exits0 independently reading
merged source/main/tree/parent, PUBLIC personal ownership, unchanged generic
description, an empty eligible PR list with PR4 filtered before stdout/evidence,
and issues46/15 OPEN. Its facts SHA256 is
`741d9ae3bd38cc81442bdaf91424587551f8fe9a3ba486c18a1b20e33fcd7881`.
The PR body SHA256 is
`f2bec7d8a59bbe2b6970a1270242de8ac864fdb876288ff39e5274ed8e48e041`.
There is no new hosted-CI, native runtime/build or release result.

The [entry follow-up](native-catalog-bootstrap.md#original-builtin-heap-dispatch)
starts from merged2bfd in a separate branch/worktree. Distinct root01 fetch and
02 worktree creation actually exit0 without changing frozen accepted sources
or existing native profile lifecycles. Root03 actually exits0:12 selected cached
lifecycle bodies rehashed and ten HTTP-200 handler/lookup/public-header bodies,
11 per major. V1 primary facts/38-member nonself seal SHA256 values are
`d524cfee52604717eef7da8e10c9cc0dd830f74edfdbaf96559e63b2501fe432`
and `275073aed8598e9cf05a7e473ffba3f8072c1e7ea2425108ea2692d3fa877336`.
Root04 passive API inventory actually exits0; complete raw streams remain
preserved beyond the wrapper's display tail.

Root05 actually exits0 rehashing those22 bodies and adding six HTTP-200
heap-handler/bootstrap-data/function-macro bodies:14 per major. V2 primary
facts/37-member nonself seal SHA256 values are
`72f2c88955846da4f06d534c7c454470af409a54a84238f38f1bebf7b3f085a6`
and `17678c719cf952f28ef275b1f77ccea50b91f649032f0c856b103a08c9dd0879`.
Root06 actually exits0 preserving54 complete function bodies and six complete
native macros/data rows. Its facts/85-member nonself seal are
`d64874de259098edab32ea295badbfb5550e9606f3246c1c63977ad1bacaf721`
and `7579d52b615006311fce9013ef88d52b0cc88c5f1a1a2c71baf804b51785d67f`.
Root07 saves eight actual paired source differences and actually exits0; facts
SHA256 is `78dc363be81b4f1a961c550035bbfeb493a0dfd0d5694405ec438f8f38b86aa0`.
These counts are byte/provenance facts, not semantic acceptance of every28-file
or60-record path or new native runtime. No helper/source mutation occurs while
any recorded root command runs.

Targeted source reads narrow original builtin heap dispatch, ordinary portal
reference cleanup, whole-cache reset and18's additional relation-sync callback
channel. They do not establish an authoritative entered-cache/owner or registry
witness. Construction from closed private history and the actual native
callback/provider footprint remains an investigation direction. Independent
review is required before C changes; all bootstrap/whole statement/runtime
gates, issues46/15 and the overall goal remain OPEN. Concurrent native two-phase
transactions and the standing exclusions remain in force.

### Entry investigation merged; passive module-footprint source follow-up

[PR62](https://github.com/samrat-shamim/darmok-proxy/pull/62) normally
squash-merges independently accepted source investigation
`8602fe2b90ccf1e46aa2a5a5860e14cd6fdd5e18` to public main
`2a18f9d27cef53b29c9c7b29cd3bc6f9049a400b`, tree
`533989d886c54d1f6e8185d8292733a0c8659a16`, parent2bfd. D1 corrects one
missing hexadecimal character in the earlier root10 binding SHA256; it closes
only at8602, while the frozen fbc947f review retains D1 OPEN. Root10's first
review rehash actually exits1 because its helper assumed a nonexistent
post-facts key; the failed streams and empty child directory stay preserved.
The separately named corrected root10b rehash actually exits0. This is an
evidence-reader correction, not a native product result.

Root12 correction binding and13 correction-review rehash actually exit0.
Their finite facts SHA256 values are
`d0ff773254de462e038aa9a67c96e10f95afb58fd809460f974ab1440505fbec`
and `c12668ea877741f6599d586b42b0ecc5bb05704263edf58ae6e4a6bb89df2195`.
The latter rehashes220 fixed review members and16 separate completion
companions/241 unique paths; it does not reopen older488/509-member graphs.
All reviewer live reads release before root mutation. Root14 push,15 create,
16 exact head/body/base readback,17 normal exact-head squash merge and18 merged
readback actually exit0. Root18 facts SHA256 is
`5769177a87c2afe48b9d345928543faacc1d3c28b0490fa8cb36931316620be8`;
it confirms the PUBLIC personal repository, unchanged generic description,
empty eligible PR list with PR4 filtered before stdout/evidence, and issues46/15
OPEN. There is no new hosted-CI, native build/runtime or release result.

The [module-footprint follow-up](native-catalog-bootstrap.md#passive-module-footprint-and-its-limits)
starts in a separate branch/worktree from merged2a18. Distinct root01 fetch and
02 worktree creation actually exit0. Root03 actually exits0 with six selected
native source bodies: two cached public headers rehashed and four new HTTP-200
loader/preload files, three per major. Primary facts/13-member nonself seal are
`b9b6468c15efcae3d1df34a33652621278121a606608cb31b5e7ca8cceccf612`
and `04b38ad11256c416bb5f61e446a3de043da5765ddd0691453ecaad6e936e6190`.
Root04 actually exits0 preserving21 complete function bodies, two native struct
definitions and two complete public API sections. Its facts/34-member nonself
seal are `1728cbc47265a3aa8e2b018be1b428ee9ffe71d183049dab17ad70d5c4276586`
and `92289a247dafc50ce26fba166c9b73e7cd03b41d17120ff4b2675c82ec22c2d5`.
Root05 saves five actual paired source differences and actually exits0; facts
SHA256 is `c3bc8e5f806abaf2c0b01fe2fa0179ef6612b95d08090a1da7778f2c62b68425`.

These are exact byte/span/difference facts and targeted module-list/preload
lifecycle findings, not complete semantic acceptance of every source file or
backend startup. Passive getters do not certify callback/registry/reference
history; the list publishes new entries after optional initialization. Capacity,
stable observation, actual executable identity and closed private history need
their concrete proof before C changes. All entry/bootstrap/whole statement,
implementation/runtime and release gates, issues46/15 and the overall goal
remain OPEN. Concurrent PostgreSQL two-phase transactions remain required.
The standing exclusions remain in force, including security work, compiler
PR4, hosted CI, publication and new stress/error/recovery/existing-profile
lifecycle experiments. No source/helper mutation occurs during a recorded
root command or a reviewer's live reads.

### Module-footprint merge and private command ownership

[PR63](https://github.com/samrat-shamim/darmok-proxy/pull/63) normally
squash-merges the corrected source-investigation leaf
`ee481de178485e62cbb2e52d1dba9bfc40033406` to public main
`4d2a81b5432474b0ac714cf3ffc4c140773f4f08`, tree
`e601044b7d81bbe6ca2fe3654ab0a6c015b7ac62`, parent2a18. D1's Linux
qualification closes only at ee481de; the historical a6e0b29 finding stays
open. The correction review accepts source investigation only. Root11's finite
review rehash and root16's merged readback facts SHA256 values are
`c99322e3db9682eeaf55f2ec06b9ca2a53e7b7807993a05e77f45e0c73fda1e4`
and `c6f70dc4b804458c98f582f8065691d63009effe779aca56b161e3542ab075c1`.
All live reads release before subsequent mutation. The readback confirms the
public personal repository, generic description, empty eligible PR queue and
issues46/15 open, with PR4 filtered before stdout/evidence.

The [private command-history boundary](native-command-history.md) starts from
merged4d2 in a new isolated worktree. Root03/04 source capture,05 selected
functions/spans and06 paired differences actually exit0. The primary v2
13-member nonself seal and selected-functions 35-member nonself seal SHA256
values are `ddbf02877f61ddf6a19fdffc40fd9b93e854f82a84f32bd7094c156071031a01`
and `50fa4db037696cf715bfaccf7cc5daf7cc3370e85be81f2dc004826bc4ccdc0f`.
These source reads do not certify complete native startup or a new runtime.

The implementation separates setup/verification ownership from the query owner
through a shared private connection mechanism. Fixed installation SQL and native
mechanism inputs remain unchanged. Current-revision checks are recorded below.
The native entry/bootstrap/whole-statement, serving and
release gates, issues46/15 and the overall goal remain open. Concurrent native
two-phase transactions remain required; all standing exclusions remain in force.

The implementation leaf is `c5fa88ef8d9d8661202c8f6901e918e7c3233436`, tree
`b3ec4c989e96089478e7e24e4db9cb149086dec8`, parent4d2. Root07 formatting,
08 workspace Clippy all targets/features with warnings denied,10 offline tests,
11 required suites and13 repository checks actually exit0. Offline tests report
1691 passes, zero failures and88 ignored database/other fixtures across41 result
groups; the database fixtures are required separately. Each PG17.11/18.6 matrix
runs 77 owner,10 BigDecimal transaction,10 BigDecimal frontend and4 CLI process
fixtures with zero failures/ignored. The new ordinary fixture proves a fresh
query session after awaited setup disposal, not native entry admission.
The eight-suite facts SHA256 is
`e6b30a0196bee7d1900527c7ce3a4f56d0e6d7b1326f796871a9e720e97e678b`.

Root09/12 read-only profile audits each run48 actual successful readers. Their
facts SHA256 values are `f5ea9a66c53715724602ff73c853f2c0d7443302626f6c9b34d19913e60a4f05`
and `16a63652b50cd5667bd0a4b209a82d2f4dbecb0541198a26f8871150ca165329`.
All eight identities, start times, restart counts, settings and library/header
bytes agree before/after; no fixture databases remain. The 22 native inputs
are unchanged. This is current Rust/CLI validation against the existing native
packages, not a new native build, full startup proof or release acceptance.
All remaining gates and exclusions still apply.

The independent review at `1b0fe078063e87670bb0ed0f19d339352836de5e`, tree
`9d474c09174c221191b1d5bc2429a68be177b5ff`, accepts the finite Rust owner
implementation carried unchanged from testedc5. D1 remains OPEN historically
at1b0: the old 6217/20-input package paragraph was labeled current. This separate
documentation leaf labels the old package/README acceptance historical and
points to current c5/22-input c434 reuse. It changes no Rust, SQL, native input
or runtime behavior; independent correction review remains required.

Reviewer audit31 succeeds after preserved own21 actual1 (wrong manifest path),
22 actual2 (wrong directory) and27 actual1 (wrong ordered preload expectation).
Those are evidence-reader failures, not product/runtime results. Actual final34,
post35 and completion36 exit0. Root15 rehashes1170 fixed review members plus16
completion companions/1192 unique paths and actually exits0; its facts SHA256 is
`7c2f3e43477abf4d98722a0b67f4b1912c944d9d045fe9d0860ece17d99482fd`.
Report/facts/completion-map SHA256 values are
`dffe02e56d244862f84ef7edab19e1e9469aebb99f84efbc5628501d94b40c19`,
`07188c80bab5ebe3287c39a9d4c5752a82b822bf7aa64ab3abe7c702bdc18f5d`
and `70956592c8f4d03f7d5762a1f1806af5a7c769c47a3e6e87195267482aa02d21`.
All live reads release before root16 creates the correction worktree. Prior
818/1170/1192 graphs stay frozen; correction checks use fixed anchors only.
No new native build/runtime/hosted-CI or release result follows. Concurrent
native 2PC and all remaining gates/exclusions, issues46/15 and the goal remain
open.


### Separate setup/query owners merged; local prepared commands begun

PR #64 was normally squash-merged from independently reviewed c7db9fd to
`cd0e0fc6936319e8a660517c1f770a7dee1c34ee`, tree
`a24d9b497c109694fc4c22f9c571b30ac36a2406`, parent4d2a81b. Root23 verifies
that exact public personal main, unchanged generic description, no eligible
open PRs after filtering PR4, and issues46/15 OPEN. Readback facts at
`logs/native-private-history-c7db9fd-merged-readback-v1/facts.json` have SHA256
`3d2a10566b66fff93c967338ebbd576c6964f2a8e63ef469f29768952da12457`.
The independent correction accepts final docs only atc7; the original1b0/D1
remains historically OPEN. Root18 rehashes162 correction fixed members and
completion records,184 unique paths, with actual final/post/completion0.

The next [local prepared-command component](prepared-commands.md) starts at
that merged main in an isolated feature worktree. It owns source-admitted
local templates, dynamic variable reads and direct parameter metadata under the
TCP connection, with distinct prepare/execute/reset/close response contracts.
No native C, SQL, catalog admission or existing-profile lifecycle is changed.
Its current implementation checks and independent review remain pending.

Ordinary stock observations use the existing MySQL8.4.11 container and pinned
PyMySQL1.1.2. The first observer's mutable mysql:8.4 tag differs from the running
container and fails actual1 before any SQL case. V2 binds the existing immutable
image and completes4 cases; V3 adds initial NULL, tiny-integer and unsigned
transitions and exposes numeric-to-string coercion/warnings. V4 additionally
observes ROW_COUNT/FOUND_ROWS; the deprecated FOUND_ROWS call's warning is not
proof of an earlier condition count. V5 adds independent PING counts showing
prepare/reset clear and known/unknown close preserve. Its facts at
`logs/prepared-frontend-reference-v5/facts.json` have SHA256
`8d79e1ed1b946b21669cdff1f54e037f3bfa7788421185310646fc247d58010e`.
Original failure/earlier observations stay immutable. The checked-in corpus
retains unsupported stock conversions separately from proxy-supported paths.

The pinned sql_prepare.cc primary capture is HTTP200,123236 bytes,SHA256
`69fce8b432c0aa8dded49fdb0c10e04f3cbc476335b4682e80ebb102fe7645ea`.
Selected preparation, precheck, execute, parameter-reset, statement reset and
close bodies inform the component; the full file is not semantically accepted.
Source bytes remain outside the Apache distribution. New template/packet
allocations and unimplemented coercions are explicit in the component contract.

Native entry/bootstrap/provider/registry/reference/writer/sequence, whole
statement/table execution, native prepared/DDL validity, full MySQL coercions,
serving, performance and release gates remain OPEN. Concurrent PostgreSQL
native2PC remains required. Security, compiler PR4, hosted CI, new native stress
or recovery experiments and release publication remain excluded.

The first prepared candidate76ef217 has a preserved strict Clippy actual101:
retaining a full Expr made the prepared-cell enum352 bytes, and two fixture
reports were unused must-use values. The corrected design retains only the
classified variable/scope facts, without boxing a whole AST or adding a per-
variable allocation; fixtures assert their returned state. Current corrected
checks and independent review remain pending. Earlier source/artifacts remain
revision-bound; no passing result is inferred from compilation of a predecessor.

At corrected revision `cbf5b1c9a82e48a0f9715fa48e6de67b9446720e`, format,
workspace all-target/all-feature strict Clippy, repository boundaries and offline
tests pass (1,691 passed, 0 failed, 93 ignored in 41 reported groups). All eight
required PostgreSQL 17.11/18.6 suites pass: per major, 82 owner, 10 decimal
transaction, 15 decimal frontend and 4 CLI tests. Each CLI group preserves its
32 child exits (15 actual 0, 17 expected actual 1). Root10–17 receipts and raw
streams are retained under `logs/prepared-frontend-cbf5b1c-*`; required-suite
facts have SHA256 `39c03dc3c2c43d02f008b3c39c8ffcee863af3083fedf6f11f25f3049fed7850`.
Before/after audits observe the same eight profile identities, starts, restart
counts, libraries, headers and settings with no residual fixture databases.
These observations certify that revision only.

Root18's v6 stock observer completes 15 cases, preserving the preceding five
and adding signed/unsigned integer-family bounds. Facts at
`logs/prepared-frontend-reference-v6/facts.json` have SHA256
`8b1d05ff1399791a57514babdb28103713c7fa7d6280903962618c0d63f4cac3`.
The new INT24 inputs return stock errors 1210/1835 rather than integer rows;
the pinned `set_parameter_value` handler has no INT24 value branch despite
the metadata setter accepting that code. The local admission now explicitly
rejects INT24 with 1235, and its corpus marks these as unsupported observations,
not matching stock error packets. TINY, SHORT, LONG and LONGLONG signed/unsigned
bounds produce the expected LONGLONG results. The generic wire decoder is not
a semantic admission authority. Current expanded checks and independent review
remain pending; full native execution and release gates remain OPEN.

### Expanded prepared-command implementation verified

At `4bffee43f7ed2361b86f16381ea24dacfb0a8641`, root20–27 all have actual exit 0.
They execute `cargo fmt --all --check`, workspace all-target/all-feature locked
Clippy with `-D warnings`, the committed stock observer, the locked offline
workspace suite excluding `darmok-postgres-tests`, before/after profile audits,
all eight required native/CLI suites and `python3 scripts/check_repository.py`.
The offline suite reports 1,691 passed, 0 failed, 93 ignored across 41 groups.
Per PostgreSQL 17.11/18.6 major, owner tests report 82 passed, decimal transaction
tests 10, decimal frontend tests 15 and CLI tests 4, all with 0 failed/ignored.
Each CLI suite's 32 child receipts retain 15 actual exit 0 and 17 expected exit 1.
No existing profile was restarted or replaced. The 22 unchanged native build
inputs remain bound to the existing c43443d package/profile facts. This is Rust
and packet validation, not a new native module build or complete engine gate.

Evidence under `logs/prepared-frontend-4bffee4-*` records commands, raw streams,
source before/after and immutable identities. Required-suite facts have SHA256
`9e460a9461c5f63135bca72c2a0e94e437c05f10a0e986da7bda1cef832750af`;
stock observation facts have SHA256
`34ff89f2aa3284ba2b536acc6588a5fe89bdea392d00b8dde4552aa7b8db5bd4`
(15 cases and 4 condition-count checks). Profile before/after facts have SHA256
`98e00dd5bd46825c6da75ae59e6a3f160f866702571d773cb9e2cc0f9ca2f41d`
and `b2dc29fa5b40aefb224f77dc13a95c1469e23f3e89be879c88cdf1bf04199d6e`,
with the same eight IDs, starts, restart counts, settings, libraries and headers,
and no residual fixture databases. The intermediate d35422a format check's
actual exit 1 remains preserved; 4bffee4 fixes only its required line wrapping.
Documentation now records these completed checks; its subsequent revision must
be proven identical for tested runtime/corpus/observer inputs before acceptance.
Independent review remains pending, and all previously open native, full
prepared, performance, serving and release gates remain OPEN.

### Prepared registry accounting correction

Independent review of clean `8ed1f8b4298f37bfb3b11e83fcb7598c9c470c4c`, tree
`7a4173023edbd6f251df12134f70e0bc9e7d44dd`, finds one required correction C1.
The new public registry `get_mut` exposed an entire statement; replacing it
with another registry's statement could change source/ID/parameter facts without
updating SQL-byte accounting or the map key. The owner needs only opaque plan
mutation. The correction exposes registry `plan_mut(id) -> Option<&mut P>` and
keeps whole registered statement metadata immutable. Its regression mutates the
payload and verifies unchanged identity/types plus count/source limits, removal
and monotonic IDs. No SQL execution behavior or native input changes.

The rejected review remains frozen at its original worktree. Its final40,
post41 and completion42 actually exit 0, but C1 remains OPEN at that revision;
passing audit commands do not accept the feature. Root29 rehashes 1,071 fixed
members and completion companions, 1,093 unique paths. Facts at
`logs/review-prepared-frontend-8ed1f8b-v1-root-rehash-v1/facts.json` have SHA256
`aa23d3f51aef678535adcb02f075b2695a1418f5cc4ca79066144605098f486c`.
The review's packet-summary bookkeeping reader37 actual1 remains preserved;
corrected position-based reader38 actually exits 0. No second product finding.
The correction is on a separate branch/worktree; fresh validation and a finite
C1 review remain pending. Whole-engine and release gates remain OPEN.

### Prepared accounting correction freshly verified

At `43f835b4037b5751a80555dbf1b054e002293b25`, root31–38 all actually exit 0:
format, workspace all-target/all-feature locked strict Clippy, committed stock
observer, locked offline suite, profile before/after audits, eight required
native/CLI suites and repository boundaries. The offline suite reports 1,692
passed, 0 failed, 93 ignored in 41 groups, including the new plan-mutation
identity/metadata/accounting regression. Per PostgreSQL 17.11 and 18.6, the
required suites report 82 owner, 10 decimal transaction, 15 decimal frontend
and 4 CLI tests, all 0 failed/ignored. CLI child receipts again preserve 15
actual exit 0 and 17 expected actual exit 1 per major. The stock observer
reproduces all 15 cases and four condition-count checks.

Raw streams and receipts remain under `logs/prepared-frontend-43f835b-*`.
Required-suite facts have SHA256
`dfe8e554f8f5e2392fa72796919db4013fc5a834c66374215e3903fda3de2eed`.
Profile before/after facts have SHA256
`cb9295a309f54b162da8fdbcb7cc17550e01e2d86beb11cb7fb66f553b53e7c0`
and `1e9793d63430521a9f97a90938c7765f828da62a4c0492f8fb0c5d178bd9a54e`:
all eight profiles keep identical IDs, starts, restart counts, settings,
libraries and headers with no residual fixture databases. The same 22 native
build inputs and existing package manifests remain fixed reuse anchors, not
new native certification. The subsequent two-file documentation revision must
be bound to these unchanged tested inputs. Corrective independent review still
decides C1 closure; full native/statement/serving/performance/release gates,
issues 46/15 and the goal remain OPEN. Concurrent PostgreSQL 2PC is required.

### Prepared commands merged; fixed native callback history traced

PR #65 was normally squash-merged from independently accepted
`5064131adf1cc1cfda5421a406f83e8823909f0a` to public main
`e8b0dde48e4484ad4f9bf0308f4c5a5d79a0787b`, tree
`e70405281d7e2067fda7c3fdd0a3bb381e9372ea`, parentcd0e0fc. Its two-file
documentation leaf preserves the 355 other entries of tested43f835b. C1 is
CLOSED only at506; the original8ed review remains historically OPEN. The finite
correction review retains984 fixed members, actual final13/post14/completion15
exit0, and root40 rehashes1,006 unique paths with actual0. Its facts at
`logs/review-prepared-frontend-5064131-correction-v1-root-rehash-v1/facts.json`
have SHA256
`297177b3e3702114832468786faf2db82586287a3156137c6f7ca1f8a7cbd48e`.
Root46 verifies the exact public personal main, generic description, no eligible
open PRs after filtering PR4, and issues46/15 OPEN. Merge readback facts at
`logs/prepared-frontend-5064131-merged-readback-v1/facts.json` have SHA256
`059dd1e71e78eced95cb21c1a6332e548371a390e190c7ba349471856885322d`.
The local prepared component is accepted; native/table prepared execution and
the complete engine gate remain OPEN.

The next isolated branch starts from that exact main and traces the query
owner's fixed native commands. The [paired source findings](native-fixed-command-history.md)
identify post-parse and utility hooks, the distinct string object hook after
SET, GUC assignment/restoration hooks, native resource-release callbacks and
savepoint completion. The current Darmok utility interception is a required
provider path; an all-NULL hook profile is not the proposed witness. The
construction requires a bound startup base, preservation through every allowed
transition and exact pre-open descriptor/reference facts before new C readers.
Those implementation gates are still OPEN.

Root03/04's actual0 primary captures at basee8 rehash eight then twenty cached
bodies and add twelve then eight HTTP-200 bodies, yielding28 pinned source files
across PostgreSQL17.11/18.6. Root05 selected reader actual1 remains preserved:
the assignment calls live in set_config_with_handle rather than the delegating
set_config_option_ext wrapper. Corrected root06 actually exits0 and saves120
selected records:92 complete function bodies,8 GUC rows,2 macros,12 bounded
call sites and6 bounded utility spans. Eight of60 paired selections differ,
including PG18 search-path reporting, AIO/type-cache cleanup and parallel-worker
check variants;52 have equal selected bytes. Source counts and extraction
success do not mean complete semantic acceptance of those files or variants.

Commands use `python3 capture-native-fixed-history-primary-v1.py`, its v2
callback capture and `python3 capture-native-fixed-history-functions-v2.py`
through the fixed root recorder with unchanged clean e8 source before/after.
Primary v2 facts at `logs/native-fixed-history-primary-v2/facts.json` have SHA256
`8948e9836c6fb9aaa7059a6704cbd66ffe893ea4287f7a04d1aa0126ae2f7990`.
Selected v2 facts/seal at `logs/native-fixed-history-functions-v2` have SHA256
`4f7d0c9a48a9f67f757568fce117cf996ff4b9dbc9d7845c9a251e6f1fd16dd1`
and `f9a976c28d5c876f32009ad69703ae5ff11ddb105a4c253996ae21137fbcedc9`.
These are finite targeted source/provenance findings, not a new native build,
runtime experiment or recertification of historical review graphs. The same22
native build inputs remain unchanged. Independent review of this documentation
checkpoint is recorded below. Entry/bootstrap/provider/registry/reference/writer/sequence,
whole-statement/table execution, serving, performance and release gates remain
OPEN. Concurrent native2PC stays required; security, compiler PR4, hosted CI,
new stress/recovery or profile lifecycle experiments and release publication
remain excluded.

At documentation revision `80ec215d0c8f12f153e79ba4401fea70a7eeb842`, tree
`eb8d6346cf59623d44495506be32634eb9b211d9`, root07 repository checks and root08
`git diff --check e8b0dde HEAD` actually exit0. Root09's finite evidence binder
checks exactly four changed docs,354 unchanged base entries and all22 unchanged
native inputs. Its310-member nonself seal and facts are under
`logs/native-fixed-history-80ec215-evidence-v1`, with facts SHA256
`716897722163b5f05e70d8922e323c832eeb8bf1ff0ed61cfb1fa7c1495294ec`.
These documentation checks introduce no new native runtime result.

The independent review at80ec215 accepts the source investigation only, with no
required findings. Its exact source/span, paired-difference and finite provenance
checks retain474 absolute fixed members. Final26/post27/completion28 actually
exit0. Root10 rehashes496 unique fixed/completion paths and actually exits0;
facts at `logs/review-native-fixed-history-80ec215-v1-root-rehash-v1/facts.json`
have SHA256
`f9a38d2f0c3ca3f3ce026fda6f668aa0800a3ad0c1490f5708ff9fc3bd4951ec`.
The review report and facts SHA256 values are
`8ea1c0068ca5c33c6415cda44d835a451911b152823c7e93ef9005045e7bca8b`
and `0749f283ca1021cde8e73e4805c3c0e466c246b54870b23177b7b7c49f7d8941`.
Own20's actual1 bookkeeping failure remains preserved: it assumed every root
seal key was absolute. Its distinct corrected reader binds the sole relative
`scripts/check_repository.py` key to root09's exact recorded source cwd; there
is no product finding or evidence mismatch. All live reads release before
root11 creates this separate review-record leaf. Acceptance of the leaf must
bind its single changed release-plan file to the other357 unchanged entries
of accepted80ec215, without reopening the historical review graph. All native
admission, full-engine and release gates remain OPEN; concurrent2PC is required.

### Fixed-command findings merged; startup reference provenance traced

PR #66 was normally squash-merged from independently accepted
`41d289ccd7351747a2ac7c816bccce620b121d89` to public main
`94d86832f01e78a809317bf93b1a64506208ed42`, tree
`a0d39304d64d419627d52b5ce76f52831a02eb2d`, parente8b0dde. The final one-file
review record preserves357 accepted entries. Its independent160-member review
retains actual final10/post11/completion12 exit0 and no findings. Root15
rehashes182 fixed/completion paths and actually exits0; facts SHA256 is
`8b31504908120ff22e3e36c435529d0756339cc381baa6778e9944060dbab230`.
Root21 verifies exact public personal main, unchanged generic description, no
eligible open PRs after filtering PR4, and issues46/15 OPEN. Readback facts at
`logs/native-fixed-history-41d289c-merged-readback-v1/facts.json` have SHA256
`6ae686208eb6387e16751ef863e761be87fd6cbb227acd842e959f35c1e0a5d0`.
That accepted source investigation does not close native implementation gates.

The next isolated branch starts at that exact merged main. The paired
[startup/reference trace](native-startup-references.md) identifies intrinsic
pin constructors, distinct reader items/locks/copied tuple descriptors, parsed
init-file options and provider calls before complete file acceptance. Native
end-of-transaction cleanup is not a production full-cache census. The selected
PG17 startup transaction obtains GetTransactionSnapshot; PG18's does not.
The entry base must cover both construction and restored-file histories and
their actual provider/registration provenance before new descriptor admission.
No native implementation or complete startup witness is certified here.

At clean94d8683, root03's primary helper captures six actual HTTP-200 bodies
and rehashes six named cached bodies, twelve files across17.11/18.6. Fresh
relcache bytes match earlier named identities. Root04 actual1 preserves a
nonunique startup commit-comment selection. Root05 actual1 preserves the
PG17-only GetTransactionSnapshot-marker assumption. Both original helper and
partial output namespaces remain frozen. Corrected root06 uses exact bounded
transaction blocks and actually exits0, retaining34 complete function bodies,
two macros, six bounded startup spans and four selected startup calls. Four
of23 pairs differ;19 have identical selected bytes. Source counts and byte
equality do not certify whole files or all function semantics.

Commands are `python3 capture-native-startup-references-primary-v1.py` and
`python3 capture-native-startup-reference-functions-v3.py` through the fixed
root recorder. The environment fixes RUSTUP_HOME to `.darmok-work/rustup` and
CARGO_TARGET_DIR to `.darmok-work/native-values/target`; no Cargo or native
build/runtime command is added. Primary facts at
`logs/native-startup-references-primary-v1/facts.json` have SHA256
`861cdcba973269013ffb1b7b7e3c4d04880f299d78e403d5aaf8e22b408071c7`.
Selected v3 facts/seal under `logs/native-startup-reference-functions-v3` have
SHA256 `685e5d212ac78710cc0c1ebe6e4a70ae5946063cf0964556e6feba9f9e1bca86`
and `cd619659b504da46c6557854a8f0cf65aa7ac8e6385364943c77c05389068438`.
Independent review and current documentation boundary checks are required
before merge. No C/Rust/SQL, native input/build, runtime/profile lifecycle or
new stress/recovery result is introduced by this checkpoint. Concurrent native
2PC remains required. Native entry/bootstrap/provider/registry/reference/writer/
sequence, whole statement/table execution, serving, performance and release
gates, issues46/15 and the goal stay OPEN. Security, compiler PR4, hosted CI and
release publication remain excluded.

### Startup reference findings merged; native registry construction traced

PR #67 was normally squash-merged from independently accepted
`941147b3d1261cb6230f01d9381c28b9ce773996` to public main
`840dd53e2808ee0ca3c666ee4db0cf221b88bcc2`, tree
`b63e3833ac43ce9a5dfcacb9a1af4e2bf724a0d0`, parent94d8683. Its four-doc
checkpoint preserves355 other base entries and all22 native build inputs.
The independent review retains338 absolute fixed members and no required
findings. Actual final20/post21/completion22 exit0; own reader13 actual1 remains
preserved separately from corrected14 and audit17 actual0. Root10 rehashes360
fixed/completion paths and actually exits0. Its facts at
`logs/review-native-startup-references-941147b-v1-root-rehash-v1/facts.json`
have SHA256
`2733242d6f0c7b2e046c64d71319a57f9bf7b6b039d8f15413b1aef3c3be99c2`.
Root16 verifies the exact merged tree/parent, public personal repository,
unchanged generic description, no eligible open PRs after filtering PR4 and
issues46/15 OPEN. Merged readback facts at
`logs/native-startup-references-941147b-merged-readback-v1/facts.json` have
SHA256 `8e8289758e0758de3f5c64ddd2022c84d453a922dfb6d13a7f14412530601396`.
That acceptance covers the source investigation only, not native startup or
descriptor admission.

The next isolated branch starts from that exact main. Its
[native registry trace](native-reloptions-registry.md) identifies global table
construction and mutations, direct kind-mask registration, borrowed enum data
and string default-validation effects. Registration is not confined by the API
to newly allocated custom kinds. Global parsing selects unset definitions too;
native count assertions and name-based fill dispatch do not establish actual
registry/parse-table correspondence. Local filler/validator lists remain separate.
The selected public API supplies no actual global-registry census. The concrete
construction/witness remains OPEN before new descriptor parsing or opens.

At clean840dd53, root03 rehashes four named reloptions/btree bodies and captures
two actual HTTP-200 public headers across PostgreSQL17.11/18.6. Root04 actually
exits0 and saves96 selected records:60 complete function bodies,10 builtin arrays,
two enum-member arrays,20 type declarations and four bounded declaration spans.
Seven of48 pairs differ;41 have equal selected bytes. The builtin arrays have
22/16/3 HEAP/TOAST/BTREE definitions on17 and24/18/3 on18. PostgreSQL18 adds
two heap/TOAST definitions and explicit-set offsets, including
vacuum_truncate_set; the native option layout and presence cannot be flattened.
Other-kind rows captured in the full arrays are outside semantic admission.

Commands use `python3 capture-native-registry-profile-primary-v1.py` and
`python3 capture-native-registry-profile-functions-v1.py` through the fixed root
recorder, with `.darmok-work/rustup` and `.darmok-work/native-values/target` as
the recorded RUSTUP_HOME/CARGO_TARGET_DIR. Primary facts at
`logs/native-registry-profile-primary-v1/facts.json` have SHA256
`7c30c41a8aa9681eae2117deac6886f0ddbcd9d265b49d3e2959c2e42728c144`.
Selected facts/seal under `logs/native-registry-profile-functions-v1` have
SHA256 `a1c0ad77c5ce57d567b3a6859b9ffd09a2c66df4ebfe5c7b882efa0b7ec527df`
and `da2ea5a827b21679f020bc286d64e635a604078c9450f4a46952e5c6ccb23570`.
Independent review and current repository/diff checks are required before merge.
No C/Rust/SQL, native build/runtime or profile lifecycle change is introduced.
Concurrent native2PC remains required. Entry/bootstrap/provider/registry/reference/
writer/sequence, full table execution, serving, performance and release gates,
issues46/15 and the goal stay OPEN. Security, compiler PR4, hosted CI, new
stress/recovery experiments and release publication remain excluded.

### Native registry findings merged; archive-bound entry census traced

PR #68 was normally squash-merged from independently accepted
`8a32da42a24e050e72d191dfcaf3a56627860e56` to public main
`470e49d32371733e948857a60f07de2bf7eaa18b`, tree
`d5d2681c34d86430a69a28dafda3076f070c7d6a`, parent840dd53. Its four-doc
checkpoint preserves356 other base entries and all22 native inputs. Independent
review has no required findings and retains349 absolute fixed members. Actual
final19/post20/completion21 exit0; a separate pre-child tool serialization
failure retains a null actual child exit. Root08 rehashes371 fixed/completion
paths and actually exits0; facts SHA256 at
`logs/review-native-registry-profile-8a32da4-v1-root-rehash-v1/facts.json` is
`7ca3b00830dea27776a62d9b84e0d66b4adb6a4ba34ab2a0bfcaa04ed4b76f6d`.
Root14 verifies exact merged tree/parent, public personal repository and generic
description, no eligible open PRs after filtering PR4 and issues46/15 OPEN.
Readback facts at `logs/native-registry-profile-8a32da4-merged-readback-v1/facts.json`
have SHA256 `d5c3e3e12ae7ded6fa378d9dd3ea6fa21c49ddd03564d6228ce45a4069cec315`.
The accepted investigation closes no native registry or descriptor-entry gate.

The next isolated branch starts from that exact main. Its
[entry mutation census](native-entry-census.md) binds official PostgreSQL
17.11/18.6 archives, tag observations before/after and twelve selected earlier
source identities. Root03 actually exits0 after six HTTP-200 responses. The
archives contain7,085/7,284 regular files, including2,387/2,463 C/header files
under src/contrib. The tag commits are083ac033419f690758508e08c1736089384bbee8
and724edf9bde9d356724ad384a2e196edc3c9f80f7. Archive identity does not establish
compiled runtime identity or full program semantics.

Root04's actual1 remains preserved: ripgrep's directory traversal order differed
from a flat lexicographic list. The exact paths matched with no missing, extra
or duplicate item. Separate corrected root05 rehashes each C/header file before
and after five explicit literal queries on each major and actually exits0.
Counts include definitions, declarations and comments; they are not call counts
or live reference events. Generated/non-C sources, indirect/bulk mutations,
external providers and the actual compiled build need separate proof.

Root06 actually exits0 and selects44 complete function bodies across22 function
pairs, one PG18-only rebuild body and six initial callback-list declarations. Four of25
pairs differ;21 have equal selected bytes. The targeted findings distinguish
constructor pins, returned owned references, scan/directory references,
whole-object count-preserving rebuilds and separate callback registrations.
PostgreSQL18's clear/rebuild split and heap-scan bitmap/batching/buffer changes
are retained. No full constructor, provider, startup or owner census is admitted.

Commands are `python3 capture-native-entry-census-primary-v1.py`,
`python3 capture-native-entry-census-symbols-v2.py` and
`python3 capture-native-entry-census-functions-v1.py` through the fixed root
recorder. RUSTUP_HOME/CARGO_TARGET_DIR remain `.darmok-work/rustup` and
`.darmok-work/native-values/target`. Primary facts/seal at
`logs/native-entry-census-primary-v1` have SHA256
`a76e2da7b03f8fd90d0ebda6435abcffadbad16d0dda81e08df526f2d5342edd`
and `a6cd6136b0050491e5430a579ceedac0f4b13321e2d04e992e01b90bb302960e`.
Symbol facts/seal under `logs/native-entry-census-symbols-v2` have SHA256
`f9ae56f41c702f4d267c6c7c9f73cca0ce2c36d92a05f6ba5b30cc2beb47b09a`
and `d1f3f070a89c9508cf0646a954b3b02eddb583f4778a7fc4fb855f6508a35dac`.
Selected facts/seal under `logs/native-entry-census-functions-v1` have SHA256
`c8be611cb6ba4ae3aa6b326200c4f199d69eb5a43e68e4b39c00ed55ca7edcf9`
and `5f9f4dc02cf03e9b40c53852e053e83af9de4ed4167f2567ad26a1fde1cc779a`.
Current repository/diff checks and independent review are required before merge.
All22 native build inputs remain unchanged; no C/Rust/SQL, native build/runtime
or profile lifecycle action is added. Concurrent native2PC remains required.
Startup/entry/bootstrap/provider/registry/reference/writer/sequence, full table
execution, serving, performance and release gates, issues46/15 and the goal stay
OPEN. Security, compiler PR4, hosted CI, new stress/recovery experiments and
release publication remain excluded.


### Entry census merged; native release and compiled builtin files observed

PR #69 was normally squash-merged from independently accepted
`8b33144d611dd0b1009f0971e5b8a4c499072dc0` to public main
`3a748b06dcb3f7a0e360a9a194ac102dc472014e`, tree
`3832badd270c843ad828f53161030c54aaac2a3b`, parent470e49d. Four docs change,
357 other base entries and all22 native inputs remain unchanged. Independent
review reproduces both archives, all ten literal queries and51 selected records
with no required findings. Final18/post19/completion20 actually exit0, retaining
418 fixed members and16 completion companions. Root10 rehashes440 paths and
actually exits0; facts at
`logs/review-native-entry-census-8b33144-v1-root-rehash-v1/facts.json` have SHA256
`46a533e0245cca909104a8535035de422eadfd3e6a89a0af32fca250c58a4f4f`.
Root16 verifies the exact merged tree/parent, public personal repository and
generic description, no eligible open PR after filtering PR4 and issues46/15
OPEN. Merged facts at `logs/native-entry-census-8b33144-merged-readback-v1/facts.json`
have SHA256 `f01e08a16dabdac5d2589e6d1ce9cb77a7b2315b7e4a4c3fe472910afdb00453`.
This accepted census closes no entry or implementation gate.

The next isolated branch starts from that main. Its
[release/header/executable observations](native-build-observations.md) bind the
existing two primary test profiles' identities/start times/restart counts,
PG_VERSION/PG_SHA256, version, configure declaration and selected public-file
hashes. Root03 actually exits0: four HTTP-200 responses bind the official release
archives/checksums;27 child readers are0. The release C/header corpora exactly
match the earlier Git inventories at2,387/2,463 files and bytes; no extra generated
C/header coverage is inferred. All five queries per major reproduce the earlier
literal counts. Declared archive hashes are not complete executable provenance.

Root04 actually exits1 because the uniform header set assumed pg_config_ext.h
on18. Its native include Makefile omits that header;17 installs it. Corrected
root05 exits0 with19 child readers0,11 installed header bodies and two native
Makefiles. Selected headers declare170011/180006, contain the commented
`#undef USE_ASSERT_CHECKING` and bind heap-handler OID3/declaration. Matching headers cannot prove all
executable inputs or establish a production owner census.

Root06 creates a temporary ELF-reader environment and exits0. Root07's unsupported
installer --report option retains actual2. Separate corrected root08 exits0,
captures two HTTP-200 PyPI artifacts and installs the exact pyelftools0.32 wheel
locally with actual child0. The failed environment is preserved untouched.
Root09's actual1 preserves the initial incorrect zero-arity assertion. Corrected
root10 exits0 with eight child readers0 and passive captures of both executables
and public fmgrtab headers. OID3 maps to heap_tableam_handler with declared one
internal argument, strict true and retset false; the traced native call's arity
is separately zero. Two RELATIVE pointer relocations per file resolve to the
selected exported link-image function, not a runtime provider certificate.
Builtin counts3023/3102 admit no other builtin rows. No provider is invoked.

Commands use capture-native-build-corpus-v1.py, capture-native-build-headers-v2.py,
prepare-native-elf-tool-v2.py and capture-native-builtin-dispatch-v2.py through the
fixed root recorder. RUSTUP_HOME/CARGO_TARGET_DIR remain `.darmok-work/rustup` and
`.darmok-work/native-values/target`. Primary facts/seal at
`logs/native-build-corpus-primary-v1` have SHA256
`26c7769e7c9cf1b0df45619f68de2a7610e371fa1935a52e83ff1e0bd3569416`
and `8d8336ee283bfc9e98b57d97b06bb1b564c32a2445d0f0e51192cceff36afac0`.
Header v2 facts/seal have SHA256
`ca29b22c55246addc94c5f606783467f126804891da09bef7c03f79553d98474`
and `d5b2f97df0adb22923a05da23818b7e93e2b6fc4415e4363c1a719d49ad432b6`.
Builtin v2 facts/seal have SHA256
`34d2f712c45128d45f2787bcae8fbc3dfcde77fc9ff992a7aa5dea6c66a6042c`
and `69609503dd8916ca51c70791e8a7c7c005ee834fc080b152733c2b2477c12823`.
Current repository/diff checks and independent review remain required before
merge. All22 native inputs remain unchanged. No C/Rust/SQL/native build, new
server/profile lifecycle, provider invocation or live-memory inspection is added.
Concurrent native2PC remains required. Complete compiled-path/build/entry/
bootstrap/provider/registry/reference/writer/sequence/table execution, serving,
performance and release gates, issues46/15 and the goal remain OPEN. Security,
compiler PR4, hosted CI, new stress/recovery experiments and release publication
remain excluded.


## Private native module observation component

The follow-up to merged PR70 (main8d3ce36fc483520ce1765d2b2d9151deb938a2c0)
adds an owned pathname-image capture beneath the common unused-invocation
boundary. Public PG17/18 estimators run before and after allocation; the final
estimate and serializer are adjacent without allocation, loader or callback
work. A bounded complete native image preserves raw bytes and order in a child
AllocSet and supplies explicit lifetime and requested-byte observations. This
implements observation instrumentation; it admits no new catalog descriptor
and does not relax the collector's pre-open proof. See
[native module footprint](native-module-footprint.md).

Current paired product/probe builds, the two ordinary module-image fixtures,
required existing native suites, repository checks and independent implementation
review are required before merge. Source-bound commands, actual outcomes,
package/provisioning identities and immutable evidence live outside the
distribution. Fresh required verification servers may be provisioned from the
new packages; existing profiles are preserved without stop, restart or signal.
The fixtures introduce no stress, forced error, interruption, recovery or
profile lifecycle experiment. No earlier native package result certifies the
changed inputs.

Exact new required command on both primary profiles:
`cargo test -p darmok-postgres-tests --test server_module_footprint --locked -- --nocapture`.
The mandatory PostgreSQL job selects the whole package, so its test manifest
includes this target without a new hosted-CI action. The probe projects a copied
image and within-invocation owner/snapshot observations; none is an all-reference
owner census or an initializer/callback history.

Startup/entry/provider/registry/reference, writer/sequence, new descriptor/table
execution, serving, performance, release, issues46/15 and the goal remain OPEN.
Concurrent native PostgreSQL two-phase transactions remain required.
Security/authentication/TLS/roles/grants, project compiler work, new native
stress/recovery experiments, hosted-CI polling or retry, account actions and
release publication remain excluded.


The initial module-observer revision3aa2aaffe988c1e890ee73db1fa16c7ca44a444a
passes four product/probe builds and42 package checks, with24 native inputs.
Two fresh primary profiles bind the packages with55 successful provisioning
commands. The PG17 module fixture actually exits101: both ordinary cases fail
during the first runtime symbol lookup, before the observer body executes.
PGXS uses `-fvisibility=hidden`; the new header omitted the explicit PGDLLEXPORT
declarations used by the existing private C APIs. Compilation alone did not
verify the exported boundary.

The correction exports only the five private observer functions through their
header. It preserves default hidden visibility and tests the actual product
API through the separate probe. Old packages, profiles, helper/command streams
and the real failed fixture remain preserved. Corrected current paired builds,
fresh profiles, actual ordinary fixtures, required existing suites and independent
review remain required before merge; no original-body runtime result is inferred
from the failed lookup or earlier package success. All broader gates stay OPEN.

## Private live builtin dispatch component

Starting from merged PR71 main b36993884830f85adee325b9936203be7fbe3185,
this component adds a private caller-owned C observation of the running backend's
heap/btree builtin dispatch. It reads only the public fmgr table and mapping,
checks selected OIDs/signatures/flags/names and linked function-pointer equality,
and copies inline scalar/name records after both checks succeed. It invokes no
handler, opens no catalog, selects no data snapshot and performs no heap
allocation. This addresses the live-table-versus-linked-symbol relation; it is
not original-provider identity, complete compiled derivation or descriptor
admission. The contract is [native builtin dispatch](native-builtin-dispatch.md).

Public mapping bounds and the missing sentinel are checked before each selected
row. Both generated handler OIDs are compile-time checked. An unexpected row
fails explicitly, with no alternate lookup or handler. The observer uses the
existing unused-invocation boundary and requires normal processing. No native
pointer escapes. Table/symbol definitions remain bound-native-build inputs;
callback/registry/reference history and actual FmgrInfo/AM routine/pg_am provider
selection remain separate obligations.

Two ordinary probe fixtures are registered in the existing PostgreSQL package.
They require exact TEXT/SHOW/transaction wire outcomes, the independently
observed 17.11/18.6 builtin counts, exact selected rows and linked-symbol checks,
and snapshot/within-invocation owner preservation before, within and after a
savepoint. No owner identity equality across different subtransactions is
claimed. They introduce no new forced-error, stress, interruption, recovery or
profile lifecycle experiment. Current strict product/probe builds, required
existing suites, affected Clippy/format/boundary checks and independent
implementation review are mandatory before merge; their actual receipts remain
external evidence. The product/probe build-context input count grows from24 to26.

    cargo test -p darmok-postgres-tests --test server_builtin_dispatch --locked -- --nocapture

The mechanism uses two indexed reads, bounded name comparisons and a fixed
stack copy. Caller cadence and measured latency/memory/throughput/contention
remain unverified. Full entry/startup/provider/registry/reference/writer/sequence,
new descriptor/table execution, serving, performance, release, issues46/15 and
the goal stay open. Ordinary profiles keep max_prepared_transactions=10;
concurrent native PostgreSQL two-phase transactions remain required. Security,
project compiler PR4, hosted CI and release publication remain excluded.

Revision76f274f06204b316ce98eb6ae4d603fdc7b17b12 passed four strict native
package builds and both new builtin and previous module fixtures on each
primary profile. PostgreSQL17 passed all51 required existing native fixtures
(55 with the four observer fixtures). PostgreSQL18 passed catalog discovery11
and publication17, then failed the existing concurrent shared-drop admission
fixture while waiting for reader admission. Its remaining three required
test binaries did not run. That revision therefore has32 PostgreSQL18 passes
and one failure, not a completed paired regression result. The actual101
receipt and package/profile observations remain preserved at
`logs/native-builtin-dispatch-76f274f-10-regression18` and its binding facts
`logs/native-builtin-dispatch-76f274f-10-regression18-binding-v1/facts.json`
(SHA256 aefd0d1b2ce3b9cfc8777d2a940dc83f44f3f830216f221727489de4a329ee46).

The fixture assumed that awaiting the first DROP reply kept the second DROP's
intent live. Native backend progress is independent of when the client polls
an already-submitted request: the second busy-database error can finish and
clear its intent before the first reply is consumed. The failed receipt does
not distinguish the fixture's two admission-wait call sites. The source shows
that the second wait lacked a native ordering barrier; increasing the timeout
or retrying would not establish the property being asserted.

The correction adds a test-only object-access delegate and an ordinary
transaction-level advisory holder. In either existing preload order the
delegate waits only after the real shared-drop intent is registered and its
module fences released. This keeps the second intent live until the fixture
observes reader admission after the first commit. Releasing the advisory holder
then permits the same existing native busy-database error and verifies its
advisory-lock cleanup. Phase-specific diagnostics identify future admission
failures. Product publication code and timeout limits are unchanged; no new
forced-error or profile lifecycle scenario is introduced. Changed probe inputs
require fresh current packages/profiles and all required paired suites before
independent review and merge. All broader gates remain open.

## Live builtin dispatch merged; entry mechanism alternatives

PR #72 is squash-merged at main 3ed9cc3308e7808eda36da631224cc61b48c9bde,
parent b36993884830f85adee325b9936203be7fbe3185. Its tree
8bff6cbebfc4e3e91628b56bb538619014d74c8c equals independently accepted
feature c737d50aa4e67fb7c8da7a89c4de6dd0c035bd58. Corrected verification
passed 55 required ordinary native fixtures per major (110 total), zero failed or
ignored, and the two changed Rust targets' Clippy, formatting, nine-package
repository boundary and diff checks. Four product/probe images bind 26 native
inputs with 62 successful package checks. Product layers reuse unchanged
accepted bytes; current probe compiler output supplies strict PGXS flag evidence.
Eight fresh profiles bind 95+278 successful provisioning actions and 76 successful
before/after artifact/identity readers. Ordinary/ordered profiles keep native
two-phase support 10; only the required negative profile uses 0.

Independent implementation review has no required findings. The fixed report
SHA256 is 79c3043154dcd31532da2506174a4d1299f147a71609c6aa1959da8d72d2b890
at `logs/review-native-builtin-dispatch-c737d50-v1/report.md`. Its final/post/
completion receipts each actually exit 0. Root 23 independently rehashes 2101
fixed members plus 16 completion companions; its facts SHA256 is
ca3f355944cd18235c8c219d1679fd7782db058d066b0462c9acf7a59d29987b.
Previous native 101 and reviewer audit-assumption failures remain preserved.
Neither the source-proved alternate hook chain nor a passing primary fixture
is relabeled as execution of the corrected two-DROP fixture on the ordered
profile. Merged readback verifies the exact tree, single parent, public personal
repository, unchanged description, no eligible open PR after filtering PR 4 and
issues 46/15 OPEN. Its facts SHA256 is
b5e689803b6a0c1571c345f9858788ac2db4b65208f41c2bedea71b2a704c8ae
at `logs/native-builtin-dispatch-c737d50-merged-readback-v1/facts.json`.

The next isolated branch begins at that exact main and records
[entry mechanism alternatives](native-entry-boundary.md). Current observers do
not establish the private cache/owner/registry state needed before descriptor
entry. The comparison specifies the construction obligations for stock
PostgreSQL and the proposed responsibilities of a native inspection/provenance
surface in a supplied server build. It analyzes the different installation,
maintenance and runtime costs without adopting a new server footprint or
claiming either mechanism is implemented. The footprint preference has been
raised with the user; stock PostgreSQL plus the extension remains the working
direction meanwhile. Concurrent native two-phase transactions remain required.

Root 01 fetch and 02 worktree creation actually exit 0; the accepted component
worktree stays frozen. Root 03 captures twelve archive-bound inputs, six full
public headers and fourteen selected private declaration matches with actual 0
at main 3ed9cc3. Its 29-member primary facts/seal SHA256 values are
2105b34133eb6d3ae7a558fe684dce0d8ecdb2b2ada6863bf28c97eb2daf0e03 and
abbc1ec19ac304fa8ea2e12d992f97c75b6719b044b7bbf5d7b0b0505180d6ea.
This source comparison performs no native operation and does not assert an
impossibility theorem about every native API. Current native input bytes remain
unchanged; no fresh runtime certification follows from these documentation
changes. All full engine, serving, performance and release gates, issues 46/15
and the goal remain OPEN. Standing exclusions remain in force.

## Stock PostgreSQL services selected as the native trust boundary

PR #73 is squash-merged at main 959101c43ba746538fd0252f59c84e4e4b05745b,
parent 3ed9cc3308e7808eda36da631224cc61b48c9bde. Its tree
01f7dbe6966c752d58f62b581d2d856cb7fa7438 equals accepted feature
03a7cc038cbe02f2f7a62ce682c41c61c6f9cfb1. Independent documentation/source
review found no required correction. Root 07 actually exits 0 while rehashing
280 fixed members and 16 separate completion companions; its facts SHA256 is
9ef1f3f5b9fb3f2a40938c28d9eae6c86743866c01d5e77a80499c29d06366f9.
The review report SHA256 is
8811e82c975dc712b890b1c89cac9a25ca579c8aa0733651e90e74b8df4654a9;
its final/post/completion records each actually exit 0. Its own10 bookkeeping
exit 1 remains preserved. Root 08–13 preflight, feature push, PR creation, open
readback, normal exact-head squash merge and merged readback all actually exit 0.
Merged readback verifies that exact tree/single parent, public personal repo,
unchanged description, no eligible open PR after filtering PR 4 and issues
46/15 OPEN. Its facts at
logs/native-entry-boundary-03a7cc0-merged-readback-v1/facts.json have SHA256
48e239183c3cdb19e4c88f63687a4e3c1db050fdb221f3fb9918394606a6fa2c.
This was a documentation checkpoint, not new native/runtime certification.

The isolated next branch starts at that exact main. A read-only independent
architecture assessment recommends Alternative A under an explicit supported
stock-native API contract. The earlier exhaustive native cache/owner/registry,
compiled-implementation and init-file-producer premises required unavailable
integrity observations beyond the functional boundary a proxy needs. The
[selected runtime contract](native-runtime-contract.md) now relies on correctly
operating stock PostgreSQL services and an admitted extension configuration,
while retaining every project acquisition/lifetime, neutral-preparation,
pre-open-path, physical-wait, DDL/2PC, defining-carrier and execution-stability
obligation. This is a deliberate specification change with rationale; those
integrity premises are withdrawn, not reported as successfully proved.

Both MySQL applications on initialized databases and MySQL clients on existing
native schemas remain required. No supplied server build, new data directory,
init-file deletion or max_prepared_transactions=0 requirement is adopted.
A relevant legitimate callback or incompatible registry definition still needs
functional admission before the dependent path; trusting PostgreSQL does not
make eventual generation equality an excuse for earlier evaluation. Raw catalog
images remain the defining input, not cached parsed options or compiled facts.
The current collector's NULL-options checks and all native input bytes stay
unchanged. New descriptor paths, writer coverage, exact guards and statement
validity still need implementation proof and current ordinary verification.

Root 01 fetch and 02 worktree creation actually exit 0. Root 03 records two
previously archive-bound relcache inputs, eight complete selected init-file
producer/consumer/pre/post-invalidation bodies and four paired differences at
main 959101c, with actual 0. Primary facts and 17-member nonself seal SHA256 are
44c3460fb4d99b7ada966cc000dc5b64b9e62aa0cb421fe7111cfe1068fa768c and
1a278df35c02abc59bb91d16a5d20c74e3d7bc9a40951b52d496107376782b5f
under logs/native-runtime-contract-primary-v1. These explain the selected native
service assumption; they are not complete-file/call-graph or runtime acceptance.
Earlier source observations and failed extraction/bookkeeping/native outcomes
retain their original scopes. Four documentation edit-context/syntax failures
are preserved in the tool trace and corrected before review; they are not native
product failures. No profile operation, native build, new stress/forced-error/
interruption/recovery experiment, security or compiler work, hosted CI or release
publication is introduced. Full engine, serving, performance and release gates,
issues 46/15 and the overall goal remain OPEN.


### Catalog reader reference scopes: implementation draft after PR74

PR74 is squash-merged at public main
fb4db00d94177db30237ca12d915cca584ca75da, with exact reviewed tree
d3b88f9c2fba5f98ce6a5ce9338cd551b688700f and parent
959101c43ba746538fd0252f59c84e4e4b05745b. Current merged readback is
logs/native-runtime-contract-adadad1-merged-readback-v1/facts.json,
SHA256 ea23bcb1d2095f00bd1254ff994d573cbde93642e5d4faf29af63bab95191bf7.
Its independent architecture/documentation review found no required corrections;
root07 rehashed all280 fixed members and16 companions with final/post/completion
actual0. This records stock native services as trust dependencies while retaining
Darmok's own functional requirements. It accepts no new collector runtime.

The next implementation draft on feat/native-bootstrap-references addresses
the initial preparation's reader increments crossing later explicit physical
acquisitions. Both existing readers now use the same transient seed mechanism:
six native AS references for four-heap SHOW, eight for six-heap storage.
Successful seed acquisition precedes NoLock reader opens. Storage closes its
initial readers/snapshot and releases the entire seed before full graph
acquisition; source/final opens use that graph's exact AS. SHOW scopes its initial
lifecycle wait before a seed, uses the no-CV second acquisition and closes the
whole attempt before another initial wait. It requires an unused physical and
semantic invocation boundary.

Returned resource pointers detach before cleanup, preserving native abort for
uncertain operations instead of attempting a second decrement. Invocation memory
is a child of the native current transaction context: normal completion deletes
it; failure preserves native resource backing storage until native abort/cleanup.
Installation verification releases its syscache tuple before the separate
namespace lookup. Three existing statement-guard fixtures now perform nonempty
catalog discovery before Share and retain their real snapshot, publisher,
prepared-completion and shared-drop assertions. No new forced-error fixture is
added.

Root03 at clean base fb4db00 actually exits0. The source-only primary at
logs/native-bootstrap-references-primary-v1 binds ten selected cached source
files directly to two already captured official archives,24 complete selected
functions and12 paired differences (nine equal). Facts SHA256 is
b926ccae45afa73284b6594ab7fb86bd0cbc8bddd63750eb93c7651c8affb544;
the51-member nonself seal is
a4293ce10797821cee887ed5864232a028ddc67ad8dd82f7ea35432a381d62f9.
This is a limited API/reference/snapshot argument, not whole-file/call-graph
semantic acceptance, a build or runtime result. Two broad passive search
displays were truncated; no complete semantic reading is inferred from them.

The preserved package-helper v1 draft was not invoked; it retained the older
image-name prefix. The separate frozen v2 helper uses the new component's image
names and a fresh packages-v2 output namespace. No failed native build or runtime
result is claimed for that uninvoked draft.

This checkpoint is planned/source and implementation draft only. Fresh
PostgreSQL17.11/18.6 product/probe packages, affected existing required ordinary
suites, current finite evidence and independent fresh review remain pending.
The current NULL-options restrictions are unchanged. Supported options/path
admission, complete transitive definitions, writer/guard coverage, immutable
binding/planning, table execution, serving, performance and release, issues46/15
and the overall goal remain OPEN. No existing native profile was changed or
stopped; no new native stress, corruption, forced error, interruption, recovery,
security/compiler/hosted-CI/account/support or publication work was performed.


Root04 is preserved as a pre-command recorder rejection, actual1: its source
readers successfully observed the intentionally dirty implementation draft at
base fb4db00, and the frozen recorder correctly required a clean source. The
requested git diff check did not execute. This is a root orchestration ordering
error, not a native product, build or runtime failure. The next diff check uses
a fresh namespace after committing the draft; this failed namespace is not
reused or relabelled.

Root05 diff and root06 repository boundaries at committed draft
33f1d3e6d8ba06758af7aaccce4a6cbc37abd061 actually exit0. Root07's first
PostgreSQL17 product build actually exits1, with nested make exit2, under
logs/native-bootstrap-references-33f1d3e-packages-v2; the outer immutable
receipt is logs/native-bootstrap-references-33f1d3e-07-packages. Strict compilation
caught a local variable named text shadowing PostgreSQL's text typedef inside
TextDatumGetCString's macro expansion. Renaming the local to version_text fixes
the C name collision without changing catalog behavior. The failed invocation
is preserved; no PostgreSQL18 build or runtime profile was reached, and no
package acceptance is inferred. The corrected clean revision uses fresh output
and image namespaces. A passive receipt display also used an absent stdout.txt
pathname and exited2; its preceding receipt read succeeded, and it is not a
native product result.

Root08 diff and root09 boundaries at clean 315c0614283c2422ecf77cd7a44da30629993c10
actually exit0. Root10 is preserved with actual130: all four PostgreSQL17/18
product/probe builds exit0, but the superseded artifact check was cancelled at
18-product-heap_storage.h after source review identified the cleanup disposition
case below. Its original raw streams and partial receipts remain under
logs/native-bootstrap-references-315c061-packages-v2 and the root10 folder.
No package facts, runtime acceptance or started database profile is inferred.
Only the owned package client's artifact inspection was signalled; no existing
PostgreSQL verification profile was operated on. This is cancellation of a
superseded package check, not a native error/interruption/recovery experiment.
An initial process matcher printed an incorrect completion inference after a
narrow prefix failed to match the actual interpreter name; the immediate recorded
wait showed the command still live. Subsequent exact owned-process matching and
the terminal root10 receipt determine the disposition. Source stayed clean at
315c061 until that recorded command ended.

Fresh source review found that the result-complete flag alone could delete the
invocation context and omit abort-required if outer cleanup itself raised ERROR.
Both readers now track successful resource cleanup independently with a volatile
flag. Deleting invocation memory requires both result and cleanup completion;
either failure preserves the native transaction-owned backing storage and records
the captured subtransaction as abort-required. This changes no physical graph,
lease, retry limit or successful protocol behavior. No forced cleanup-error
fixture is introduced; current ordinary suites and independent source review
must assess this disposition.

The separately frozen package v3 helper batches the same artifact reads in one
inspection container per image, reducing 52 short artifact containers to four.
All native/public headers, README, server/probe artifacts, versions, absent-probe
checks and product/test byte equality remain checked. Its fresh packages-v3
namespace has no prior result. The startup/fixture v2 helpers consume that schema
with fresh names; their separately preserved v1 drafts were never invoked.
Two earlier passive inventory searches used absent guessed backend/packages
paths and exited2; successful preceding reads keep their narrow scope. Those
tool errors do not describe native product behavior.

At clean 7abbcfd9fd385fc97c9dc58ab6859aa65835f647, root11 diff, root12 repository
boundaries, root13 targeted rustfmt and root14 targeted Clippy actually exit0.
Root15 builds and binds four PostgreSQL17.11/18.6 product/probe images with all14
recorded commands exit0 and26 native inputs. Package facts under
logs/native-bootstrap-references-7abbcfd-packages-v3 have SHA256
77ce9d17ab7e79af6947bfac4e2387bd70d4948e2a68fad9aa590330ef48d84d.
Packaged native bytes remain unchanged after this build.

Root16 setup actually exits1 after four fully completed PostgreSQL17 entries.
The first PostgreSQL18 fresh cluster took about59seconds through ordinary initdb
and the entrypoint's checkpoint/startup, exceeding the helper's40-attempt window.
Passive logs/state showed its later healthy, running state, no OOM and no restart;
that failed profile remains unaccepted and unchanged. Root17 validates only the
four completed17 entries and203 successful command receipts into a fresh aggregate
scope, without changing or relabelling the failed setup. Its facts SHA256 is
7c97aba775eda84b40fc59139276a446283e68ad5756b90bbc19cd4d59c4ea47.
Root18 uses the separately frozen startup v3 helper with a monotonic180-second
deadline and matching health-start period on four newly named18 profiles. All226
commands exit0; facts SHA256 is
71b310554d95d0c242bdd1acf80584446877750c0d598fa130a069910492387e.
No existing profile was stopped, restarted or signalled; native durability and
two-phase settings were not weakened. Each primary/ordered profile uses native10,
and only the separate negative configuration uses0.

Root19 PostgreSQL17 integration and root20 private-owner cases actually exit0:
55 cases across seven binaries plus three private-owner cases, zero failures or
ignored cases. Their30 before/after native artifact readers each exit0. Binding
facts SHA256 are08533df4f6f93bd86ed0542ec427f326a31c25529fae1b8373b58f32db9e1637
and f5c9dd5d119c5907948cce7553b404c17ec594260bb8223cd4ce10fd900c1bb9.
Root21 PostgreSQL18 integration actually exits101: catalog discovery11 and builtin
dispatch2 pass; publication17 pass and one concurrent shared-drop observation
times out. The other four binaries and18 private-owner cases were not run.
Failed binding facts SHA256 is
f0ce617a5bfd1e6f5702ecf3c2ff85dc137070c2008ae7ff2ee76838711f6870.
No successful whole18 suite or current mechanism acceptance is inferred.

Root22 passively captures the failed18 profile's logs with actual0. Its native
second DROP reaches the expected busy-database error at06:03:29.172UTC, after an
ordinary14.140-second checkpoint. Together with the fixture's ordered awaits,
this identifies the late observation, not its initial stamp read. The helper
started an unconditional20-second observation timer before deliberately keeping
that observation blocked across separately bounded release/drop stages. The
timer could expire while the test intentionally prevented completion. The fix
separates event parsing from timing: immediate observations retain their20-second
deadline; all three existing intentionally staged observations use the receive
operation and retain their caller's bounded completion deadline after release.
No assertion or native expected outcome is removed, and no duration is simply
increased. The failed native cohort is preserved; fresh required verification and
independent review remain pending. A passive search used the helper-root cwd for
the fixture source and exited2 after displaying the matching log lines; the next
source read used the correct worktree. Earlier outcomes retain their scope.

At 69a408ef17fffc2a13a33c76c9d5aac250d8d471, root23 diff, root24 targeted rustfmt
and root25 targeted Clippy actually exit0. Root26 creates eight freshly named
verification profiles, all451 commands exit0, binding the unchanged26 native
inputs to the accepted7abbcfd package build. Its facts SHA256 is
f284792825b9421c6e18f27bfaef3515ec7b172216fe1a2b49efddad0b06b519.
The fixture v3 wrapper explicitly records separate source and native build heads
and validates the actual current native inputs before and after runtime checks.

Root27 PostgreSQL17 integration actually exits101: discovery11 and builtin2
pass; publication17 pass and the concurrent shared-drop fixture fails at its
first DROP completion deadline, line1855. This is a distinct, located outcome;
the observation's unconditional timer has already been removed. The remaining
four binaries, private-owner cases and all18 runtime cases were not started.
Its failed binding facts SHA256 is
7c2e9e4601b3ad4c803fc7abcf7aec9e47a979bfacc56473f0b1d3dbf817f46f.
All30 before/after artifact readers exit0; no whole current runtime acceptance
is inferred. This failed cohort remains preserved.

Root28 passively captures that profile's native logs with actual0. The first
DROP's ordinary checkpoint begins06:24:23.728UTC and completes06:24:44.552UTC:
total20.825seconds, including19.726seconds of filesystem sync. The test's client
EOF occurs at06:24:43.639UTC after its20-second deadline; the native checkpoint
then completes normally. The native durability operation is not a metadata
deadlock. The fixture's20-second durable utility budget was an unstated disk
latency requirement, not a proxy semantic assertion. A named120-second database
DROP completion budget now applies to the three existing DROP completion waits;
lock/admission, immediate observations and released reader completion retain
their existing20-second deadlines. Native fsync/checkpoint behavior, assertions,
outcomes and the extension's bytes remain unchanged. This bounded functional
verification policy is separate from the uncompleted performance gate. Fresh
required verification and independent review remain pending; no new native fault,
stress, interruption/recovery or profile lifecycle experiment is introduced.

At 6db9794ef20dceec0ccc229b4b9a9998a777cbeb, root29 diff, root30 targeted
rustfmt and root31 targeted Clippy actually exit0. Root32 creates eight fresh
PostgreSQL17.11/18.6 verification profiles with all441 commands exit0, preserving
the26 accepted7abbcfd native inputs. Its facts SHA256 is
070efb6e847f20fdc187646db5710ad2dbac67d1bb28d4fdaefacf37c2a843e2.
All primary/ordered profiles retain native max10, separate no2pc profiles use0,
and restart counts remain0. Root33 PostgreSQL17 integration and root34 private
owner cases actually exit0:55 plus3 passed, zero failed or ignored.

Root35 PostgreSQL18 integration actually exits101: discovery11 and builtin2
pass; publication17 pass and one concurrent shared-drop assertion fails at
line1883, because the second writer's advisory reference is still visible.
The remaining24 integration cases and3 private-owner cases were not started.
Failed binding facts SHA256 is
09776adad0697c7d95db64a6bb3cf9c9620ae93f7dc2434b738fc28a00261ff7.
All30 before/after native readers exit0 and their native facts are unchanged.
This is an assertion failure after the expected busy-database error, distinct
from the earlier timing failures; no whole PostgreSQL18 acceptance is inferred.

Root37 captures this failed profile's native logs passively with actual0; the
stderr SHA256 is
b096d6bd3e239185047092952cde7ecebe09d317a6b84f64585730dfc9ff8f9e.
The checkpoint completes normally and the second DROP reports its expected
busy-database error at06:51:18.000UTC. Selected paired PostgresMain source shows
EmitErrorReport before AbortCurrentTransaction; the connector's batch_execute
returns on the ErrorResponse without confirming ReadyForQuery. The probe's
advisory reference is transaction-owned. Checking its release on receipt of the
error therefore races native abort cleanup. The three existing staged DROP
futures now use the existing command-event interface, preserve the exact DROP
tag or one backend error, and require ReadyForQuery Idle before returning either
outcome. The OBJECT_IN_USE and zero-advisory assertions remain. No extra query,
sleep, weakened assertion or new timeout is introduced; native package bytes
remain unchanged. Fresh required verification and independent review are pending.

Outside-repository evidence binder drafts v1/v2 were created exclusively and
read but never invoked. V2 distinguishes each of the eight exact per-profile
fact records from successful command receipts; neither draft supplies a runtime
result or an acceptance seal. Two passive searches in this turn guessed absent
native source/probe paths and exited2; later file inventories identified the
actual paths. These preserved tool errors do not describe native product failure.

At 62deefe017e947bfb2dba454ea36fb9ea5f2f662, root38 diff, root39 targeted rustfmt,
root40 targeted Clippy and root41 repository boundaries actually exit0. Root42
creates eight fresh17.11/18.6 profiles with all529 commands exit0; facts SHA256 is
783aa535ae4463ac3bc3d20f2042f5bbdf91948e477ec1a91b76b6569f4e70ad.
The26 native inputs still exactly match the accepted7abbcfd package build.
No failed or old native profile is reused as this verification cohort, and no
existing profile is stopped, restarted or signalled.

Root43/44 PostgreSQL17 integration/private and root45/46 PostgreSQL18
integration/private all actually exit0. The full ordinary matrix is55 plus3
per major:116 passed, zero failed or ignored. All four wrappers bind thirty
successful before/after native artifact/identity readers and the unchanged26
native inputs to the actual tested source and separate native build head.
Their binding facts SHA256 values are, respectively:
cf13267032f10aa824bb2933492206527595ab3fd50a37db494ef7d17002202a,
c6cdf1ffb29aacbf22d317a4b4ef4d0df123e0b3344c5d779ab2097129ab4e08,
6eaee20ce29ee7a7dd8b9772787a97b9e3cc5b7785e97c362433a00b1b24f7d4,
and78ba4dd5f4e5f1a2875e7f19d524805add34d65e7efc84f4bf4c4d083b156333.
Both publication suites pass all18 cases, retaining every busy-drop outcome,
admission and zero-advisory assertion after actual ReadyForQuery completion.
Primary/ordered native max10 and concurrent prepared transaction requirements
remain unchanged; zero is still only the negative configuration.

Integration command: cargo test --locked --offline -p darmok-postgres-tests
--test server_catalog_publication --test server_statement_guards
--test server_relation_guards --test server_heap_storage
--test server_module_footprint --test server_builtin_dispatch
--test catalog_discovery -- --include-ignored --test-threads=1.
Private command: cargo test --locked --offline -p darmok-execute --lib
native_backend::tests::native_catalog:: -- --ignored --test-threads=1.
Each runs through the frozen fixture v3 wrapper under
logs/native-bootstrap-references-62deefe-{43,44,45,46}-pg{17,18}-{integration,private};
the matching -binding-v3 directories retain artifact and configuration binding.
RUSTUP_HOME and CARGO_TARGET_DIR use the previously recorded isolated local
toolchain/target; inherited native URLs are cleared and reconstructed privately
from these fresh profiles. No credentials are recorded.

Root47-drop-primary actually exits0. Its six bounded source spans compare both
cached postgres.c files directly to the official17.11/18.6 archives before/after
selection and bind the two connector early-error functions to tracked bytes.
Facts SHA256 is
2a00eb5b86299ec2577531d0ce265a5a37de91188e49807614c07e63ac24c98d;
its14-member nonself seal is
ca425821499b1c0f32d25f5cdb0d07571d64ba9a1fc4cb85d391488031a9d859.
The complete selected spans were read. They establish the local error/abort/Ready
ordering; they do not certify all of PostgresMain, every cleanup callee, security
configuration or a whole native integrity/callback graph.

Only repository status/evidence documentation changes after this tested revision.
Native/package and the two changed Rust fixture bytes remain exactly those tested.
The packaged README retains its build-time draft label; current verification is
recorded by the repository reference-scope document and this ledger. Binder drafts
v1/v2/v3 remain uninvoked and unchanged; fresh v4 binds the final documentation
revision to the current package/profile/fixture/selected-source evidence without
expanding historical review graphs. Independent review is required before merge.
An initial documentation patch was rejected for an unmatched line before any
mutation; the following patch used the actual line boundary. No execution result
or failed evidence namespace was overwritten or relabelled.
This completes local ordinary reference-scope verification only. New supported
options/descriptor paths, writer coverage, transitive type collection, immutable
binding/planning, table execution, serving, performance and release gates,
issues46/15 and the overall goal remain OPEN. Standing exclusions remain.

### Raw relation option carriers — candidate implementation

Base: d0f39c521dddbfdb6220199c54c4a9132e4e90b7, the exact reviewed PR75
merged tree933fb06a150383c3baf4d4707512db87ebdc4cd7. PR75 independent review
has no required findings; root50 rehash and root59 merged readback actually
exit0. Its116/0/0 results remain evidence for that earlier native source only.
Merged-readback facts are logs/native-bootstrap-references-59127f8-merged-readback-v2/facts.json,
SHA2562b6ba82f60c8f3f596f81b8bde9b2ce529de998642cd6623ac017cd921106e1a,
with37-member seal7dbce45131c1b3fe72f07018d4244cfe6d94a80e71d5561165ffb8f7b7692c45.
Readback v1's uninvoked stdout.log assumption was corrected in distinct v2
before execution; no failed namespace or result was overwritten.

The working branch captures one raw pg_class.reloptions source per distinct
selected graph node through an extra class scan in the SAME raw observation
span and registered snapshot. Pure graph selection/source construction now
also occupy raw; no second fence acquisition or options parser is added.
Only selected carriers are copied. A/B/C compare source identity, independent
NULL presence and exact stored bytes; B supplies normalized owned images.
The existing opened bootstrap descriptors still require NULL options. All
returned scans detach before cleanup after raw/S. See native-relation-options.md
for the exact sequence, cardinality, native resource and cost contracts.

Direct catalog scan count becomes21 per successful attempt, plus selected
metadata TOAST scans. options_rows reports the extra full class pass, while
payload bytes count only selected carriers. Physical modes/counts and protocol
round trips are unchanged. Longer raw exclusion, selected carrier/bitmap/image
allocations and phase budgets need current verification; no performance result
or complete options/descriptor/writer/table gate is claimed.

Two ordinary fixtures cover actual heap/index/parent-TOAST option changes and
reset, duplicate roots, NULL sources, unrelated-source filtering, preserved
historical copies and native borrowed modes. Prepared SUE option changes are
observed before and after both normal outcomes with first-unselected and
established real table reads. The independent varlena oracles add text[]
signatures and exact class source/image/accounting checks. These are required
ordinary feature fixtures, not stress, injection or recovery experiments.

Passive current source commands01–12 are saved under native-options-path-d0f39c5.
Own08 actually exits1 because its rg child exits2 on nonexistent statement_guard.c;
raw/partial streams remain preserved. The corrected09 reads the actual
utility/prepared/reader code in darmok_server.c. A later direct documentation
inventory has Python wrapper0 but rg child2 for nonexistent native-catalog-payloads.md;
the actual native-variable-catalog-payloads.md was then read. Neither error is
product verification. The earlier design notes remain a distinct draft; the
implemented same-span sequence is the current spec.

Own12 primary capture actually exits0. logs/native-options-path-primary-v1
binds six named files to official PG17.11/18.6 archives, four complete selected
writer/temp-predicate bodies and two complete public class declarations. Facts
SHA256d195a6778de42d284c93be0d2d172e9e55804845f1d54df6289b0f7510199c39;
16-member nonself seal4a07aa82dfd362fe024686f30de00c3d51b4a5ce129c6dbdeab575235bd780d7.
Selected bodies/declarations and actual paired differences were read. This
is finite interface evidence, not complete writer/kernel/registry acceptance.

Current strict native packages, affected ordinary PG17/18 suites, strict Clippy,
boundaries and independent implementation review remain PENDING. Both use cases
and concurrent native two-phase transactions remain required. Wider
options/descriptors/writers/closure/binding/planning/execution/serving/performance/
release gates, issues46/15 and the goal remain OPEN. Security work, compilerPR4/
source/resources, hosted CI/admin/account actions, release publication and new
stress/interruption/recovery or existing-profile lifecycle experiments remain
excluded. Existing profiles are preserved.

### Raw relation options: first ordinary run and semantic-wait correction

The first implementation is committed at d6571f495920b36c1e1f730f23fb24c530e45db1,
tree 42880d1e0e60e4f6cdf067fa6689d896807b6412. Recorded source checks 13–16
(diff check, targeted rustfmt, repository boundaries and strict storage-test
Clippy) all exited 0. Paired arm64 Linux PostgreSQL 17.11/18.6 packaging
(root 17) exited 0: four images, 26 source inputs and fourteen successful inner
commands. Package facts SHA256 is
792b80fda1eb4688a45f5d5974aab54915094edc42ae285ebf3f52830aa8e249.
The eight fresh required profiles (root 18) and all 568 setup commands exited
0; profile facts SHA256 is
6f4d29d52a53cc27598279d69b94a03f6539d3d0a748008929321bc8a86648e6.
Normal and ordered profiles have native 2PC enabled with ten slots; zero slots
are only the explicit unsupported configuration.

Root 19, the first PostgreSQL 17 integration command, exited 101. Three
completed bins passed 11/2/18 tests; storage passed 12 and failed the new
prepared-options fixture with a bounded SQL timeout. The remaining three bins
and private/PG18 runs were not executed. All thirty before/after fixture
binding readers exited 0; their identities stayed unchanged. Binding facts
SHA256 is 8f6a9a3b5eee8d8a755eb60c965c5f8f3cb4a61d7c45bf3c57be7d5d4efc1e94.
This run is failed evidence, not acceptance; preserve its eight profile
namespaces, receipts, streams and binaries unchanged.

The new fixture incorrectly assumed physically compatible SUE/AS permits the
full consumer to return while metadata is prepared. The existing
PRE_PREPARE callback deliberately transfers semantic RX, and the final
consumer acquires S. That wait is required even when native relation modes
coexist. The correction verifies this real wait, its lack of raw/publication
locks, and the independent data horizon; a separate connection completes the
prepared transaction before the consumer returns. It keeps the earlier owned
JSON images for copy-independence checks and compares completed current images
for both commit and rollback. Native C, installed header and probe inputs are
unchanged; no barrier is bypassed and no timeout is widened. This preserves
concurrent native 2PC support. Corrected ordinary verification is pending.

Passive failure diagnosis is recorded in the unique root-19 suffixes. The
first source reader exited 1 on an excessive declared range; its corrected
EOF reader/log capture exited 0. The first lock-path reader likewise exited 1
on an excessive range; the corrected EOF/native-writer reader exited 0.
The semantic-writer-paths reader exited 2 because a guessed observer file did
not exist (all selected source spans were emitted); the actual observer is in
the storage test file, read by the corrected observer reader at exit 0. Keep
all failures as failures. These are passive reads of the failed ordinary run
and selected source interfaces, not new stress or forced-error experiments.

External evidence verifier v1 is an uninvoked draft. Its guessed root-08
namespace was corrected in uninvoked v2, which additionally binds producing
stdout hashes, helper hashes and the real source-reader values. Neither
certifies the failed runtime. Corrected evidence needs a new namespace and
must preserve the failed runtime and passive readers. No old kernel, registry
or proof graph is recertified. Broader descriptor, writer, closure, execution,
serving, full performance and release gates and issues 46/15 remain open.

### Raw relation options: prepared semantic wait exposed an ephemeral horizon

Corrected fixture source 1388ddd2e43584c24f95c2129e2fa4ec73e926a1, tree
2a88dc0c82172e3afc017e5bbbaf2bac0e5e7adc, passed source checks 23–26 at exit 0.
Its eight fresh profiles (root 27) and all 574 setup commands exited 0, using
unchanged d6571f4 native build inputs. Profile facts SHA256 is
7fbeab003506f4ea49e38ecb363678b2a5b54cdf7d4c8f2bdee67ede75206221.
Root 28 PostgreSQL 17 integration exited 101: completed bins passed 11/2/18,
storage passed 12 and failed one, and later bins/private/PG18 tests were not
executed. The corrected fixture observed the real semantic S wait but found
backend_xmin present for the first-unselected view. All thirty before/after
binding readers exited 0 with unchanged identities. Binding facts SHA256 is
2402de0a807e0f14cd383297bd1802f4810fcab207d0a6798322e385708bbc80.
Keep this second failed ordinary run and its eight profile namespaces intact.

The selected caller chain explains the leak. storage_prepare invalidated the
cached native catalog snapshot at entry, then performed installation and
native descriptor/cache work that can recreate it. The subsequent semantic
wait occurs before C registers its own observation snapshot; conditional
observation cleanup therefore cannot close this cached horizon. PostgreSQL 17
get_extension_oid directly scans pg_extension through a native catalog
snapshot; 18 uses EXTENSIONNAME syscache and can need a snapshot on a miss.
The cached catalog snapshot independently occupies RegisteredSnapshots and
protects PGPROC xmin. The selected native invalidation removes that cached
entry and recomputes xmin, preserving active and registered caller snapshots.

The fix makes successful metadata-only preparation end by invalidating that
ephemeral catalog snapshot, outside raw/S and before any subsequent wait.
It is a central preparation postcondition for A/B/C, not a special exception
for this fixture or a barrier bypass. Failed native preparation still needs
normal abort cleanup. The first-unselected and established-view assertions
remain unchanged. No timeout is extended, no new descriptor or options parser
is admitted, and concurrent native 2PC remains required. Native heap_storage.c
and packaged documentation changed, so fresh paired builds and new required
verification profiles are pending. Earlier binaries do not certify this fix.

Passive root-28 snapshot ownership and selected caller readers exited 0.
Snapshot primary capture also exited 0 and binds four named native files to
the official archives plus ten complete selected function bodies, with an
18-member nonself seal. Facts SHA256 is
3fa2d1c3c96bc7c6a412b5851d98e6c75661290f55ff8a7ffcc882886eb47221;
seal SHA256 is 82d173da48ecb1db73333a5af885d0dbcee7830c236ae2e3c667969403f00872.
The four snapshot bodies agree by major, while extension lookup differs.
Verifier v3 remains an uninvoked draft for the now-failed corrected source;
it is not acceptance. Preserve v1/v2/v3 drafts and every failed receipt.
No new native stress, forced error, interruption, recovery or existing-profile
lifecycle experiment occurred. Broader descriptor, writer, closure, execution,
serving, performance and release gates and issues 46/15 remain open.

### Raw relation options: current paired ordinary verification

Native fix and fixture source 38f2548a9a812b6e7252002fa247788a9addcc4b,
tree 77132b3f287831a1e008fca74939f21c1a62a21c, passes recorded commands
32–41 at actual exit 0. Diff hygiene, targeted Rust formatting, repository
boundaries and strict storage-test Clippy pass. Both native packages are rebuilt;
historical binaries do not certify the changed C or packaged documentation.

Root 36 packaging passes: four arm64 Linux PostgreSQL 17.11/18.6 images,
26 exact inputs and fourteen successful build/artifact commands.
Facts: logs/native-options-path-38f2548-packages-v2/facts.json,
SHA256 b5e176ada9d5f2d8fb59e58b58cf2b8b6759df8d38178dd789fffe37d7033ada.
Root 37 passes all eight fresh required profiles and all 564 setup commands.
Facts: logs/native-options-path-38f2548-profiles-17-18-primary-no2pc-unpreloaded-ordered-v3/facts.json,
SHA256 dd4b16d2526de39ef6eec410db54dd74d54864b06bde61bc395e0542168a6f2b.
Supported normal/alternate-preload profiles have ten native 2PC slots; zero
slots remain solely the explicit unsupported configuration. Existing profiles
are preserved without stopping, restarting or signaling them.

Roots 38/40 run the seven affected PostgreSQL integration binaries with
locked/offline Cargo, include-ignored and one test thread. Each major passes
11/2/18/13/2/6/5 tests, totaling 57, with zero failed or ignored.
Roots 39/41 run the private native catalog subset, each passing three with
zero failed or ignored. The total is 120 passed / 0 failed / 0 ignored.
All four commands have thirty successful before/after binding readers,
unchanged container/start identities, zero restarts and the exact current
native inputs. Fixture binding facts are:

- logs/native-options-path-38-pg17-integration-binding-v3/facts.json:
  SHA256 4e0916b7f9051a1e7bdb9621a18036bb96e7f74b6ca9d8155859ad176b8bfe59.
- logs/native-options-path-39-pg17-private-binding-v3/facts.json:
  SHA256 1a5f107ed01df8373d9237a6109871f3378e78a139e46f08b748f418f20451f6.
- logs/native-options-path-40-pg18-integration-binding-v3/facts.json:
  SHA256 857f82e2f0b3e9fbf4d1d1ac54a249f73f28d536d9ab9d3362fe2256c9956b65.
- logs/native-options-path-41-pg18-private-binding-v3/facts.json:
  SHA256 b30eeaba79b9152c7212a8145e780ccc3830b75eb354e7feba0107186c55bf7d.

Both new option fixtures pass on both majors. Ordinary heap/index/parent-TOAST
SET/RESET yields exact source identities, carrier presence and owned images.
Prepared metadata really retains semantic RX while SUE coexists with reader
AS: the consumer waits for S without raw/publication locks. The first-unselected
view has no leaked catalog horizon, and an established caller data view retains
its horizon. The observer completes both native outcomes before the current
definitions are rechecked. Earlier owned results and native prior modes remain
unchanged. No timeout or barrier is weakened.

Only repository documentation changes after this tested revision. All 26
native/package inputs and the changed Rust fixture bytes remain exact.
Verifier drafts v1/v2/v3 stay uninvoked and unchanged; fresh v4 binds final
documentation to current packages/profiles/suites and the two finite selected
primary records. Its producing command is recorded at
logs/native-options-path-44-final-evidence/receipt.json. Final diff/boundary
checks and independent review are merge requirements; this ledger does not
claim those future checks have already passed. Both earlier failed runtime
namespaces and every failed passive reader remain preserved as failures.

This completes local ordinary option-carrier and preparation-horizon
verification only. Broader options/descriptor admission, writer coverage,
transitive type/statement closure, binding/planning/execution, serving,
full performance and release gates, issues 46/15 and the overall goal remain
OPEN. Both use cases and concurrent native 2PC remain required. Standing
security/compiler/hosted-CI/admin/release and new native experiment exclusions
remain in force.

### Descriptor branch declarations: finite implementation pending verification

The next finite component exposes actual pg_class relchecks, relhasrules and
relhastriggers through the existing storage facts and probe. CHECK counts are
nonnegative selected declarations and participate in A/B/C definition equality.
Rule/trigger hints retain actual final-observation bits without being promoted
to invariants: ordinary maintenance may clear them. The bootstrap profile uses
these shared fields instead of redundant private copies. No scan, lock,
descriptor or provider path is added.

Two ordinary fixtures cover independent complete graph oracles, CREATE/DROP,
NOT VALID/validated CHECK counts, native maintenance, duplicates and unrelated
objects, plus both prepared outcomes with first-unselected and real established
RR data views. The latter uses an unrelated data sentinel so target DDL can
prepare normally. Current paired native builds, eight required fresh profiles,
ordinary suites and independent implementation review are pending. Earlier
120-test acceptance remains PR76-only. Existing profiles and all failures are
preserved. Broader descriptor/closure/execution/performance/release gates and
the overall goal remain open; standing scope exclusions remain in force.

### Descriptor branch declarations: current paired ordinary verification

Source 06ce5b026cba6a9c9dc35fb1957f9f3bec14bfcd, tree
e86c183721e0c4f2e32b367a8a2d6d1150b62dc4, passes recorded roots 9–18
at actual exit 0: diff hygiene, targeted formatting, repository boundaries,
strict storage-test Clippy, paired packaging, fresh required profile setup and
both majors' integration/private suites.

Root 13 rebuilds four arm64 Linux images from 26 exact native/package inputs;
all fourteen build/artifact commands pass. Package facts:
logs/native-descriptor-fields-06ce5b0-packages-v1/facts.json,
SHA256 af33cdf213409a93358034494e3ae5e79a0d85d4bee21145b95775707ab701b2.
Root 14 prepares eight fresh profiles with all 571 commands successful.
Profile facts:
logs/native-descriptor-fields-06ce5b0-profiles-17-18-primary-no2pc-unpreloaded-ordered-v1/facts.json,
SHA256 8c8d8d5a20f04348694a9b2fe89b4171772d0e1925539de6a54ad4030e2ddfbf.
Supported primary and alternate-preload profiles retain ten native 2PC slots;
there is no requirement to disable native prepared transactions. Existing
profiles remain preserved without stop, restart or signal actions.

Roots 15/17 each pass 59 integration tests (11/2/18/15/2/6/5 across the seven
binaries); roots 16/18 each pass the three selected private catalog tests.
The total is 124 passed, 0 failed, 0 ignored. Every run has thirty successful
before/after binding readers, exact native inputs and unchanged container/start
identities with zero restarts. Fixture binding facts and SHA256s are:

- logs/native-descriptor-fields-15-pg17-integration-binding-v1/facts.json:
  709d042809d93303e103ee77d6237e73649256ee85c5442fcb664af46f6fda63.
- logs/native-descriptor-fields-16-pg17-private-binding-v1/facts.json:
  037d4455473b3799c143896ae96bfd7060354ec4a5be184faca3e7842e269429.
- logs/native-descriptor-fields-17-pg18-integration-binding-v1/facts.json:
  57d42e3844a98d4e9672eb37c49290fa6c0d6cebda22236503314a8fa822eebd.
- logs/native-descriptor-fields-18-pg18-private-binding-v1/facts.json:
  1348ef49a2f58ba84730882feaa7db99e02c80098386d0a81acfa2f4afb5c534.

Both new fixtures pass on both majors. Actual class hints and CHECK counts
match the independent complete graph oracle through CREATE, NOT VALID checks,
validation, DROP and ordinary VACUUM. The prepared writer retains actual target
AX ownership; the reader waits without coordination fences or an unwanted
catalog horizon. Both commit and rollback preserve a genuinely established RR
data view while current declarations are refreshed. Earlier owned facts remain
historical. Timeouts, lock assertions and 2PC configuration are not relaxed.

Only three repository documentation paths change after tested source; all 26
native/package inputs and the changed Rust fixture remain byte-identical.
The passive root-04 reader's actual exit 1 is a line-bound assertion (157 lines,
requested 158), preserved with raw streams; corrected reader root 05 passes.
It is not a native test failure. Final hygiene/boundary/evidence checks and
independent fresh implementation review remain merge requirements. Broader
descriptor/constraint/type/provider/statement/execution/performance/release
gates, issues 46/15 and the overall goal remain OPEN. Standing exclusions hold.


### Declared type-link discovery source gate

Base: PR77 merged at `0b6aa8667dfb274fff8c5140813ac88a576af3ad`, tree
`d88a1d2383b679140079175c3ae7112ead05f30e`; frozen component readback is
`logs/native-descriptor-fields-17a646c-merged-readback-v3/facts.json`, SHA256
`d2fbe59f0f1f7fe254f30e98f22e93cda0cfa35dd3c9505a98bfd077e2446a8c`.
That component's124/0/0 tests and accepted review certify its original scope.

The next contract is `docs/native-declared-type-links.md`: root rowtype and live
column seeds, recursive actual base/element/array-companion links, iterative
selected membership and independent domain-chain validation, selected default
carriers through a second tracked scan of the existing admitted type reader.
It adds one direct scan per A/B/C observation (24 total), while leaving the
metadata roots/physical modes unchanged. Whole fixed maps and selected queues
use existing cumulative budgets, with an explicit4096 selected-type limit.
The full type collector cost/sequence has been corrected for the existing class
options pass and the required single raw span; its new descriptor gates stay open.

Current passive readers07/08 at this clean base actually exited0. The primary
record `logs/native-type-links-primary-v1` owns selected pinned17/18 type headers,
facts SHA256 `14344de93dc3f19f6d54b0c061cfa574d4228f424d497d19dc55be21200762fa`
and four-member nonself seal SHA256
`cafbd4cc43aa510fbb8a495f56d87fcdf0d5bb5dc48d4d1a76ed01c54b2c2866`.
No new C, Rust, packages or runtime results exist at this source checkpoint.
Independent source acceptance is required before the proposed C changes; current
paired packages/ordinary suites and fresh implementation review remain required
afterward. Full composite/range/enum/domain-constraint/provider/statement,
execution, serving, performance, release and overall goal gates remain open.


### Declared type-link implementation checkpoint

The existing-reader source gate was independently accepted at
`1e4c7581130da8a6a0bffb6d2a218b0cf3425efa`, tree
`d2ff0ffd1b21e36882b533427e97a9e5727b54ec`. Root11 rehash facts SHA256
`a33467334ebcb93e263e3162dd8b575148a647fd99526b0d4ae1e7c45e5683c5`
binds565 fixed members plus16 completion companions; root12 separately checks
observer21 actual0/empty stderr. Acceptance applies only to this selection/pass.

C now separates whole fixed type-map membership, selected-node count and own
binary/text default completion. Root rowtypes and live slot types seed actual
base/element/array companions. Membership terminates ordinary cycles; independent
iterative domain colors detect base cycles. The queue starts small and grows
geometrically under cumulative requested-copy accounting, with4096 checked before
insertion; domain scratch uses actual selected count. The tracked second type
scan stays under the same registered raw snapshot and closes with the other scans.
Sorted selected facts and each actual default carrier keep existing A/B/C equality,
B normalization and caller data-snapshot/ownership/abort boundaries.

The probe reports type_payload_rows separately; the ordinary SQL oracle computes
its own recursive UNION node set and observes both default fields. Existing
zero-column and default expectations are updated. Two ordinary fixtures add
ancestor defaults/companion/shared/duplicate/empty/own-TEMP cases and prepared
link/default changes through both commit/rollback and real data-view states.
Current formatting/diff pre-commit checks pass, but current strict paired native
packages, required ordinary suites and fresh implementation review remain pending.
No previous runtime result certifies these changes. The full definition/bootstrap/
provider/statement/execution/serving/performance/release and goal gates stay open.

### Declared type-link paired run and publication-fixture correction

The first implemented source was
`39e83887b5bfea493baa2a3dde6a621de87c908f`, tree
`d512dddb73d6e785e6b8b6ea1ba068bde2442ee3`. Root13–16 formatting, diff,
repository boundaries and strict targeted Clippy passed. Root18 produced four
strict PostgreSQL17.11/18.6 product/probe packages with26 bound native inputs and
14 actual0 commands. Package facts SHA256:
`e44897911110642ad60abeae431d44429df9c2c80e85d3cc631d8eb85754db96`.
Root19 verified eight fresh required profiles with620 actual0 commands; facts
SHA256 `7e336a6b68ece0c96f9a7a959f7e856375216034f7c0cc2e61c307695ddea311`.

Root20/21 passed all61 integration and3 private catalog tests on PostgreSQL17,
including both new type-link fixtures. Each had30 actual0 before/after readers
and unchanged profile identities/binaries. Their binding facts SHA256 values:
`061f5c908e96aa74585f4ab20317d9b025c1b3d4c2ac7ccbc7e0e37f765859f9`
and `6f2d10bccbb10a8ad99698a0a4ac48c6b2a6d6293c4ca972e59ea09afae2896e`.

Root22 is preserved actual101 on PostgreSQL18: catalog discovery11 and builtin
dispatch2 passed, then catalog publication had17 passed/1 failed/0 ignored. The
remaining integration targets and private suite did not run. Binding facts
SHA256 `e7b26a905a29deff24c0b6b6ae9913f242298bd3ddc4b4c730eac513ec5ceba2`
records30 actual0 readers and unchanged identities/binaries. All eight profiles,
packages, raw streams and namespaces remain preserved.

The failed publication assertion compared generation128 from an unheld one-shot
observation with generation131 after acquiring a retained reader Share. The
one-shot releases its fences before output (`catalog_read.c`762–875); the probe
only acquires its retained Share at `darmok_catalog_probe.c`772–788. A publication
between these scopes is allowed. Root23–26 read the exact failed fixture and
these existing native paths; this is not a new kernel/registry/history census.

The fixture now checks monotonicity across that unheld interval and uses the
observation made under the first retained Share as its DDL baseline. Its existing
two-reader coexistence, pending Exclusive, release-one-still-blocked and final
completion checks stay intact; post-DDL generation must advance from the held
baseline. It adds no requests, sleeps or timeout changes. Native product/probe
inputs are unchanged by this correction. Fresh paired packages, profiles, suites
and implementation review are still required for the corrected source.

The uninvoked `verify-native-type-links-evidence-v1.py` draft remains preserved
at SHA256 `58de0a36bebc59be4172d359524faa9780e93f10e5e0f53f808dcbfdf67c17ea`;
it has no execution or acceptance claim. Full collector/bootstrap/provider/
statement/execution/serving/performance/release and goal gates remain open.

### Declared type-link corrected paired verification

The corrected tested source is
`abe6b3411ac20dbce81efb205322049058be7c06`, tree
`04b73cb6be81f29d36f1f75c29f821e4850d38e7`. Root27–30 passed targeted formatting,
feature diff, repository boundaries and strict Clippy for both changed Rust
fixtures. Root31 verified all26 native inputs unchanged from39e and only the
publication fixture/release ledger changed in the correction.

The uninvoked v2 package-helper draft's broad namespace edit also changed its
fixed corpus leaf to native-build-corpus-primary-v2. Its complete diff exposed
this before invocation; the draft remains preserved at SHA256
`ab03c0f88dcb1d9bb96f707ef4eb2a1f442a845eb2ee6662470df01d6e07f27b`,
with no build/profile namespace or execution claim. Root32 created the distinct
v3 helper, restoring that fixed v1 leaf. Its SHA256 is
`30d6acb73c236e5e54823a03994286a8c1eceaf3732010d17bdf24feeea0d36f`;
the current package/profile/binding namespaces are fresh v2 namespaces. Complete
helper differences were read before these corrected helpers ran.

Root33 strictly rebuilt/verified four PostgreSQL 17.11/18.6 product/probe packages,
14 actual0 commands and26 exact native inputs. Package facts SHA256:
`c56f56a4b61a32a20e2fbb91e2c1966929aef5e78aba3e0a7a979455f9cc1876`.
Root34 verified eight fresh required profiles with595 actual0 commands; facts
SHA256 `bcabcdead82bea2670618e779f8ef5dcd775d7610a9abdfc29f650aaa2f50bf8`.
Both primary profiles have native two-phase transactions enabled. The no2pc
profile remains an existing boundary variant, not a serving restriction.

Root35–38 passed all 128 required tests:61 integration and3 private catalog tests
per major, zero failed and zero ignored. Private runs use --ignored and filter
101 unrelated tests. All four runs have30 actual0 before/after readers with
unchanged container IDs, images, start times, restart counts and libraries.
Binding facts SHA256 values:

- PostgreSQL17 integration:
  `5dd4e663daa826bff8a6a5caca55b1b1c39a872d5ec595e9e11350f7e3ad87d6`.
- PostgreSQL17 private:
  `afd673c442c27f26778a8673db92b729a6105b5deba4747e6c73f3a16eb0a847`.
- PostgreSQL18 integration:
  `a62d6857d40241cef4108e7a070d4fbc0de3cdcd2526f816cc67b3018cd4b471`.
- PostgreSQL18 private:
  `e163d06c438e1caeb60014a4893f139aeea6465fb6eb6b573a5ab695ebcaa0fc`.

Root39 checks the exact current counts and code bindings; its complete stdout
SHA256 is `2e48355c8ff8cd3ad2717848ec2ca3cddf459a4fa39ea76ce8991dc0d1bb94c0`.

Both new type-link fixtures and the corrected publication fixture pass on both
majors. These ordinary results cover current selected facts/default images and
existing native ownership/data-view cases, not a full transitive type definition
or semantic/performance certificate. Final changes after this tested source are
confined to these nonpackaged documentation updates. Independent fresh
implementation review is required before normal exact-head squash merge. The
full collector/bootstrap/provider/statement/execution/serving/performance/release
and goal gates remain open.

### Declared type links merged; composite field source gate proposed

PR78 merged normally at `a5cb93019284ec9f1e77a330fb5012d337981cc5` on
2026-10-04T16:18:42Z. Its sole parent is
`0b6aa8667dfb274fff8c5140813ac88a576af3ad`; merged tree
`1953e986ab779b6b7ae353846aee42c3fb6d9c5c` equals independently reviewed clean
`ea284b342358e4ecd9d17ea202b0cd0536410b0f`. The fresh finite implementation review
found no required corrections and verified 128 actual passes. Report SHA256
`461ab381039dba75a1eb5698e1ddd89ac47b357cdaa90729eeb72519388f4449`,
2965-member nonself seal
`6ba345f7dd616ce42ce6ac6ec2bc8ff31ce15a2d4eef17585776267742a9f50e`,
16 completion companions; final/post/completion/observer each independently 0.
Root45 rehashed 2987 unique paths and root46 separately verified observer30 and
its raw streams. The preserved native101 and reviewer bookkeeping1 remain
failures in their original records.

Root47–53 checked the public personal repository and exact reviewed head,
pushed only the feature branch, created PR78, normally squash-merged that head
and fetched the merged object. The filtered noncompiler queue was empty;
description remains “A MySQL-to-PostgreSQL compatibility proxy”. Root56 actual0
verifies the unchanged source, exact merged parent/tree and 227 fixed members in
`logs/native-type-links-ea284b3-merged-readback-v2`: facts SHA256
`d620535fbded9faa98bf371b210f62f44385a439b04a07ba59d4f73c0b85613c`,
seal `0fd0337e4fc2032f430f1a4502e66dae01518823724c68251550842f4b6e1631`.
The fully read uninvoked readback-v1 draft's nonexistent receipt fields were
corrected in a distinct v2 helper before invocation; neither old evidence nor
profiles were reused or changed. Named previous leaves were not re-expanded.

The next finite [composite field source contract](native-composite-type-fields.md)
proposes a whole fixed attribute map, grouped selected composite traversal and
one selected missing-carrier pass through the same six admitted readers. It
distinguishes copied standalone/table definitions from physical descriptors,
preserves root provenance and adds one measurable scan per observation. Its
independent source gate must close before C changes. No new native execution,
application descriptor, provider or complete type/statement admission is claimed;
full collector/bootstrap/execution/performance/release and overall goal stay open.

### Composite field source design accepted; implementation verification open

The finite source-only review at clean `c43464987b02aa743e95cb7a53bc130e73221a58`
found no required corrections, accepting grouped composite traversal through the
already admitted readers conditionally. Report SHA256:
`b0ea70dd1ff346c2c1a1223375002eb8a3f4dfd1adc202aaf0bf2a37faf4c772`;
518 fixed members,16 completion companions, final/post/completion/observer each
actual0. Root16 independently rehashed540 unique paths; root18 checked the exact
observer receipt and streams. Reviewer reader17's range error and root17's wrong
JSON key remain actual1 in their original namespaces. Root23's wrong probe
pathname is likewise preserved as actual1; root24 resolved its exact filename
and distinct root25 read the intended probe/fixture bodies at actual0. These
reader failures are not native executions or erased successful labels.

The [composite field contract](native-composite-type-fields.md) now has a C/header/
probe implementation: whole positive fixed map and relation groups, bounded
selected traversal, separate tracked missing-carrier pass and complete composite
fact/range comparisons. It removes the obsolete selected-root column map and
does not extend application physical graphs or open application descriptors.
Independent recursive SQL and class/attribute/default/missing oracles expand to
all selected composites. Two additional ordinary fixtures cover nested/shared/
dropped/inherited/nonroot/default/missing/own-TEMP declarations and prepared
composite-plus-root changes, both native outcomes and real data-view states.
Nine direct scans per observation/27 across A/B/C are exposed; phase budgets do
not certify peak memory, contention or throughput. Paired strict packages, fresh
required ordinary PG17/18 profiles/suites and independent implementation review
remain open. Concurrent native2PC remains required. Full collector/bootstrap/
provider/statement/execution/performance/release and overall goal remain open.


### Composite runtime failure preserved; independent oracle consolidated

Four strict paired packages completed at13ca94f. Initial profile setup root31
failed a readiness deadline during stock initialization; six individually
complete profiles and two distinct fresh profiles are explicitly assembled,
while the failed unpreloaded PG18 profile stays preserved and unaccepted.
PG17 required integration root39 exited101: discovery11, builtin2 and
publication18 passed, heap8 passed/11 timed out; later binaries did not execute.

Independent source diagnosis found no required C correction within the finite
copied-composite contract and one medium fixture correction R1 (duplicate type
selection/seven payload-oracle requests). The
[component ledger](native-composite-type-fields.md#preserved-verification-failure-and-oracle-correction)
binds the original failed streams and the311-member review with actual-zero
terminal observer and333-member root rehash. R1's cost is established; a common
cause for all timeouts remains unproven.

Fixtures now share independent selection per metadata phase, batch the six
catalog graphs and record phase outcomes/elapsed time. Original20s/60s bounds,
phase freshness, NULL/carrier/source/graph assertions, historical outputs, real
prepared wait barriers and concurrent native2PC stay required. The26 packaged
native inputs are unchanged by this fixture correction. Current paired ordinary
runtime verification and final implementation review remain OPEN, as do the
full collector/provider/execution/performance/release gates and overall goal.


The first consolidated fixture head a841adf passed66 required PG17 cases.
PG18 root55 passed18 heap cases, including both new composite cases, but the
existing carrier fixture expired its60s case bound after all12 carrier samples.
The failed receipt and unchanged profiles are preserved. The
[phase-sharing ledger](native-composite-type-fields.md#graph-expectations-complete-the-per-phase-oracle-sharing)
records the exact streams and evidence limit: repeated catalog graph queries
were avoidable; the cause of variable query latency remains unproven. Independent
graph expectations now join the per-phase shared oracle, while every native
capture and all assertions/barriers/bounds remain. Current runtime acceptance
and final review are still OPEN.
