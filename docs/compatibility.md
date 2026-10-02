# Compatibility policy

Darmok 0.1.0 is experimental. The reference is MySQL 8.4; supported PostgreSQL
backends are intended to be 17 and 18. These are test targets, not evidence of
current support. No end-to-end compatibility claim has been certified yet.

## What support means

Every construct is classified as one of:

- **Supported**: verified observable behavior, including errors, metadata,
  warnings, NULLs, rows affected and transaction effects.
- **Constrained**: supported only under documented, machine-checked conditions;
  other inputs fail explicitly.
- **Unsupported**: rejected with a specific error before unintended effects.
- **Unverified**: no support claim. Parser recognition is insufficient.

The executable registry must distinguish parsing, semantic acceptance,
translation, execution, and evidence. A successful parse or a plausible emitted
SQL string cannot promote a feature to supported.

Transaction effects include lock lifetime and snapshots, not only final values.
The [frontend recovery contract](transaction-recovery.md) records observed
MySQL/native savepoint differences and the remaining mechanism requirement.
Native rollback receipts cannot promote transaction behavior to supported.

## Initial verification priorities

| Area | Required cases |
| --- | --- |
| Queries | Joins, aliases, subqueries, CTEs, grouping, ordering, DISTINCT, LIMIT, NULL semantics |
| Writes | INSERT/UPDATE/DELETE, defaults, generated IDs, explicit IDs, duplicate keys, RETURNING emulation, affected rows |
| DDL | Create/alter/drop, types, constraints, indexes, defaults, identity, temporary objects, implicit commits |
| Metadata | SHOW and information_schema for proxy-created and native objects; schema/role visibility; quoted names |
| Types | Signed/unsigned ranges, exact decimals, binary/text, dates/times, time zones, JSON, native unsupported types |
| Sessions | SQL modes, charset/collation, session variables, warnings, LAST_INSERT_ID, FOUND_ROWS, reset/change user |
| Transactions | Autocommit, begin/commit/rollback, statement errors, savepoints, locks, disconnect and cancellation |
| Prepared statements | Parameter and result metadata, NULLs, binary encoding, repeated execution, route changes and DDL |

Test string comparison, sorting, LIKE, coercion and collation semantics rather
than accepting collation names without implementing them. Never reinterpret a
legitimate native PostgreSQL date as a MySQL zero date without explicit column
representation metadata. Approximate compression, JSON, weight-string or
full-text behavior cannot be labeled compatible.

Do not add an ORDER BY to an unordered query to satisfy a fixture's incidental
ordering. Differential comparisons must account for SQL nondeterminism while
still checking multiplicity, types and values.

## Deliberately outside the first release

Replication/binlog serving, distributed transactions/XA, stored routine
execution, unrestricted cross-physical-database SQL and arbitrary plugin loading
are outside scope. Additional unsupported protocol modes are enumerated by the
capability registry. Unsupported syntax must fail deliberately; backend syntax
errors are not a substitute for an advertised compatibility contract.

The catalog lease profile requires a PostgreSQL primary with native two-phase
transactions disabled (`max_prepared_transactions=0`). This is separate from
MySQL prepared statements, which remain in the target capability matrix.
The [catalog lease contract](server-catalog-lease.md) describes the table-lock
dependency behind that installation requirement.

## Required evidence

1. Unit tests of parsing, rewrite decisions and type/protocol boundaries.
2. Differential execution against MySQL 8.4 using equivalent datasets.
3. Native PostgreSQL schemas created independently, including quoted names,
   native constraints/defaults/identity, mixed schemas and exact data round trips.
4. Wire tests and real PHP, Python, Node, Go and Java clients.
5. Authorization, concurrent DDL, session reuse, malformed input, cancellation,
   resource exhaustion and connection-loss tests.

The native-schema suite is a peer of the differential suite. Success on
proxy-created schemas alone cannot satisfy a release gate.
