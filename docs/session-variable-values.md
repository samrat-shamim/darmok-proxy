# Canonical variable values and SQL modes

Status: implemented model component, not a SQL variable handler or compatibility
gate. A separate [SQL input component](session-sql-input.md) preserves assignment
and read syntax facts. Full SQL classification, assignment coercion, warning
policy, backend effects, scope-specific reads, output and reset integration
remain pending.

## One value authority

The extracted generic map could accept `sql_mode` while the public field used
by translation retained a different value. It could also overwrite version
metadata or store values for settings whose effects were absent. Synchronizing
the map and fields would leave two authorities and more mutation paths.

`SessionState` now owns one private `SqlModes` value. `sql_modes()`, canonical
`sql_mode` reads, the parser projection and `TranslationFingerprint.sql_modes`
all derive from it. The generic insertion API and public SQL-mode string are
removed. `set_sql_modes` accepts only the closed typed value and checks that
transaction state is settled. It changes the local model only: it is not a
receipt for a frontend SET, PostgreSQL effect, warning or validated output.

The text/collation/time-zone fields are private, immutable initial model values.
Reads and translation identity share their source. Their names are labels, not
proof of negotiated codecs or MySQL comparison/sort behavior. Changing these
values, including result-charset NULL/no-conversion behavior, requires an
implemented setting and encoding boundary; there is no arbitrary-value map
that can pretend to implement it.

The warning stack is private. Canonical warning/error counts and the clamped
wire warning count derive from that stack; an independent mutable counter is
removed. Clearing or appending diagnostics cannot leave a second stored count
behind. Frontend statement diagnostic lifetime and SHOW WARNINGS remain pending.

## Closed variable registry

`SessionVariableReader` reads a `SessionVariable`, or resolves an internal
canonical name through `get_system_var`. Successful reads return a value;
unimplemented reads return ValueNotImplemented, rather than absent entries or
guessed defaults.

| Value source | Implemented model reads | Mutation path |
| --- | --- | --- |
| Typed transaction state | autocommit, transaction_isolation, transaction_read_only, completion_type | Existing staged transaction command API |
| Full SQL-mode set | sql_mode | `set_sql_modes` with a validated `SqlModes` |
| Immutable initial text settings | time_zone, character_set_client/connection/results, collation_connection | Unimplemented |
| Build/profile identity | version, version_comment, version_compile_os | Read-only |
| Statement diagnostic stack | warning_count, error_count | Read-only variable values; explicit diagnostic APIs |
| Other declared names | No implemented value | Explicit error |

`version_compile_os` uses the compiled target's OS rather than an unconditional
Linux label. Profile version text identifies Darmok and is not a certified
MySQL server implementation.

`write_path` identifies the two typed mutation APIs or returns ReadOnly /
SettingNotImplemented. The latter does not imply that an available immutable
read is missing. It is not SQL dispatch: a controller must first distinguish
global/session/unqualified reads, SET forms and wrong scopes, then establish
value coercion and statement semantics. For example, a canonical version value
does not implement `SELECT @@SESSION.version` or its scope error. The input
component retains these forms without claiming a value implementation. The
registry rejects names containing `@@` or a dot rather than erasing their SQL scope.
Legacy `storage_engine`, `interactive_wait_timeout` and `tx_isolation` names
are not invented as aliases. `interactive_timeout` is declared but unimplemented.

Packet limits, timeouts, engines, foreign-key toggles, database charset facts
and session tracking cannot be inferred from constants in the old profile.
Their canonical reads and setting paths remain explicit errors here. No
authentication, authorization, timeout policy or resource-limit work is added.

## SQL-mode representation

`SqlModes` is a private four-byte identity over the twenty-one public mode names
in the MySQL 8.4 reference vocabulary. It exposes no raw numeric-mask constructor.
`SqlMode::ALL` defines canonical name order. Case and duplicates do not create
distinct values. Composite ANSI and TRADITIONAL names remain present and expand
to their constituent modes; ANSI includes IGNORE_SPACE and ONLY_FULL_GROUP_BY.
The initial set includes all six stock MySQL 8.4 defaults.

`parse_names` handles a names-only value. It trims ASCII spaces only at the end
of the whole value, ignores empty comma elements and preserves spaces inside
individual names. Unknown, obsolete and internal names return an error without
producing a partial value. Numeric SQL expressions, DEFAULT evaluation, other
value types and SQL assignment warnings are separate obligations. Recognition
of a mode, including deprecated PAD_CHAR_TO_FULL_LENGTH, does not implement its
validation, coercion, grouping, temporal or comparison behavior.

The parser consumes only syntactic mode flags through `parser_flags()`. The full
mode value remains in translation identity: STRICT_TRANS_TABLES and
STRICT_ALL_TABLES can have the same parser projection and different identities.
The inherited parser string helper and its dialect/Parser wrappers are removed;
they discarded unknown names, misinterpreted NO_FIELD_OPTIONS and omitted part
of ANSI. The standalone parser still accepts explicit grammar flags, without a
dependency on the session library. There is no replacement string-coercion shim.

MySQL names and mode facts are grounded in the
[8.4 mode specification](https://dev.mysql.com/doc/refman/8.4/en/sql-mode.html)
and stock observations below. External pinned MySQL implementation sources are
retained outside the distribution for reference; no GPL implementation is
imported into this Apache-2.0 project.

## Verification boundaries and costs

The [stock corpus](../tests/reference/mysql_sql_modes.json) declares fourteen
ordinary cases. The unchanged observer checks the six defaults, canonical
ordering, all public names, composite expansion, named-value errors and their
retained prior values, element/whole-value whitespace, empty elements, and a
mode update surviving rollback without removing a prior savepoint. That case
also reads an unchanged global mode default. It uses an assigned disposable
database and preserves exact request/error/effect and cleanup receipts.

CI requires this third stock corpus independently of the two unchanged
transaction corpora, and uploads `mysql-sql-mode-values`. Run it using an
explicit existing stock fixture and caller-provided `MYSQL_PWD`:

```sh
python3 tests/reference/observe_mysql_transactions.py \
  --container "$MYSQL_CONTAINER" --image "$MYSQL_IMAGE" \
  --corpus tests/reference/mysql_sql_modes.json \
  --evidence-dir "$MODE_EVIDENCE_DIR"
cargo test -p darmok-session --locked
```

Manual Rust actions compare fourteen selected mode-string projections with the
corpus. They exercise typed name validation and the local value's read/parser/
fingerprint agreement, not a SQL interpreter or executor. Other tests cover
ordinary quote/literal parser projection, distinct validation-mode identities,
transaction choice preservation, unconfirmed-state refusal, derived diagnostic
counts and explicit unavailable/read-only dispositions. Compile-fail examples
exercise removal of generic insertion and duplicated mutable fields.

The mode identity is a fixed-size copy/hash instead of a cloned SQL-mode string.
Canonical name lookup avoids allocating a lowercased successful key. Name
materialization reserves String capacity upfront; a boxed read value may also
shrink that allocation. Parsing scans a fixed vocabulary.
Other fingerprint strings still allocate, and canonical transaction labels are
still materialized as strings. No native requests or locks are added. Actual
cache hit rates, end-to-end allocation counts, latency and throughput remain
unmeasured.

These are component and stock-reference checks. SQL-level mode acceptance,
full warning/error/status/wire equivalence, native query/write semantics, catalog
validity, prepared execution, reset behavior and M2/M4 remain incomplete.
