# Native lookup context

Status: **implemented owner initialization; execution validity remains pending**.

The native owner uses `search_path = pg_catalog`. Logical MySQL database names
and mapped PostgreSQL schemas are explicit route inputs, rather than an ambient
PostgreSQL lookup path. This lets the same owner mechanism serve application
schemas and existing native schemas without inheriting a role's default schema
selection or the connector's startup `search_path` option.

## Construction

On a fresh connection, `NativeBackend::connect` submits the fixed request
`ROLLBACK; SET search_path = pg_catalog`. The control checker requires both tags,
in order, and idle ReadyForQuery before the owner is returned. A partial or
failed initialization leaves the owner uncertain; construction returns an error
and owner Drop requests driver abortion and drops the client. It does not await
driver termination or return a disposal receipt. Idle readiness after ROLLBACK
alone does not certify the lookup setting.

Both commands share the existing initialization request. There is still one
control round trip per connection, with a static SQL string and two checked
tags. There is no per-statement SET, readback, context allocation, cache or
additional lock introduced by this component. This is a cost analysis, not a
latency measurement. Connector configuration and its connection establishment
remain unchanged; the owner sets its lookup context after startup completes.

Transaction start, commit, rollback, scope finish and scope recovery do not
change this setting. No public raw SQL method or caller-selected context setter
is added. The fixed command belongs to owner construction, not a frontend SET
translation or a pool reset API.

## Name binding

Generated native SQL must qualify application relations with the selected
native schema and exact identifier spelling. Functions, types, operators and
collations need their declared native identities as well; their semantic
admission and lowering remain separate work. An unqualified table name cannot
become a logical route by being placed on `search_path`. Permanent DDL also
needs an explicit target schema. Quoting and route resolution are compiler and
DDL-controller responsibilities; this component does not generate their SQL.

PostgreSQL still searches the current temporary namespace implicitly for
relations and types. The configured path string can remain `pg_catalog` while
the effective namespace list changes when a temporary namespace is first
created. Temporary objects need connection identity, actual namespace identity
and an execution-validity rule; the fixed path does not remove them or certify
their later binding. This follows PostgreSQL's
[lookup rules](https://www.postgresql.org/docs/18/runtime-config-client.html#GUC-SEARCH-PATH).

## Prepared descriptions are not binding proof

PostgreSQL can re-analyze a prepared statement when its lookup context changes,
even if its SQL uses a fully qualified application relation. Its
[prepared-statement contract](https://www.postgresql.org/docs/18/sql-prepare.html)
does not promise an immutable name binding. The connector attaches the cached
statement description to returned rows. Matching that description therefore
does not prove which relation supplied the row.

Required PostgreSQL 17/18 fixtures prepare a qualified query, rename its native
schema, and create a same-shaped replacement under the original name. Without
a lookup change the observed statement retains the original relation. An
explicit path change or first temporary namespace creation causes it to read
the replacement while the cached column origin still names the original OID.
The returned `tableoid` establishes the actual source independently of that
cached origin. These are ordinary sequential DDL fixtures, not a concurrent DDL
or complete dependency-validity test.

The initialization makes the configured context predictable. It does not
create a catalog lease, prevent every re-analysis, bind relation names to OIDs,
refresh a portal description, or cover dependencies of views, functions,
operators, collations, domains or hidden writes. Catalog-dependent admission
and reusable catalog plans remain disabled until the complete
[execution-validity contract](native-execution.md) is implemented and verified.

Private owner fixtures additionally check startup path replacement, all six
native transaction modes, savepoint finish/recovery, exact qualified application
names, implicit temporary namespaces and disposal after partial initialization.
Their fixture SQL does not expose or certify a public admitted statement lane.
