# Explicit native schema initialization

This component installs and verifies version 1 of the functional `darmok`
schema in one physical PostgreSQL database. `NativeBackend::initialize_schema`
and `verify_schema` use the existing exclusive connection owner. They expose
no caller SQL or raw client. CLI wiring, configuration, credentials, grants,
routing, `serve` admission and the executable remain pending. This receipt
certifies functional artifacts, not privilege policy or runtime readiness.

Both operations require confirmed idle. They reject an existing transaction
before submission and never commit or roll back a caller's outer work. Each
submits one fixed batch: explicit read-committed, non-deferrable `BEGIN`, a
manifest `DO`, and `COMMIT`. Initialization uses read-write access; verification
uses read-only access. Success requires exactly `BEGIN`, `DO`, `COMMIT` and
observed idle `ReadyForQuery`. It does not distinguish newly created from
already present artifacts. No command-line success is claimed by this library.

The manifest takes a database-scoped transaction advisory lock using the fixed
signed key `4922526098346491905`. Initializers take it exclusively; verifiers
take it in shared mode. This serializes these explicit operations only. It is
not a general DDL guard or an execution-validity lease. Read-committed access
allows the operation to observe another initializer's committed installation
after acquiring the lock. No automatic retry occurs.

On absence, initialization creates the namespace, singleton metadata table and
four functions in the same transaction. Verification reports absence and never
creates objects. A pre-existing namespace must contain the matching functional
inventory, table, primary index, composite and array types, function signatures,
bodies and behavioral flags, and one matching metadata row. Validation checks
table structure before reading its data. Additional namespace objects, partial
installations, changed helpers or version/profile mismatches are errors.
PostgreSQL 18's separate NOT NULL constraint validation is checked alongside
the PostgreSQL 17 attribute representation. Ownership, ACLs and privilege
properties are deferred with the excluded security work; physical tuning
properties are not functional version identity. No repair, replacement,
migration, legacy behavior or application-object inference is provided.

An ordinary manifest SQL error is retained with its observed prefix tags and
failed-transaction readiness. The owner then awaits an explicit `ROLLBACK` and
preserves its result separately in `NativeSchemaFailure`. Rollback failure
cannot become a successful reset. Stream errors, unexpected tags or missing
readiness retain the owner's uncertain state and require disposal. A failure
observed in idle remains an error without inventing a rollback receipt. Dropped
operations have no asynchronous cleanup guarantee.

Version 1 requires PostgreSQL 17 or 18 and UTF8 database encoding. Metadata is
`darmok.installation(singleton bool PRIMARY KEY CHECK (singleton),
format_version int4 NOT NULL, profile text COLLATE pg_catalog."C" NOT NULL)`.
Its sole row is `(true, 1, 'substring-signed64-utf8-bytes-v1')`.

The immutable, strict, parallel-safe SQL kernels are:

| Function | Arguments | Result |
| --- | --- | --- |
| `darmok.substring_utf8` | `text, int8` or `text, int8, int8` | `text` |
| `darmok.substring_bytes` | `bytea, int8` or `bytea, int8, int8` | `bytea` |

Positions are one-based. Negative positions count from the end. Position zero,
positions beyond either end, and nonpositive lengths produce an empty value.
NULL in any argument produces NULL. Omitted length consumes the remainder.
UTF8 text counts Unicode code points; bytea counts bytes and preserves arbitrary
bytes, including NUL. Bounds are checked in signed 64-bit arithmetic before
converting the native substring offset and count to int4. The helpers do not
implement frontend coercion, unsigned arguments, warning translation, other
encodings or grapheme counting. Their reference behavior is MySQL 8.4
[SUBSTRING](https://dev.mysql.com/doc/refman/8.4/en/string-functions.html#function_substring).

Function bodies qualify built-in types, functions, operators and helper names.
Manifest declarations and casts qualify built-in types. An implicit temporary namespace can shadow even
an unqualified built-in type despite an explicit `pg_catalog` search path;
qualifying manifest declarations and casts avoids that ordinary resolution
failure. Function bodies are versioned
as SQL source strings to permit exact functional comparison across the two
supported PostgreSQL majors. String-body SQL functions do not provide the
creation-time dependency tracking of a parsed SQL body; see PostgreSQL's
[CREATE FUNCTION](https://www.postgresql.org/docs/18/sql-createfunction.html).
This component supplies no execution-validity lease. Helper name qualification
and output metadata cannot close the dependency gate in `native-execution.md`.

Successful installation/verification adds one control round trip; an observed
failed transaction requires a second for rollback. Schema catalog work occurs
on these explicit operations, not per row. Shared/exclusive advisory locks
serialize initialization rather than application query execution. Runtime
kernel performance and frontend SQL admission remain release gates.

Required private fixtures use distinct disposable databases on PostgreSQL 17
and 18. They exercise creation, unchanged repeats, read-only verification,
missing/partial/conflicting installations, definition and metadata drift,
transactional rollback, preservation of outer work, two ordinary owners
initializing/verifying one database, and the independently
captured 810-case stock MySQL 8.4 corpus. This is component evidence; M3 and the
full release remain open. Exact revision, commands and outcomes are recorded in
`release-plan.md` after verification.

The corpus also has a repository-local standard-library observer. With the
existing stock fixture's `MYSQL_PWD` environment, run:

```text
python3 tests/reference/observe_mysql_substrings.py --container <reference-container> --image mysql:8.4@sha256:6ea90827b1100f8f2ae306a539f86d2c264a26ed435a2a9f75551dd5c3aeb242 --corpus crates/execute/fixtures/native-substring-mysql84.json --evidence-dir <new-external-directory>
```

It verifies the pinned image, server version, all values and immediate statement
warning counts, preserving raw commands and streams. The native fixture is
required by both existing PostgreSQL native-owner jobs; the stock observer is
required by the MySQL reference job. Hosted jobs have not been run for this
component while the account blocker persists.

The final implementation/fixture check is bound to revision
`edd7361ffcd4b393986fc5e4e5e83c5edb77f622`, tree
`6b00ffd146641108a6276b105616af881c6961bd`. All twelve revised regression
commands passed, including nine initialization groups on both backend majors
and the default/BigDecimal parser configurations. The ninth group installs
ordinary temporary types named like built-ins, confirms their implicit lookup,
then verifies installation, unchanged repeats and both helper families.

Earlier source/evidence reviews identified missing future-CI receipt retention;
the workflow correction adds an always-run upload using the existing pinned
action. A subsequent self-review reproduced unqualified manifest type resolution
failure; declarations and casts now use explicit `pg_catalog` types. Installed
helper bodies and schema artifacts retain version 1. Historical receipts and
reviews remain separately bound to their original revisions. The final evidence
leaf changes documentation only and preserves all implementation, fixture,
corpus and observer bytes and sealed runtime evidence. No hosted-CI pass,
executable integration or complete M3 gate is inferred from these local results.
