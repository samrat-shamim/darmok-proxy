# Native catalog facts

`darmok-catalog::read_native_relations` reads relation definitions by their
database-local PostgreSQL OIDs. It takes a client or transaction and returns
exact relation names, active user-column definitions and declared types. All
requested relations and each column's domain ancestors are read by one
parameterized catalog query. A missing relation fails the complete operation;
an empty request makes no database call. Repeated OIDs are deduplicated.

This component captures native definitions. It does not establish that a native
type can be translated or encoded for MySQL. Enum, array, range and composite
type identities remain visible rather than becoming string or scalar substitutes.
Domain identities and their immediate base types remain distinct, even though
PostgreSQL's result descriptions report domain base types.

## Facts and their limits

| Fact | Meaning |
| --- | --- |
| Relation/schema OIDs and names | Exact identities in the connected physical database; names retain case and quoting characters |
| Relation kind and persistence | Native table, view, index, sequence or other catalog kind; permanent, unlogged or temporary |
| Attribute number | Native positive user-column number; dropped columns are excluded and leave gaps |
| Declared type OID | The stored declaration, including domains; distinct from a reported result type |
| Type modifier | Raw PostgreSQL type-specific modifier, including meaningful `-1` values; no guessed precision or length |
| Array dimensions | Native declaration; PostgreSQL does not enforce the declaration as the stored value's dimension count |
| Not-null flag | Raw native constraint flag, including potentially unvalidated constraints on PostgreSQL 18; no proof of projected query nullability |
| Expression flag | A column default or generation expression exists; identity and domain defaults remain separate |
| Identity/generation | Native always/by-default identity and stored/virtual generation kinds |
| Collation OID | Native column collation identity; no inferred MySQL collation equivalence |
| Domain descriptor | Immediate base type, its modifier, native not-null flag and declared array dimensions |

The catalog fields follow PostgreSQL's [relation catalog](https://www.postgresql.org/docs/18/catalog-pg-class.html),
[column catalog](https://www.postgresql.org/docs/18/catalog-pg-attribute.html)
and [type catalog](https://www.postgresql.org/docs/18/catalog-pg-type.html).
Unknown relation, persistence, identity, generation or type-kind codes return
explicit errors. Type category codes are retained as native data; user-defined
categories are valid PostgreSQL declarations.

No default/check/index expressions are deparsed here. Constraint validation,
index definitions, domain checks/defaults, system attributes and expression
lineage need their own catalog/semantic contracts. A base column's not-null
constraint cannot become a query's MySQL `NOT_NULL` flag without analyzing
outer joins and other projection behavior. A PostgreSQL schema name cannot
become a logical MySQL database alias without the route contract.

## Freshness and execution

The read is one SQL statement over the native catalogs. It preserves a
transaction's own catalog changes and honors the caller's PostgreSQL isolation
snapshot. Read Committed reads see catalog changes committed before each
statement starts; Repeatable Read can retain older catalog facts until that
transaction ends. "Fresh" means a new query, not a bypass of native snapshot
rules. Those rules follow PostgreSQL's [transaction isolation](https://www.postgresql.org/docs/18/transaction-iso.html).
The component contains no result/plan cache, notification listener or generation token.
Reading through a transaction does not by itself lock all described relations
against subsequent DDL. A caller must establish catalog validity and the owning
connection before reusing facts for an execution plan. OIDs from another
physical database are not interchangeable.

This deliberately leaves reusable catalog-dependent plans disabled. A fresh
read handles ordinary changes between calls, including renamed or recreated
objects and rolled-back DDL; it does not certify concurrent translation and
execution. That gate still needs its chosen locking/validation design and
evidence.

## Cost and verification

All relation IDs are supplied as one typed OID-array parameter. The connector's
[typed query API](https://docs.rs/tokio-postgres/0.7.18/tokio_postgres/struct.Client.html#method.query_typed)
combines preparation and execution in one round trip. The query emits one row
per active column and domain ancestor, or one row for a relation with no user
columns. Output maps hold each relation and declared type once; repeated
domain rows do not duplicate column/name allocations. No table data is read,
and no extra lookup is issued per column. There is no benchmark claim yet.

Required PostgreSQL 17/18 fixtures verify quoted names, native type/domain
identity, column definitions, temporary objects, relation kinds, and fresh
reads after ordinary transactional/external DDL and Repeatable Read snapshot
behavior. MySQL wire metadata and
catalog-dependent plan execution remain separate gates. Security-related work
remains deferred at the user's request.
