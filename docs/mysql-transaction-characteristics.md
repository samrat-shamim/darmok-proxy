# Stock MySQL transaction characteristics

Status: **finite stock reference evidence; frontend policy is not implemented**.
The eleven cases in `tests/reference/mysql_transaction_characteristics.json`
observe transaction characteristics on the pinned MySQL 8.4 fixture. They are
separate from the thirteen [recovery cases](transaction-reference.md).

## What the fixture observes

Session defaults, next-transaction overrides and active transaction settings
have different lifetimes. The fixture reads session defaults with
`@@session.transaction_isolation` and `@@session.transaction_read_only`; it reads
the current thread's transaction event from
[`performance_schema.events_transactions_current`](https://dev.mysql.com/doc/refman/8.4/en/performance-schema-events-transactions-current-table.html).
The join selects the current CONNECTION_ID, rather than another session's event.
Each capture checks isolation, access, state, completion, transaction-event
AUTOCOMMIT, both defaults and the session autocommit variable. The event's state
and AUTOCOMMIT fields are observations, not MySQL packet status flags.

The corpus checks that the transaction instrument and current-event consumer
are enabled on the stock fixture. It changes no global configuration. Missing
instrumentation, a missing event, stale/different values or incomplete results
fail the required observation; they are not skipped or replaced with defaults.
Boolean completion values remain distinct from integer default/count values.

The eleven cases cover:

- All four MySQL isolation labels with both access modes, through explicit
  transaction starts and rollback.
- One next-transaction specification followed by restoration of session defaults.
- Session changes during a transaction: changed defaults, unchanged active modes.
- Independently supplied next isolation/access updates and a session update that
  replaces only the named next characteristic before transaction start.
- START TRANSACTION access overriding the next access for that transaction.
- Both next-only SET forms rejected during an active transaction with error
  1568/25001, while earlier and subsequent writes and active modes remain intact.
- Next choices consumed by implicit transactions with autocommit disabled, and
  by an autocommit table statement after valid nontransactional statements.
- COMMIT AND CHAIN and ROLLBACK AND CHAIN retaining active characteristics
  despite changed session defaults; an ordinary later start uses the defaults.
- Bare variable assignment changing session defaults, while unqualified
  `SET @@transaction_isolation` / `SET @@transaction_read_only` apply to the next
  transaction. See the [SET TRANSACTION scope rules](https://dev.mysql.com/doc/refman/8.4/en/set-transaction.html).

These observations require a frontend policy to distinguish named session
updates, pending next updates and the choices of a confirmed active transaction.
A single isolation string or an ambient PostgreSQL default does not supply that
policy. The [native choices](native-transactions.md) are a separate component;
MySQL READ UNCOMMITTED is not established by PostgreSQL's READ COMMITTED alias.
The planned [frontend controller contract](frontend-transactions.md) maps these
finite observations to settings lifetimes and records the remaining outcome
boundaries. It is not an implemented session policy.

## Running and retaining evidence

Use the existing generic observer with an explicit corpus, selected existing
stock container/image and a new evidence directory. Supply MYSQL_PWD in the
caller's environment as described in [the observer contract](transaction-reference.md):

```sh
python3 tests/reference/observe_mysql_transactions.py \
  --container mysql-reference-84 \
  --image mysql:8.4@sha256:6ea90827b1100f8f2ae306a539f86d2c264a26ed435a2a9f75551dd5c3aeb242 \
  --corpus tests/reference/mysql_transaction_characteristics.json \
  --evidence-dir /tmp/darmok-mysql-transaction-characteristics
```

Each case uses its assigned disposable database and a fresh session. Eleven case
requests plus metadata/setup/cleanup use seventeen SQL clients, two read-only
image inspections and three local metadata commands. There are thirty-one
transaction-event captures within those requests. These are fixture costs, not
proxy round trips or a performance measurement. The observer retains exact SQL,
errors, effects, sources, identity, completion and database-absence receipts.
CI runs the corpus in the stock MySQL job and retains a separate
`mysql-transaction-characteristics` artifact, including failed observations.

The fixture establishes labels, defaults, finite boundaries and declared data
outcomes. It does not execute the proxy or prove isolation snapshots, dirty-read
behavior, read-only write/DDL enforcement, row/schema-lock equivalence, warning
or packet status mapping, prepared execution, catalog validity or release gates.
M2/M4 remain incomplete. Security-related work stays deferred.
