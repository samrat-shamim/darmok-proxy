# Frontend transaction settings

Status: **typed setting/staging component implemented; frontend controller and
transaction equivalence pending**. `darmok-session` owns typed transaction
choices and ordered command stages. Its separate
[SQL input component](session-sql-input.md) classifies direct transaction settings
and retains variable syntax facts.
Neither component executes SQL, validates native receipts, encodes output or
maps frontend choices to the [native transaction choices](native-transactions.md).

## Why the inherited representation was replaced

The extracted `SessionState` stored an isolation string separately from a mutable
variable map. Updating one did not update the other. Its variable normalizer
removed scope and `@@` prefixes, erasing assignment distinctions. It had neither
typed access choices nor separate session, next and active characteristics.
The inherited Read Committed default also differed from the pinned stock
fixture's Repeatable Read default.

Adding another map entry or deriving active settings from PostgreSQL defaults
would retain these ambiguities. The replacement uses one private typed source
of truth with explicit lifetimes, removing the old public string, autocommit
boolean, lifecycle enum and transaction map entries. It distinguishes a
frontend start from a native transaction opened internally for execution and
recovery. Native BEGIN and ReadyForQuery are not inputs to this component and
cannot themselves consume a frontend next override.

The [fourteen stock cases](mysql-transaction-characteristics.md) provide finite
reference observations. They do not establish a complete command classifier,
an error matrix or backend equivalence.

## State and representation

The state component owns these separate values privately:

| Value | Meaning | Lifetime |
| --- | --- | --- |
| Session defaults | Complete isolation/access pair for ordinary subsequent transactions | Until a successful named session update |
| Pending next choices | Independently optional isolation/access overrides | Until their verified consuming boundary or a named session update supersedes them |
| Active choices | Complete pair selected for the current frontend transaction | Until a confirmed frontend end; chaining carries it forward |
| Autocommit setting | Configured frontend behavior | Until its specified successful setting boundary |
| Pending command outcome | Staged effects and the current confirmed or unconfirmed execution phase | Until that command's outcome is accounted for |

Autocommit one does not mean there is no active transaction: an explicit START
can be active with the setting still one. Native ReadyForQuery likewise does
not determine whether a frontend transaction is active. Client savepoints,
native internal savepoints and their recovery outcomes remain separate under
the [recovery contract](transaction-recovery.md).

Use closed isolation/access values rather than arbitrary strings or generic
`Value` entries. Each characteristic has its own optional next override; absence
means use its current session default. Preserve the MySQL Read Uncommitted
identity when classifying input. It must never become a native Read Committed
alias. A recognized frontend label is not an admitted backend behavior.

The library's initial defaults are Repeatable Read, Read Write and autocommit
one, matching the pinned stock fixture. Runtime startup must not obtain these
choices from ambient PostgreSQL configuration.

`transaction_settings()` returns an immutable copy of settled choices.
Canonical internal reads of `transaction_isolation`, `transaction_read_only`
and `autocommit` derive from that same state. The generic setter and variable
map have been removed. Other canonical values follow the
[closed variable contract](session-variable-values.md).
`SessionState` cannot be cloned to escape a borrowed command stage.

The canonical variable interface accepts internal names, not SQL expressions:
it rejects scoped references and the old `tx_isolation` alias. Its isolation
value is a string and its access/autocommit values are internal unsigned zero/
one values. These representations do not certify MySQL result types, signedness
or packet metadata. The SQL input view retains unqualified and scoped reference
forms without selecting their values. Scope-specific lookup, aliases,
observations and wire evidence remain separate work. Nontransaction variable
values do not constitute an implemented SQL session controller.

## Preserve assignment form until semantic classification

