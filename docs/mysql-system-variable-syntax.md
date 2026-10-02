# MySQL system-variable syntax

Status: local parser and borrowed-input implementation. Independent review
found no ordinary functional defect; required CI and merging remain pending.
No session controller or proxy execution is supplied here.

The old MySQL tokenizer embedded `@@` in an ordinary word. An immediate
backtick then entered the unquoted scanner instead of the quoted-name scanner.
An ordinary identifier could not separately preserve the sigil and a name's
delimiter; quoting the combined value would format a column reference.

MySQL now emits a separate `@@` token. `Expr::MySqlSystemVariable` retains an
explicit scope, optional named-instance prefix, name delimiter and sigil span.
Ordinary column identifiers retain their own expression variants. PostgreSQL
operators and other dialect tokenization retain their existing routes.
Semantic equality and hashing exclude locations, while the expression span
covers the sigil through the final name.

`SetAssignmentTarget` distinguishes ordinary object names from explicit MySQL
system-variable targets. Single and comma-separated assignments use this same
type. Tuple assignments in other dialects continue to contain ordinary names.
Existing callers adopt the new types directly; there is no legacy sigil decoder.

## Contextual name grammar

The [nineteen stock observations](mysql-system-variable-names.md) establish the
finite ordinary cases used here. Immediate names after `@@` take the identifier
or backtick route. Names after a qualifier dot take ordinary tokenization:
reads can use quoted text, and `ANSI_QUOTES` changes double quotes into
identifier delimiters. Assignment targets take identifiers. Spaces around the
qualifier dot are retained as accepted syntax. A quoted `@@sql_mode` column is
never converted into a variable reference.

The AST also retains named-instance prefixes and assignment persistence
qualifiers from the pinned release grammar. These do not have native execution
evidence in this fixture. The current canonical session input contract rejects
named-instance targets and persistence scopes explicitly.

Session classification borrows the typed assignment target or read expression.
It keeps registry identity, bare/unqualified/scoped form and both explicit and
preceding keyword scope. It does not scan sigil strings, rescan SQL, select a
value, coerce expressions or resolve compound assignment semantics.

## Checks and costs

`cargo test -p darmok-session --test system_variable_names --test sql_input --locked`
checks the declared forms, distinct quoted columns, pointer identity of borrowed
targets/values, formatter round trips, full spans and immutable/mutable visitors.
The fixture consumer selects grammar flags from the recorded final session
mode; it does not implement mode changes between statements or verify native
effects. Local all/default workspace tests, strict workspace Clippy, formatting,
repository boundaries and minimal/std/visitor parser checks pass. The 22
existing ignored workspace tests remain unrun by those suites; required CI and
native execution gates remain separate.
The observer and all four native corpora remain unchanged.

Unqualified references add a prefix token and no longer put sigil bytes in the
name. Scoped AST nodes avoid a compound-identifier vector and a stored qualifier
string. Explicit target nodes change enum layouts; larger inline variants can
increase the memory used by assignment lists. Classification remains one pass
with borrowed names and expressions, no native requests and no lock changes.
Actual allocation counts, cache hit rates and latency remain unmeasured.

SQL admission, coercion, scope-specific values, native effects, wire output,
catalog coherence, performance and M2/M4 remain incomplete. Security-related
work remains deferred. GitHub Actions is unavailable for the personal account;
the feature remains unmerged until required CI can run.
