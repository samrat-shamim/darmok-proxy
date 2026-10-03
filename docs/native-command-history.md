# Private command history and separate connection owners

Status: owner separation implemented and locally verified at c5fa88e;
the finite Rust implementation is independently reviewed at1b0fe07.
Native entry and bootstrap admission remain open.

## Boundary and rationale

Exclusive Rust ownership prevents unrelated callers from submitting SQL. It
does not make every command submitted by that owner suitable for catalog-entry
proof. The previous `NativeBackend` also executed setup and verification DO
manifests. Procedural setup may load functions and change persistent native
callback or reference state even after a successful transaction completion.
Its history must not be inherited by a query owner.

`NativeDatabaseSetup` now owns only fresh connection establishment, the fixed
idle lookup initialization, initialization/verification manifests, their checked
commit or failure cleanup, and local disposal. It has distinct setup states,
errors and disposal receipts. `NativeBackend` owns fixed transaction controls,
borrowed scopes and catalog discovery; it has no setup operations or states.
Neither type exposes a client, arbitrary SQL, client adoption, conversion or
connection transfer. Shared `NativeConnection` is a private client/driver and
disposal mechanism, not a public owner or a compatibility wrapper. Public
compile-fail examples cover the absent setup/query methods and conversion.

The intended sequence is setup connect, initialize or verify, await disposal,
then query connect. The API cannot coordinate independent connections or prove
that another client has completed setup. A functional setup receipt also does
not prove later catalog validity. Query connection establishment still requires
its own native entry proof before table execution can be admitted.

The fixed SQL manifest, installed objects and connector arguments are unchanged.
Both owners retain the existing fixed initialization control, exact event
checking and disposal behavior. No authentication, TLS or credential behavior
is added. This costs a second connection/startup at the setup-to-query boundary;
there is no additional per-statement round trip, allocation or cache mechanism.
No measured latency or memory acceptance is inferred.

Current verification at `c5fa88ef8d9d8661202c8f6901e918e7c3233436` passes
formatting, workspace Clippy with all targets/features and warnings denied,
repository checks and offline workspace tests. Offline results include the
three new public API compile-fail examples. Required local PG17.11/18.6 runs
each pass 77 native-owner fixtures, ten BigDecimal transaction fixtures, ten
BigDecimal frontend fixtures and four actual CLI process fixtures, with no
failures or ignored tests in those required runs. These are suite invocations,
including feature variants, not 101 distinct behavioral cases per major.

The new ordinary owner fixture initializes and verifies a disposable database,
creates setup-only temporary/session state through private fixture access,
awaits setup disposal, connects a fresh query owner and observes that the setup
state is absent while committed compatibility functions remain available.
Fixture SQL is not a public query execution API or native admission proof.
Read-only before/after audits observe identical identities, start times, restart
counts, settings and native library/header bytes on all eight existing profiles,
with no residual fixture databases. All 22 native inputs remain unchanged;
there is no new native package build. The finite implementation is independently
accepted; complete native admission remains required.

## Targeted native source findings

Pinned PostgreSQL 17.11/18.6 source separates initial session construction from
normal command processing. `InitPostgres`'s selected tail initializes search
path, client encoding and session storage, optionally loads session modules,
updates startup statistics and completes its startup transaction. The selected
`PostgresMain` call supplies the session-library flag for ordinary backends.
Its later command-loop error boundary and initial readiness do not certify that
all earlier callbacks or module loads were pure. `InitializeSession` itself
allocates zeroed storage in `TopMemoryContext`; the remainder of the session
subsystem was not admitted by that selected function read.

`CommitTransaction` runs deferred-trigger/portal work and transaction callbacks
before resource-owner release. Relcache/invalidation cleanup, lock release,
GUC/SPI/namespace/snapshot cleanup and owner deletion have distinct phases.
`AbortTransaction` and `CleanupTransaction` likewise have separate cleanup and
state transitions. Successful commit, rollback or ReadyForQuery alone therefore
does not prove an empty callback/registry/reference history. PG18 adds AIO/type
cache cleanup and changes the command-loop error context relative to PG17.
These are source findings, not new native error or recovery experiments.

The captured primary source set is eight bodies across both majors. Selected
evidence contains sixteen complete function bodies and eight explicitly bounded
startup spans, not complete `InitPostgres` or `PostgresMain` acceptance. Four
paired differences are retained; eight selected pairs have identical bytes.
The primary v2, selected-functions and paired-differences facts SHA256 values are:

```text
8efdc2fcd84c24670ffb58e12f3acb512e3539348d060374f864c3d5fbf2683e
572b326ea83773d2a697e08147dbb4eea20eca54a354cb8dc8527f6535f95d96
000f1cd8ab0d84c4610e34afc9088ee814fc6b8aeddfb0686480c15536ab6093
```

Original source paths are `src/backend/utils/init/postinit.c`,
`src/backend/tcop/postgres.c`, `src/backend/access/transam/xact.c` and
`src/backend/access/common/session.c` at the official `REL_17_11` and
`REL_18_6` tags. The finite captures preserve exact source bytes and spans.
They do not certify complete startup, artifact identity or runtime behavior.

## Remaining gates

Typed owner separation closes only the setup-to-query connection-transfer path.
A concrete native entered-cache/reference witness, actual module/callback and
registry/provider footprint, writer coverage, guard sequence and bootstrap
admission remain required. Concurrent PostgreSQL native two-phase transactions
remain a v0.1 requirement. No C admission changes follow from this boundary
alone. Whole-statement execution, serving and release gates remain open.
