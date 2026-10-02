# SQL session input

Status: parser preservation and input component implemented; SQL session
controller, value coercion, native effects and output integration pending.

The inherited parser consumed SESSION/LOCAL/GLOBAL before SET TRANSACTION,
then dropped that scope. It also collapsed direct settings and CHARACTERISTICS
AS TRANSACTION into a boolean. A controller using that AST could apply a
session-default change to next-transaction choices. Rescanning SQL or adding
overlapping flags would leave the source representation ambiguous.

The AST now has distinct Direct, Characteristics and Snapshot variants, each
retaining its keyword scope. It cannot represent a snapshot and a mode list in
the same setting. Formatting preserves the form and scope. This is a breaking
parser API change in an unreleased project, with no compatibility shim.
Preservation is not execution certification for every dialect accepting that
syntax.

## Direct transaction setting intents

`classify_mysql_transaction_setting` accepts Direct settings with one isolation,
one access mode or both. It preserves all four MySQL isolation identities and
returns the existing typed SessionTransaction or NextTransaction assignment.
It does not stage or change state. The staged model still requires submission,
trusted frontend boundary confirmation and validated output.

LOCAL/GLOBAL scopes, Characteristics and Snapshot forms return explicit errors.
LOCAL is a syntactic alias declared by the MySQL grammar, but its controller
integration has not been verified by the current stock corpus. No global
setting implementation is supplied. Empty or repeated characteristics and
non-MySQL isolation choices cannot produce a typed assignment.

The [MySQL scope specification](https://dev.mysql.com/doc/refman/8.4/en/set-transaction.html)
and existing [fourteen stock observations](mysql-transaction-characteristics.md)
ground the session/next distinction. Recognition of Read Uncommitted does not
map it to PostgreSQL Read Committed or establish backend behavior.

## Borrowed variable source facts

`mysql_system_variable_assignments` iterates single or comma-separated variable
assignments in order. Each item borrows the original name and expression and
retains a canonical registry tag, bare/unqualified-@@/qualified form, explicit
keyword scope and preceding keyword scope. No expression is evaluated or
cloned. Unregistered variables and unsupported SET/name shapes return errors;
an iterator ends after its first reported error. Earlier items do not authorize
partially applying a compound statement.

Names now use `SetAssignmentTarget`: ordinary names and explicit MySQL system
variables are distinct. Reads require `Expr::MySqlSystemVariable`. Sigils and
qualifiers are not decoded from ordinary identifier strings. The
[name syntax contract](mysql-system-variable-syntax.md) describes quoting,
formatting, spans and the native reference boundary.

The [MySQL assignment specification](https://dev.mysql.com/doc/refman/8.4/en/set-variable.html)
states that the most recent keyword scope carries forward, while an @@ qualifier
applies to its own name. The component retains both facts independently. For
example, in `SET SESSION sql_mode='', @@transaction_isolation='READ-COMMITTED'`,
the second item retains both its inherited SESSION keyword context and its
unqualified-@@ form. Their combined transaction target is not guessed.
Compound scope resolution, expression order, DEFAULT evaluation, coercion,
warning behavior and atomicity remain controller work.

`classify_mysql_system_variable_read` retains the source expression and its
unqualified or SESSION/LOCAL/GLOBAL qualifier. It does not call a canonical
getter: unqualified read lookup and scope availability require their own
semantics. A quoted column identifier is not reinterpreted as a system-variable
reference. Successful registry recognition does not supply an implemented
value or setting path.

## Verification and costs

`cargo test -p darmok-session --locked` exercises ordinary parsed source forms,
all isolation/access combinations, named-default changes while active,
independent pending choices, borrowed expressions, compound keyword context,
and scoped read identity. Model stages in these tests receive trusted manual
boundaries; no native execution or packet metadata is observed. Parser tests
check scope and source-form preservation across ordinary dialects, and CI
checks all/default features plus minimal/std/visitor feature combinations.
The four existing stock corpora and observer remain unchanged.

Successful classification copies fixed-size tags and borrows AST nodes. The
assignment iterator makes one pass and uses a fixed 31-name registry lookup;
it adds no value cloning, native round trips or locks. The transaction classifier
scans its mode list without allocation. Parser AST construction still allocates
and explicit variable nodes change its layout. End-to-end allocation counts,
cache behavior and performance remain unmeasured. SQL syntax admission remains
separate: this input view does not
certify every construct accepted by the shared generic parser.

M2/M4, catalog validity, transaction equivalence, wire/session integration and
release gates remain incomplete. Security-related work remains deferred.
