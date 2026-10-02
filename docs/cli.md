# Explicit database setup commands

The workspace's `darmok-cli` package builds the `darmok` executable. It currently
implements these commands:

```text
darmok init --database-url-env ENV_NAME
darmok schema verify --database-url-env ENV_NAME
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
functional schema requirements.

Help, version and usage parsing finish before starting a runtime or connection.
For a database command, the CLI parses the settings, establishes a fresh native
owner, calls the reviewed initializer or verifier, and awaits owner disposal.
No caller SQL or raw native client is exposed through the production command.

## Outcomes

Initialization creates version 1 only if the reserved schema is absent. An
existing exact installation succeeds without replacement. Verification opens
the reviewed read-only transaction and never creates or repairs anything.
The fixed manifest, supported artifacts, mismatch cases, confirmed completion
and rollback behavior are specified in [the schema contract](native-schema.md).

Success is printed to stdout only after both schema completion and local
driver disposal succeed:

```text
Darmok schema initialization completed for format version 1
Darmok schema verification completed for format version 1
```

Exit codes are `0` for success/help/version, `2` for usage errors and `1` for
settings, connection or schema failures. Failure output goes to stderr, with
no success line. Missing/invalid settings name the environment variable.
Connection failures include a SQLSTATE when the connection error supplies one.
Manifest errors include the primary SQLSTATE/message and separately report
confirmed rollback or rollback failure. A disposal failure after committed
schema success explicitly says that the operation completed but local disposal
failed; it cannot be reported as an installation rollback. The CLI does not
retry, repair, migrate or convert an uncertain result to success.

Local driver disposal confirms local task termination only. It does not resolve
an uncertain server commit or establish a general asynchronous cleanup promise.
Forced interruption and transport failure experiments are deferred.

## Cost and verification

One invocation creates one current-thread Tokio runtime and one native owner.
Argument/settings allocations occur at startup, outside any row execution path.
The CLI adds no SQL to the initializer's fixed transaction and no catalog cache.
Owner setup and schema-operation requests retain their existing round trips;
ordinary failed transactions add the owner's explicit rollback request. Runtime
query latency and complete artifact benchmarks remain pending.

Offline tests run actual binaries for help, version, usage and ordinary settings
errors. Three required process groups use disposable physical databases on each
PostgreSQL major: fresh/repeated initialization and read-only verification with
unchanged application state; ordinary partial/version/function mismatches with
no repair; and independent installations in two explicitly selected databases.
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

## Recorded local candidate

The successful process/runtime checks apply to code revision
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
bookkeeping failure and pending hosted CI are in the release plan. This evidence
documentation does not relabel runtime executions as a later docs-only leaf.
