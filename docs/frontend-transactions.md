# Frontend transaction settings

Status: **controller design; not implemented or certified**. The existing
`darmok-session` fields and variable store are extraction scaffolding. This
contract defines their replacement before a frontend controller can use the
[native transaction choices](native-transactions.md).

## Why the current representation cannot be the controller

`SessionState` stores an isolation string separately from a mutable variable
map. Updating one does not update the other. `SessionVariableStore` removes
scope and `@@` prefixes before storing a value, so distinct assignment forms
become the same operation. Neither representation distinguishes the session
default, a one-transaction override or the settings of an active transaction.
There is also no typed access-mode state. The inherited Read Committed default
does not match the pinned stock fixture's Repeatable Read default.

Adding another map entry or deriving active settings from PostgreSQL defaults
would retain these ambiguities. The replacement must have one typed source of
truth with explicit lifetimes. It must also distinguish a frontend transaction
start from a native transaction opened internally for execution and recovery.
An internal BEGIN cannot itself consume a frontend one-transaction override.

The [eleven stock cases](mysql-transaction-characteristics.md) provide finite
reference observations. They do not establish a complete command classifier,
an error matrix or backend equivalence.

## State and representation

The planned controller owns these separate values privately:

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

The intended initial defaults are Repeatable Read, Read Write and autocommit
one, matching the pinned stock fixture. This is a planned replacement of the
inherited default, not a statement about the current library. Runtime startup
must not obtain these choices from ambient PostgreSQL configuration.

Reads of confirmed session defaults must derive from this same state. Do not
keep transaction values in the generic variable map, allow independent public
field mutation or accept a caller-supplied native Ready state as a frontend
completion. Unqualified variable reads, aliases and other read scopes need
their own declared observation and wire metadata before support; the existing
scope normalizer cannot decide their meaning.

## Preserve assignment form until semantic classification

The controller input must retain the original SET form and explicit scope.
The stock corpus and MySQL's
[scope table](https://dev.mysql.com/doc/refman/8.4/en/set-transaction.html)
distinguish these ordinary forms:

| Input | Planned state target |
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
transaction statements. The listed @@SESSION assignment forms have documentation
evidence and still need executable cases.
LOCAL synonyms, DEFAULT expressions, aliases, prepared forms and compound SET
assignments are also separate verification work. Global or persistent forms
must not be silently converted into session updates; their behavior is outside
this controller contract. No global setting implementation is proposed here.

## Reference boundaries and planned transitions

The following table maps the existing finite observations to requirements.
Case names identify entries in
`tests/reference/mysql_transaction_characteristics.json`.

| Reference case | Required transition |
| --- | --- |
| all-four-isolations-and-both-access-modes | Keep isolation/access choices distinct; labels alone do not establish snapshots or write enforcement |
| next-only-controls-one-explicit-transaction | Select the pending pair for the first start; the later ordinary start uses defaults |
| session-update-inside-active-keeps-active-modes | Change future defaults while preserving the active pair |
| per-characteristic-next-updates-compose | The observed access-only next update retains pending isolation; reverse ordering is a planned requirement needing its own case |
| session-update-between-transactions-overrides-only-named-next | A session isolation update supersedes pending isolation while retaining pending access |
| explicit-start-access-overrides-next-and-is-one-only | Explicit START access wins for that transaction; it does not change session defaults |
| next-only-set-inside-active-rejected-and-prior-work-retained | Both observed next-only SET commands fail with 1568/25001, preserving writes, active choices and later valid work |
| implicit-transaction-consumes-next-after-autocommit-off | With autocommit disabled, the observed first table operation uses pending choices; the later transaction restores defaults |
| autocommit-next-consumed-after-transactional-statement | The observed DO, literal SELECT and setting statements preserve pending choices; the first table COUNT consumes them |
| completion-chain-retains-active-characteristics | COMMIT AND CHAIN and ROLLBACK AND CHAIN carry active choices despite changed session defaults; a later ordinary start uses defaults |
| variable-assignment-distinguishes-bare-and-unqualified-at-scope | Preserve the different assignment forms through classification and state updates |

Compose a proposed start per characteristic: explicit supported START choice,
then pending next choice, then session default. This precedence is established
for access by the finite explicit-start case; it is not a license to invent
START isolation syntax. The planned consumption rule removes the pending choices
for that transaction, including an overridden access choice. In the current
explicit-start case, session and pending access are both Read Only, so the later
Read Only transaction cannot prove that the overridden access was cleared.
That rule needs a reference case with distinct session and pending access values.
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
boundaries for those operations; this document does not add an implementation
or expand their supported syntax. Session settings cannot bypass those
controllers by changing a boolean or map entry directly.

## Implementation and verification gate

Replace the inherited string/map/lifecycle duplication as one deliberate
change. Do not add a second model beside it or preserve a string-setter shim.
Derive reads and any relevant translation identity from the authoritative
confirmed/staged state; a cache fingerprint is not a catalog-validity lease.
Typed state should store fixed-size choices rather than allocate isolation
strings on each command. Selection should add no configuration/readback query
to the existing explicit native BEGIN. These are design costs; round trips,
allocations and throughput require measurements on the actual controller.

Before claiming implemented behavior, require:

1. State transitions independently checked against the declared finite
   reference outcomes, including partial updates and later transactions.
2. Ordinary reference cases for newly supported SET/read forms, actual failing
   start/statement boundaries, completion variants and default expressions,
   plus reverse partial-update order and distinct-value overridden-access reset.
3. The actual frontend controller integrated with exclusive native ownership,
   semantic admission, catalog validity, recovery and encoding completion.
4. MySQL wire/driver evidence on PostgreSQL 17/18 over proxy-created and native
   objects, checking settings, errors, warnings, status, data, snapshots,
   read-only SQL behavior and row/schema-lock lifetime separately.
5. Measured controller requests, allocations and contention; no automatic DML
   replay or unsupported-isolation fallback.

The catalog installation choice and
[savepoint-lock blocker #15](https://github.com/duotronic-ai/darmok-proxy/issues/15)
remain open. A pure state model, fixed BEGIN string or stock label observation
cannot close these gates. M2/M4 are incomplete. Security-related work remains
deferred; this contract concerns SQL transaction characteristics only.
