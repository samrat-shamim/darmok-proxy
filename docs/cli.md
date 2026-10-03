# Explicit database setup commands

The workspace's `darmok-cli` package builds the `darmok` executable. It currently
implements these commands:

```text
darmok init --database-url-env ENV_NAME
darmok verify --database-url-env ENV_NAME
darmok --help
darmok --version
```

The version is the workspace package version, not a release certification.
`serve`, `config check`, `doctor`, logical aliases and the configured route graph
remain pending. These commands advance the standalone initialization path;
they do not establish an end-to-end serving proxy or complete M3.

## Input and database selection

`ENV_NAME` names an existing environment variable containing the native
PostgreSQL connector's connection settings: either a PostgreSQL URI or its
keyword/value format. A database name must be explicit and nonempty in those
settings. The connector's default of selecting a database by username is
deliberately rejected for setup operations. Initialization never creates a
physical database. Each invocation operates on one selected database.

There is no CLI configuration file or frontend credential mapping in this
component. The commands use the existing connector with `NoTls`; TLS work and
credential/grant policy remain deferred. PostgreSQL 17/18 and UTF8 are the
functional requirements. Installed extension files and an already preloaded
`darmok_server` module are also required; see the
[combined setup contract](native-database-init.md).

Help, version and usage parsing finish before starting a runtime or connection.
For a database command, the CLI parses the settings, establishes a fresh native
owner, calls the reviewed initializer or verifier, and awaits owner disposal.
No caller SQL or raw native client is exposed through the production command.

## Outcomes

Initialization installs the compatibility format1 schema and server extension1.0
only when absent, or validates an exact existing installation without replacement.
Verification uses the read-only transaction and never creates or repairs objects.
Both operations require the live native handler's strict empty protocol1 response
before a separate checked COMMIT. The [combined setup contract](native-database-init.md)
specifies functional manifests, observed completion and rollback behavior;
[the schema contract](native-schema.md) specifies compatibility helper semantics.

Success is printed to stdout only after combined completion and local driver
disposal succeed:

```text
Darmok database initialization completed for compatibility format version 1 and server extension 1.0
Darmok database verification completed for compatibility format version 1 and server extension 1.0
```

Exit codes are `0` for success/help/version, `2` for usage errors and `1` for
settings, connection or database setup failures. Failure output goes to stderr, with
no success line. Missing/invalid settings name the environment variable.
Connection failures include a SQLSTATE when the connection error supplies one.
Manifest errors include the primary SQLSTATE/message and separately report
confirmed rollback or rollback failure. A disposal failure after committed
setup success explicitly says that the operation completed but local disposal
failed; it cannot be reported as an installation rollback. The CLI does not
retry, repair, migrate or convert an uncertain result to success.

Local driver disposal confirms local task termination only. It does not resolve
an uncertain server commit or establish a general asynchronous cleanup promise.
Forced interruption and transport failure experiments are deferred.

## Cost and verification

One invocation creates one current-thread Tokio runtime and one native owner.
Argument/settings allocations occur at startup, outside any row execution path.
The CLI adds no SQL to the initializer's fixed transaction and no catalog cache.
Owner connection setup retains its existing round trip. Combined setup uses
two operation round trips, validating the native response before commit;
a confirmed failed setup request uses explicit rollback instead of commit. Runtime
query latency and complete artifact benchmarks remain pending.

Offline tests run actual binaries for help, version, usage and ordinary settings
errors. Four required process groups use disposable physical databases on each
PostgreSQL major: fresh/repeated initialization and read-only verification with
unchanged application state; ordinary partial/version/function mismatches with
no repair; independent installations in two explicitly selected databases; and
extension nonmembership dependencies versus extra members/reserved objects.
They check actual child exit codes/output, catalog OIDs and row/catalog `xmin`,
successful URI and keyword/value inputs selecting an explicit nondefault
database,
helper NULL/UTF8/arbitrary-byte behavior, normal connection closure and database
removal, and an ordinary nonexistent-database connection error. Missing database
infrastructure fails the required test invocation.
This is physical selection evidence, not logical routing authorization.

`DARMOK_CLI_PROCESS_EVIDENCE_DIR`, when set for tests, records every child's
actual argv, settings format, selected physical database, timestamps, status and
original stdout/stderr in unique files. It is a test evidence destination, not
a product setting. Hosted PostgreSQL CI
requires these groups explicitly; local evidence and the account CI blocker are
recorded in [the release plan](release-plan.md).

## Historical schema-only local candidate

The following prior schema-only process/runtime checks apply to code revision
`6c0f3e899687e5cf4bf27c17d1178d0e643f3250`, tree
`d30e82956723e92049d9a64b8a17275d8399a13f`. Each PostgreSQL major runs three
required groups and 23 actual command children: 11 exits0 and 12 expected exits1.
Both keyword initialization and verification succeed against the explicit
nondefault database, with unchanged installation/application snapshots. The
offline groups run 16 children per workspace feature set: five exits0, five
expected exits1 and six expected usage exits2.

All15 matrix commands exit0. Independent review closes the original positive
keyword coverage observation; no functional finding remains. The copied
optimized macOS arm64 development binary is checked for help/version from an
empty working directory only. Its schema behavior, other artifact platforms,
serving, real drivers and release readiness are not established by that smoke
check. The complete local hashes, exact command/environment records, preserved
bookkeeping failure and pending hosted CI are in the release plan. This historical evidence does not certify the current combined setup or relabel
runtime executions as a later source revision.

## Historical combined setup runtime candidate

The earlier combined setup checks are bound to implementation
`07647b50af46bd215616a154bd332855d630d7c8` / tree
`9d001170be930f4f09a396d0b5bd9e2ded4ffdef`. Both PostgreSQL majors pass four
required process groups with 32 children each: 15 exits0 and 17 expected exits1.
Both URI and keyword settings select the explicit nondefault database. The
default and BigDecimal offline invocations each run 15 children: four exits0,
five expected exits1 and six expected usage exits2. Setup-library fixtures pass
13 groups on each major, and the affected catalog-owner fixtures pass three.
The release plan records the exact commands, source and evidence. The later C1
checker correction is not present at076; this earlier result does not certify
that correction. The historical schema-only results above retain their scope.

## Reviewed combined setup implementation

The corrected implementation is independently accepted and locally checked at
`3b9a15e2523a15624efb35e186bb20a4401061c8` / tree
`1b7f4a1020e2c1d5ccf688469c556d46e8038446`. Each PostgreSQL 17.11/18.6 suite
passes 13 setup library, 3 catalog-owner and 4 CLI process groups, with zero
failed/ignored required native tests. The process groups retain 32 children per
major: 15 actual0 and 17 expected1. Default and explicit `sqlparser/bigdecimal`
offline checks each pass 5 catalog, 3 CLI and 22 execute tests, including the
malformed error tails, and retain 15 command children each (4 actual0,
5 expected1,6 usage2). Unrelated4 process and76 execute ignores remain explicit.

All 12 recorded checks, including formatting, nine-package boundaries and strict
workspace/connector Clippy, pass. Four native product/test packages built at
6217c0e include the current README; all 20 native inputs remain equal at3b9.
Saved actual native library identity permits use of the eight existing954
profiles with their historical packaged README. The final documentation leaf
only changes unpackaged instructions and validation records. Runtime and
packaging results remain bound to their respective revisions, not relabeled as
new executions. See the release plan for exact evidence and independent review.
Full serving, executor, artifacts on other platforms and release gates remain open.
