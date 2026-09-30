# Compiler boundaries

The compiler foundation parses and emits syntax. It is not an executable SQL
compatibility layer. Catalog binding, routing authorization and semantic
admission remain required before an emitted statement can become a plan.

## Input and parameter identity

Parse one statement at a time using the session's validated MySQL parsing mode
and explicit SQL-byte, parameter-count, parser-recursion, AST-depth and AST-node
limits. Empty input and
batches fail explicitly. Text commands cannot carry prepared placeholders.
MySQL executable comments are currently rejected: the tokenizer preserves
their contents for admission instead of expanding them without a version check.
Ordinary comments and quoted strings retain their normal lexical meaning.

Number client `?` tokens in source order before constructing the AST. This
identity is independent of AST traversal order. Rewriters may reorder,
duplicate or eliminate existing placeholders; they cannot introduce a new
client slot. Emission validates every placeholder identity and builds a dense
PostgreSQL binding map. Full frontend arity is still required when a rewrite
eliminates a parameter.

For example, `WHERE id >= ? LIMIT ?, ?` has frontend slots for the threshold,
offset and count. PostgreSQL receives `WHERE id >= $1 LIMIT $3 OFFSET $2` with
the same values. If a rewrite eliminates slot 1, surviving client slots are
mapped explicitly to dense backend positions. No component rescans emitted SQL
to discover parameters; quoted `$1` text and comments cannot affect arity.

## Emission and identifiers

Identifier conversion quotes object, column, alias and constraint names
throughout the AST, preserving their exact case and contents. Function-name
qualifiers are quoted; valid unquoted function names retain their syntax. A
backtick inside an identifier remains a backtick in the identifier's value. The emitter does not
rewrite serialized SQL to repair missed identifiers.

MySQL identifiers such as `$1` are quoted so PostgreSQL cannot reinterpret them
as parameters. Function-name hooks distinguish special syntax such as
`CURRENT_DATE` from a keyword used as a column name. Parameter validation visits
all raw values, including AST fields that do not carry a source-span wrapper.

MySQL engine, charset and collation table options require semantic lowering;
the emitter rejects residual options instead of silently dropping them. This
does not establish support for these options or for arbitrary PostgreSQL SQL.
The semantic registry must validate every execution path separately.

The owned emission path avoids an additional full AST clone. Binding values are
borrowed rather than copied; binding-map construction is linear in the visited
AST and uses space bounded by the configured parameter count.

## Resource boundaries

Parser call depth does not bound a constructed AST: a flat sequence of additions
or UNIONs builds a deep tree in a loop. The parser checks construction boundaries
and left folds against explicit structural limits. Every field and container
participates in the generated structural traversal; new fields without support
fail to compile. Depth includes containers and metadata nodes, with the root at
one. Node count bounds breadth as well as depth.

The compiler caps parser recursion and structural depth at 128. Complete
resource verification remains pending. Operators may configure tighter limits. Trees are checked
again before cloning or emission, because rewrites may expand them. There is no
unchecked `Clone` operation on `ParsedStatement`. Rejected trees and unchecked
rewrites are destroyed iteratively, including paths that never request emission;
returning an error must not trigger a recursive destructor failure.

The structural check walks fields iteratively with a bounded work list. Parser
construction checks may revisit a subtree, so parsing adds work proportional to
its nodes times the configured depth. This cost and allocation behavior must be
included in the compiler benchmark before release. Iterative disposal allocates
per field on rejected/rewritten trees; admitted unmodified trees use ordinary
bounded-depth destruction. Packet/SQL-byte limits remain separate controls.
Operator defaults belong to the server configuration milestone.

## Evidence and remaining work

Compiler tests cover source-order bindings in CTEs, nested queries, comma LIMIT,
eliminated/duplicated placeholders, malformed rewrite identities, SQL parsing
modes, quoted markers, input limits and redacted diagnostics. Required native
PostgreSQL tests exercise parameter ordering against independently created
tables, including a removed parameter and PostgreSQL's reported bind arity.
An isolated small-stack regression is present for flat expressions, set operations,
nested functions, malformed suffixes, wide valid projections, and deep rewritten
expression/set/type trees with both Box- and Vec-based ownership. It is explicitly
ignored because resource stress work is outside the current user-requested scope.
An incomplete parser-recursion review remains a release blocker; the checks
above do not certify all construction and error-cleanup paths.

These tests do not establish MySQL expression, type, collation, session or
transaction compatibility. A runnable generic engine, catalog coherence,
semantic admission and both end-to-end examples remain M2–M4 work.
