# Stock system-variable name reference

`tests/reference/mysql_system_variable_names.json` declares nineteen ordinary
MySQL 8.4 cases for system-variable name syntax. The required CI job runs the
unchanged stock observer with the pinned reference image and preserves a
separate `mysql-system-variable-names` artifact. This fixture is a prerequisite
for the parser correction in [issue #25](https://github.com/samrat-shamim/darmok-proxy/issues/25).
It does not execute the proxy.

Each case resets the session mode, performs its declared setting statements,
and returns one exact JSON effect. The cases cover unqualified and scoped
backtick names, bare/SESSION/LOCAL assignment keywords, quoted-text reads,
scoped double quotes with and without `ANSI_QUOTES`, ordinary spaces around the
qualifier dot, case-insensitive names and quoted columns whose names
look like system variables. `sql_mode` supplies a visible value without changing
global state. One scoped read also verifies that the global value remains at
the reference default.

All nineteen cases declare an empty error list. The observer requires every
submitted statement to complete without unexpected stderr and checks the exact
final value. Column cases return both the variable value and a distinct literal
column value, so a successful variable lookup cannot stand in for column lookup.

There are twenty-five SQL clients: server metadata, initial absence, creation,
setup, nineteen cases, drop and final absence. Two read-only container/image
inspections and three Git/tool metadata processes make thirty captured children.
Twenty-five unique final markers associate SQL completion with each process;
thirty-three artifact files preserve raw commands, SQL, stdout/stderr, source,
corpus and results. The assigned disposable database must be absent after
cleanup. Existing recovery, transaction-characteristic and mode-value corpora
remain unchanged.

The initial twenty-four-case draft completed eight backtick cases, then failed
on an unqualified single-quoted read with error 1064. Its request never reached
the final marker; cleanup was confirmed. The release lexer gives immediate
names after `@@` a different route from qualified names after the dot, so grammar
productions alone cannot establish accepted quoting. The accepted-name matrix
was corrected before further execution. The failing source and receipts remain
preserved separately; this is not a successful twenty-four-case observation.
Immediate text/double-quote/space forms are not advertised as supported, and
the other five removed forms have source-based inference rather than native
runtime results. Scoped quoted-text names remain in the required corpus.

This is finite stock reference evidence for the declared names and effects.
The [parser syntax component](mysql-system-variable-syntax.md) consumes this
reference separately; the fixture itself supplies no AST, classifier,
coercion, query/write mode semantics, native execution, wire output, SQL admission,
catalog, performance or M2/M4 release gate is implemented by this fixture.
Security-related work remains deferred. Exact revisions, commands and executed
results are retained with the component's CI and review receipts outside the
distribution.

The [MySQL variable-assignment specification](https://dev.mysql.com/doc/refman/8.4/en/set-variable.html)
provides the broader scope contract. Quoted-name behavior is established by the
declared stock cases; grammar-source evidence alone is not a runtime result.