The controller input must retain the original SET form and explicit scope.
The stock corpus and MySQL's
[scope table](https://dev.mysql.com/doc/refman/8.4/en/set-transaction.html)
distinguish these ordinary forms:

| Input | Typed target after classification |
| --- | --- |
| SET SESSION TRANSACTION ... | Named session defaults |
| SET TRANSACTION ... | Named next-transaction choices |
| SET SESSION transaction_isolation = ... | Session isolation default |
| SET @@SESSION.transaction_isolation = ... | Session isolation default |
| SET transaction_isolation = ... | Session isolation default |
| SET @@transaction_isolation = ... | Next isolation choice |
| Corresponding transaction_read_only assignments | Corresponding access choice, preserving the same form distinction |

This table is not a supported syntax registry. The corpus exercises explicit
SESSION variable assignments, bare assignments, unqualified-@@ assignments and
transaction statements. A further case exercises both listed @@SESSION
assignment forms, named pending-access replacement between transactions and
changed defaults with unchanged active choices.
The parser retains LOCAL syntax, and the direct command classifier returns an
explicit unimplemented-scope error for it. LOCAL controller integration,
DEFAULT expressions, aliases, prepared forms and compound SET semantics remain
separate verification work. The input view retains compound keyword context and
per-name qualifiers independently; it does not apply their values. Global or
persistent forms must not be silently converted into session updates; their
behavior is outside this controller contract. No global setting implementation
is proposed here.

The separate [stock SET semantics fixture](mysql-set-semantics.md) supplies
selected LOCAL, compound-assignment and DEFAULT failure observations for the
controller's requirements. It does not change the classifier's implemented
forms or supply native/output integration.

## Reference boundaries and state transitions

The following table maps the existing finite observations to requirements.
The Rust logical traces check the setting pair and defaults only; the data,
error, event and wire requirements listed here remain distinct gates.
Case names identify entries in
`tests/reference/mysql_transaction_characteristics.json`.

| Reference case | Required transition |
| --- | --- |
| all-four-isolations-and-both-access-modes | Keep isolation/access choices distinct; labels alone do not establish snapshots or write enforcement |
| next-only-controls-one-explicit-transaction | Select the pending pair for the first start; the later ordinary start uses defaults |
| session-update-inside-active-keeps-active-modes | Change future defaults while preserving the active pair |
| per-characteristic-next-updates-compose | The observed access-only next update retains pending isolation |
| session-update-between-transactions-overrides-only-named-next | A session isolation update supersedes pending isolation while retaining pending access |
| explicit-start-access-overrides-next-and-is-one-only | Explicit START access wins for that transaction; it does not change session defaults |
| next-only-set-inside-active-rejected-and-prior-work-retained | Both observed next-only SET commands fail with 1568/25001, preserving writes, active choices and later valid work |
| implicit-transaction-consumes-next-after-autocommit-off | With autocommit disabled, the observed first table operation uses pending choices; the later transaction restores defaults |
| autocommit-next-consumed-after-transactional-statement | The observed DO, literal SELECT and setting statements preserve pending choices; the first table COUNT consumes them |
| completion-chain-retains-active-characteristics | COMMIT AND CHAIN and ROLLBACK AND CHAIN carry active choices despite changed session defaults; a later ordinary start uses defaults |
| variable-assignment-distinguishes-bare-and-unqualified-at-scope | Preserve the different assignment forms through classification and state updates |
| reverse-next-updates-preserve-pending-access | The later isolation-only update retains pending access; a subsequent transaction returns to defaults |
| explicit-access-discards-overridden-next-with-distinct-defaults | In both observed access directions, explicit START overrides the pending access, then the later transaction uses the distinct session access |
| qualified-session-assignments-update-named-defaults-not-active | A named @@SESSION access update supersedes pending access while retaining pending isolation; active choices stay unchanged after default updates and two later starts use those defaults |

Compose a proposed start per characteristic: explicit supported START choice,
then pending next choice, then session default. This precedence is established
for access by the finite explicit-start case; it is not a license to invent
START isolation syntax. Consumption removes the pending choices for that
transaction, including an overridden access choice. The original explicit-start
case has equal session/pending access, so it does not distinguish clearing from
retention. The distinct-defaults case now checks both override directions and
the later restoration of session access. This establishes those finite
transitions, not all command or failure variants.
Do not reset the entire session pair when only one named default is updated.

Chaining derives its new pair from the preceding active pair, as documented
for [COMMIT and ROLLBACK](https://dev.mysql.com/doc/refman/8.4/en/commit.html).
A bare COMMIT/ROLLBACK and completion_type settings are distinct syntax/policy
cases; the corpus uses explicit AND CHAIN and AND NO CHAIN. RELEASE variants
and requests without an active transaction remain unverified.

Transaction starts must be classified from the admitted command's semantics,
including transactional object participation. An arbitrary SELECT can call a
function or perform writes; an internal recovery transaction for a literal
result is a different boundary. Do not consume pending choices by first keyword,
by opening a PostgreSQL guard transaction or by observing a native Transaction
state. The corpus's nontransactional statements are finite examples, not a
general classification algorithm. Admission and complete
[catalog validity](native-execution.md) remain prerequisites for that algorithm.

## Completion and failure belong to the boundary

A proposed pair may be selected without publishing a completed frontend start.
The command controller must retain confirmed settings and staged changes
through the relevant native operation and frontend output validation. Native
controls use a complete `NativeTransactionSpec` for a new transaction;
savepoint scopes preserve the existing native pair. Neither operation supplies
a frontend acknowledgement or a semantic mapping automatically.

The planned controller must account for the observable frontend start/end
boundary separately from successful statement completion. A statement failure
can follow a real transaction start. Blindly undoing every setting transition
because the SQL returned an error, or publishing every transition before
submission, would both be incorrect strategies. The current corpus does not
establish pending-choice consumption for failed starts, missing objects,
read-only DML errors or output failures. Those paths need ordinary reference
and integrated outcome evidence before their transitions are implemented.

Preserve the [recovery categories](transaction-recovery.md): known statement
failure, confirmed whole rollback, implicit commit and unconfirmed outcome
have different frontend consequences. A rejected next-only SET within an
active transaction cannot mark the whole transaction failed or discard prior
writes. Local native recovery does not imply a frontend transaction end.
Unknown outcomes cannot restore a reusable controller or invent active/default
state. Original errors and cleanup failures must remain distinguishable.

START while already active, autocommit transitions and supported DDL need their
own ordered command controllers because preceding work may commit. The thirteen
[recovery cases](transaction-reference.md) establish finite successful/error
boundaries for those operations. The state component can stage an active START
as commit then start, and an active autocommit-zero to one change as commit then
assignment. It does not perform or validate those commits. DDL is not represented
by its typed commands. Session settings cannot bypass the eventual controllers
by changing a boolean or map entry directly.

## Borrowed command stages and known partial outcomes

`stage_transaction_command()` selects at most two ordered frontend boundaries
without applying them. The guard exclusively borrows the state. Dropping it
before submission leaves confirmed choices, pending overrides and translation
identity unchanged. `mark_submitted()` must be called once before any operation
can produce command effects; it does not submit a native request.

`record_confirmed_frontend_boundary()` applies only the next staged boundary.
A known start consumes pending choices and publishes the selected active pair
inside the retained history. A known end clears active choices. Each known
boundary remains recorded if a later boundary or output is unconfirmed; a known
preceding commit cannot be undone by discarding the proposed following start.
Unexpected, premature or repeated phase reports are retained and cannot be
overridden by later reports. Matching all boundaries still requires
`finish_success_with_validated_output()` before the settled snapshot, canonical
variable interface or translation fingerprint becomes available again.

These receipt and success methods assert a trusted caller contract. They do not
prove native completion, semantic admission or encoding. Dropping a submitted
guard or failing settlement retains `UnconfirmedTransactionCommand`, including
the before/last-confirmed states, remaining boundary and phase error. There is
no reset/reuse API. A future controller must dispose of an unconfirmed native/
frontend session; an error packet alone cannot settle this success-only stage.
Statement failures and confirmed recovery need their own verified outcome path.

The initial command vocabulary covers named session/next assignments, the four
declared variable forms, explicit and implicit starts, a successful autocommit
statement, active commit/rollback with an explicit chain choice, and the finite
autocommit boundaries above. Commands requiring an absent or wrong frontend
state fail before staging.
Autocommit setting changes during an active transaction whose setting is already
one are deliberately rejected as unverified, including repeated one. Completion
without an active transaction, compound SET, DEFAULT, completion_type, RELEASE,
client savepoints and failed statement transitions are not implemented by this
vocabulary. No SQL syntax is supported merely because it has a typed command.

## Implementation and verification gate

The string/map/lifecycle replacement has no setter shim. The translation
fingerprint includes the complete confirmed transaction snapshot: defaults,
independent pending choices, active pair and autocommit. It returns an error for
unsettled outcomes. The [canonical variable model](session-variable-values.md)
removes generic insertion of NULL or nonstring charset values: text settings
are immutable initial model values and their mutation path is unimplemented.
SQL NULL/no-conversion charset behavior remains pending.
A fingerprint is not a catalog-validity lease or a completed session controller.

Choices, plans and transaction snapshots are fixed-size copies, with no isolation
string allocation, native query or lock added by this component. Canonical name
lookup no longer allocates a successful key; the isolation `Value::String` read still allocates;
the typed snapshot/label API avoids that string materialization. Other fingerprint
strings are still cloned. More state dimensions can create legitimate cache
misses when choices change. Actual hit rates, allocations, requests, latency and
throughput remain unmeasured and require the integrated controller workload.

The required `cargo test -p darmok-session --locked` suite includes fourteen
manual typed traces over the unchanged stock characteristic corpus. They compare
forty-one selected isolation/access pairs and their session defaults/autocommit,
plus derived canonical reads. The successful autocommit trace compares its
completed pair without treating MySQL's retained event as currently active.
These tests supply trusted boundaries in memory; they neither parse nor execute
the corpus SQL and do not check event identity, native receipts, data, SQL error
codes, locks, snapshots or packets. Separate ordinary phase tests cover prepared
abandonment, known partial outcomes and unavailable state after unconfirmed
submission. Stock corpora and observer remain unchanged.

Before claiming frontend controller behavior, require:

1. Independent review of the state projection and actual controller against
   declared finite reference outcomes, including partial updates and later
   transactions; in-memory boundary assertions cannot certify the controller.
2. Ordinary reference cases for newly supported SET/read forms, actual failing
   start/statement boundaries, completion variants and default expressions.
3. The actual frontend controller integrated with exclusive native ownership,
   semantic admission, catalog validity, recovery and encoding completion.
4. MySQL wire/driver evidence on PostgreSQL 17/18 over proxy-created and native
   objects, checking settings, errors, warnings, status, data, snapshots,
   read-only SQL behavior and row/schema-lock lifetime separately.
5. Measured controller requests, allocations and contention; no automatic DML
   replay or unsupported-isolation fallback.

The catalog installation choice and
[savepoint-lock blocker #15](https://github.com/samrat-shamim/darmok-proxy/issues/15)
remain open. A pure state model, fixed BEGIN string or stock label observation
cannot close these gates. M2/M4 are incomplete. Security-related work remains
deferred; this contract concerns SQL transaction characteristics only.
