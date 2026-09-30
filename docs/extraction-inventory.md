# Extraction inventory

This inventory guides implementation, not advertised compatibility. It records
what to preserve, replace or remove and why. Findings are release work until
resolved by implementation and executable evidence.

## Components

| Component | Decision | Verification boundary |
| --- | --- | --- |
| SQL parser, AST, MySQL modes, emitter, visitors | Vendor with complete grammar tests and Apache notices | Package names, features, AST round trips, supported emission |
| MySQL packet codec and typed values | Preserve and audit | Fragmentation, bounds, binary/text encoding, full auth exchanges |
| Session and prepared statement state | Retain SQL state; remove domain trust/allocation state | Reset/change user, authorization, limits, connection ownership |
| Translation passes and construct registry | Retain generic AST approach; audit every advertised semantic rewrite | Differential results, metadata and explicit unsupported variants |
| Catalog and plan cache | Rework around routed schemas and native metadata | External DDL, dependencies, identity, temporary objects, races |
| Executor, streaming and pools | Retain sound mechanisms; separate generic lifecycle from removed integrations | Transactions, cancellation, pool isolation, resource limits |
| Global ID allocator and branch bootstrap | Exclude | Use ordinary PostgreSQL identities/sequences for local IDs |
| Change-capture/outbox integration | Exclude | DML has no undeclared product-specific side effects |
| Remote control-plane credential resolution | Replace with explicit configured identities and grants | Whole routing/authentication graph, backend grants |
| Metrics and logging | Preserve useful instrumentation; verify actual measured events | Bounded state/cardinality, real cache outcomes, secret redaction |
| SQL compatibility installation | Replace implicit/bootstrap behavior with explicit per-database init | Namespace ownership, versions, idempotence, grants, native data |
| Fixtures/build/release scripts | Self-contained generic replacements | No adjacent repository, image resolver, private service or application fixture needed |

## Risks that must be resolved

1. **Schema assumptions.** Static `public` schema selection and indiscriminate
   identifier lowercasing cannot represent arbitrary native schemas or quoted
   names. Routing must bind exact schemas and catalog identities.
2. **Native type fidelity.** Year-one dates must not become zero dates just
   because a global sentinel convention exists. Type and representation facts
   must be explicit and scoped to the relevant column.
3. **Incidental row order.** Application-driven primary-key ordering for
   unordered joins changes SQL behavior and introduces sorting costs. Remove
   that assumption and use order-aware differential comparisons.
4. **Cache lifetime.** Notification-only invalidation, TTLs, and database-only
   keys do not establish correctness for external DDL, schema aliases, roles,
   transaction-local catalog changes or dropped/recreated objects.
5. **Compatibility declarations.** Registered charset/collation names, helper
   SQL functions and parser constructs are not proof of implemented semantics.
   Inaccurate storage-engine metadata and approximations must not survive as
   advertised support.
6. **Wire capability mismatch.** Capability bits and command handlers must
   agree, particularly around multiple results/statements, cursor modes,
   long-data parameters, resets, errors and session status.
7. **Resource bounds.** Prepared-statement registries and diagnostic state must
   have explicit bounds. Cache metrics must report actual cache results rather
   than maintain an unbounded set of previously seen SQL fingerprints.
8. **Database tests.** Existing test helpers can return success when Docker is
   unavailable. New required suites must fail clearly instead. Ignored upstream
   parser formatting cases are recorded separately from execution support.

## Baseline scope

The pinned starting workspace contains 11 packages and a separate parser fork.
Its workspace test run reported 1,425 passing tests, one failing integration
test and two ignored tests. The failing change-capture integration checks a
schema-change event row count; that subsystem is excluded from Darmok. The
baseline is an extraction reference, not release evidence.

The independent parser import includes its own unit tests, integration tests,
SQL regression fixtures and doctests. Seven upstream pretty-print formatting
tests are explicitly ignored with an upstream issue reference. Their ignored
status must be reported and must never count toward supported proxy behavior.
