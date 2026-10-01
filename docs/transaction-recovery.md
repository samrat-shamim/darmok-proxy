# Frontend transaction recovery contract

Status: **design and stock reference observations; no frontend controller or
transaction compatibility gate is implemented**. The
[native owner](native-backend.md) supplies confirmed control mechanisms. The
controller must choose recovery from the admitted statement's semantic contract,
preserve the original error separately, and settle frontend state only against
the confirmed outcome. PostgreSQL SQLSTATE or readiness alone cannot supply that
contract.

## Recovery categories

| Semantic category | Required effect | Native mechanism available | Remaining obligations |
| --- | --- | --- | --- |
| Unsupported before effects | No unintended effects or session changes | Admission is pending | Reject before effectful analysis, conversion, planning or execution |
| Statement-local failure in autocommit | Undo the owned statement transaction | Statement recovery performs ROLLBACK/idle | Original error, nontransactional effects, output and frontend state policy |
| Statement-local failure in an explicit transaction | Undo all statement steps, preserving earlier work and required locks | Statement recovery performs ROLLBACK TO plus RELEASE/transaction | MySQL lock and snapshot effects are not established |
| Whole-transaction failure | Undo earlier and current work; remove all savepoints | Transaction recovery performs one ROLLBACK/idle from either boundary | Error classification and frontend state/savepoint updates |
| Implicit commit boundary | Commit preceding work at the specified frontend boundary | Explicit native controls exist | A separate admitted controller for each supported transaction/DDL/session command |
| Unconfirmed finish or cleanup | Retain the known/unknown outcome and original error; no reusable owner | Disposal only | Frontend error and state disposition; no replay or success assumption |

There is no default recovery category. Native statement recovery inside a
savepoint must not automatically escalate to whole rollback on cleanup failure:
that could discard earlier work which MySQL would preserve. The owner retains
the cleanup error and becomes uncertain. Native whole recovery is requested
explicitly and confirmed independently of any internal savepoint's existence.

Stock MySQL 8.4 observations distinguish statement errors from full rollback.
The recorded duplicate-key, CHECK, NOT NULL, missing-column and multi-row
duplicate cases preserve earlier transaction writes and permit later valid
writes; multi-row failure undoes its complete statement. The ordinary InnoDB
deadlock observation removes the victim's earlier write and savepoint, then
permits a new autocommit write when the configured autocommit setting is one.
These observations inform the categories above. They do not implement native
error-to-frontend mapping, messages, warnings or wire status flags, and do not
exhaust the error matrix. Lock-wait timeout policy is unverified.

The reference also distinguishes ordinary DDL, temporary object operations,
autocommit transitions and a new transaction begun while one is already active.
Ordinary CREATE, including the observed existing-table error, commits preceding
work. Temporary CREATE avoids that commit but its object survives rollback;
temporary ALTER commits. Changing autocommit from zero to one commits, while
setting zero again does not; START TRANSACTION commits the active transaction.
Exact supported command variants require their own admitted controller and
evidence. PostgreSQL's transactional DDL cannot inherit these rules implicitly.

## Data recovery is not lock equivalence

Four finite cases were observed independently on stock MySQL 8.4.11 and on
PostgreSQL 17.11/18.6. Each used a private disposable database with two existing
integer rows, Repeatable Read, a persistent holder connection and a second
session's valid FOR UPDATE NOWAIT query. The second session rolled back and
closed normally. Probes before savepoint rollback confirmed the actual conflict;
probes after full rollback confirmed the lock was removed and the value restored.

| Case after ROLLBACK TO and RELEASE | MySQL 8.4.11 | PostgreSQL 17.11 and 18.6 |
| --- | --- | --- |
| Existing-row UPDATE, with earlier InnoDB work before the savepoint | Later row value restored; later lock retained | Later row value restored; later lock released |
| Existing-row locking read, with earlier InnoDB work before the savepoint | Later lock retained | Later lock released |
| A separate row locked/updated before the savepoint | Earlier write and lock retained | Earlier write and lock retained |
| Savepoint before the first table operation; UPDATE follows it | Later write and lock removed | Later write and lock removed |

