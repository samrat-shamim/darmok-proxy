# Stock transaction reference fixture

Status: **required finite MySQL reference suite; actual proxy verification is
pending**. The corpus and runner live in `tests/reference/` and need only this
checkout, Python 3 and Docker with the selected existing MySQL container/image.
The observer requires an explicit corpus; the eleven
[transaction-characteristic cases](mysql-transaction-characteristics.md) use
the same mechanism with separate SQL/data and evidence. Missing dependencies or
incomplete observations fail the required check.

The thirteen cases preserve the earlier transaction observations: duplicate-key,
CHECK, NOT NULL and missing-column errors with prior/next writes; complete
multi-row recovery; client savepoint retention and name reuse; ordinary and
temporary DDL boundaries; autocommit transitions; and beginning a transaction
while one is active. The case SQL, error expectations and final effects are data
in `mysql_transactions.json`, separate from the observation mechanism.

An expected error names its exact single-line SQL statement, its occurrence,
numeric code and SQLSTATE. The runner maps this declaration into the complete
submitted request's physical lines, then compares the entire CLI stderr list.
An error from the earlier identical savepoint statement cannot satisfy the
later occurrence. `--force` is used only for declared errors; a zero client exit
alone is insufficient. Other stderr, extra/missing errors, an incorrect marker,
an extra result row or different effects fail the observation.

Each request has one unique trailing completion marker. A case has one JSON
effect row with the expected integer counts, strings and NULLs. JSON comparison
keeps booleans distinct from integer counts. These finite effects do not define
general numeric/JSON coercion or MySQL wire type policy. Raw error text is
retained, while exact message wording, warnings and frontend status flags remain
separate obligations.

## Running the fixture

Provide `MYSQL_PWD` in the caller's environment for the stock fixture. Select an
existing container using the pinned image, and a new evidence directory outside
the checkout:

```sh
python3 tests/reference/observe_mysql_transactions.py \
  --container mysql-reference-84 \
  --image mysql:8.4@sha256:6ea90827b1100f8f2ae306a539f86d2c264a26ed435a2a9f75551dd5c3aeb242 \
  --corpus tests/reference/mysql_transactions.json \
  --evidence-dir /tmp/darmok-mysql-transaction-reference
```

The default stock fixture user is `root`; `--user` selects another fixture user.
The runner assigns a fresh UUID database name, requires its absence before
creation, and creates the declared tables only there. After an attempted CREATE,
cleanup attempts to remove that assigned database and checks its absence. Cleanup
failure is recorded separately and fails the run. Successful ordinary runs prove
their final absence; failure branches require separate evidence.

Evidence contains exact source/corpus hashes, Git revision and dirty state,
platform/versions, selected image/container identities, each process command and
exit, submitted SQL, CLI streams, mapped errors, values and artifact hashes.
Processes are counted separately as SQL, inspection and metadata. The ordinary
thirteen-case run uses nineteen SQL clients, two read-only image inspections and
three local metadata commands. Case requests add one constant marker SELECT;
these are test costs, not a proxy performance measurement.

CI provisions the pinned MySQL service and runs this suite as a separate required
job. It runs both declared corpora and retains their receipts as separate
`mysql-transaction-reference` and `mysql-transaction-characteristics` Actions
artifacts, including failed observations. Existing PostgreSQL 17/18 fixture jobs
remain separate. Driver, wire, lock/snapshot, catalog-validity, complete recovery
classification and release gates remain pending. See the
[frontend recovery contract](transaction-recovery.md).