The context in the last row matters. The first expectation of universal MySQL
retention failed and is preserved as failed evidence. MySQL's SQL layer records
the engines present at savepoint creation and uses full engine rollback for
engines which joined afterward. The
[pinned 8.4.11 source](https://github.com/mysql/mysql-server/blob/99960bf74fa919347e4f4e3ca47672f333d6e91f/sql/handler.cc#L2257)
supports this explanation for the observed first-table-operation case. The source
tag is not an assertion of exact container-build provenance.

For participating InnoDB transactions, the ordinary cases match the documented
[MySQL stored row-lock retention](https://dev.mysql.com/doc/refman/8.4/en/savepoint.html).
PostgreSQL documents
[lock release during savepoint rollback](https://www.postgresql.org/docs/18/explicit-locking.html).
Neither a matching table value nor a confirmed native transaction state proves
the frontend lock contract. The NOWAIT errors are observations of conflict, not
a proposed cross-engine error-code mapping or a timeout-policy test.

[Blocker #15](https://github.com/samrat-shamim/darmok-proxy/issues/15) tracks the
required mechanism and end-to-end proof. Reacquiring locks after rollback leaves
an observable gap. Replacing row locks with broad relation locks changes
contention. Removing native statement scopes loses recovery, while converting
statement errors into full rollback changes earlier-write behavior. These do
not establish the contract and must not become silent approximations. An assisted
catalog-validity module alone is also not a lock-equivalence proof. A deliberate
change to the compatibility contract requires an explicit product decision.

## Controller and execution obligations

Keep the frontend autocommit setting, active transaction, client savepoint
identities and pending session changes separate from native ReadyForQuery state.
Stage their changes at the specified semantic boundary; a successful native
cleanup receipt is not a successful statement. Whole recovery must remove the
frontend transaction/savepoint state only against its confirmed outcome. Local
recovery must retain the earlier state required by its admitted contract.
Unconfirmed outcomes cannot invent either completion or rollback.

The [execution contract](native-execution.md) still requires the scope through
all backend steps, row decoding and frontend encoding. Recover original statement
errors, output failures and later backend errors according to their established
categories, preserving original and cleanup outcomes. Nontransactional native
effects, including sequence advancement, require explicit policy. No automatic
DML replay follows a deadlock, serialization failure or unknown finish outcome.

Before advertising transaction support, the actual proxy must demonstrate the
data, row-lock, schema-lock and snapshot effects for every admitted category on
both PostgreSQL versions, over both proxy-created and independently created
native objects. Include client/internal savepoints, earlier and later work,
errors followed by valid work, whole rollback and output-validation failures.
Verify errors, warnings, frontend state and status flags separately from values.
Measure the chosen mechanism's requests and contention before making performance
claims. Security-related and adversarial/resource work remains deferred.

## Evidence boundaries

The original thirteen MySQL transaction cases and the independent finite deadlock
case are frozen outside the distribution. Their review ties the observed errors
to exact statements; the original thirteen-case runner does not yet enforce that
mapping itself. It cannot become a required compatibility fixture unchanged.

The new lock runner checks each expected CLI error against its exact physical SQL
line, rejects other stderr, requires a unique completion marker, checks values,
and records normal child exits. PostgreSQL uses psql `-f -` for line attribution;
an earlier runner attempt without it is preserved as a format failure. The final
lock run contains twelve stock groups and seventy-two SQL-client invocations,
all exiting zero. The helper separately asserts successful completion of six
read-only Docker image/container inspection calls. All three newly created
databases were removed and absence checked. No failed attempt is relabeled as
passing.

Exact source hashes, commands, errors, results, image identities and cleanup
receipts are external evidence. No actual proxy, driver compatibility, schema-lock
equivalence, complete recovery classification, concurrent-DDL validity, performance
or release gate is certified by these stock observations.
