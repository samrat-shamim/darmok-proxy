# Private live builtin dispatch observation

Status: private C observation component. Current paired product/probe builds,
ordinary fixtures and independent review are required before merging changes.
This checks the selected heap and btree rows in the running backend's public
builtin function table against their linked C symbols. It does not admit a new
descriptor, invoke a provider or establish the complete native entry profile.
Concurrent native PostgreSQL two-phase transactions remain required.

## Why a separate observation is needed

The earlier [executable file observations](native-build-observations.md) describe
the selected heap row and its link-image relocation. A file-relative pointer
does not report the function binding in a running process. Public fmgrtab.h
exposes the native builtin table, count, highest OID and OID-to-index mapping;
the [PG17 header](https://github.com/postgres/postgres/blob/REL_17_11/src/include/utils/fmgrtab.h)
and [PG18 header](https://github.com/postgres/postgres/blob/REL_18_6/src/include/utils/fmgrtab.h)
declare this data without a private-layout cast.

Using fmgr_info, an OID function call or GetIndexAmRoutine for an observation
would take a different native path and could invoke the handler. This component
instead reads the exported data directly and compares two function pointers;
it calls neither heap_tableam_handler nor bthandler. The native declared arity
remains one even though the native AM acquisition paths can call their handlers
with zero arguments. This observer does not emulate either invocation.

## Capture and lifetime

builtin_dispatch.h exports one private capture function with PGDLLEXPORT. It
requires a live caller-provided writable record, normal processing and the same
unused-invocation guard as the [module observer](native-module-footprint.md).
Active relation attempts, refresh, module fences, publication, parallel/historic
execution and transaction cleanup remain excluded by that existing guard.

Capture first checks that the native builtin count fits the public uint16 index
representation. For each of OIDs 3 and 330 it checks the mapping range, missing
mapping sentinel and table index before reading the selected row. It requires
the exact OID, declared one-argument signature, strict/non-set flags, exact
bounded NUL-terminated C name and equality with the corresponding linked symbol.
Compile-time checks retain the generated handler OIDs. Unknown rows fail with
an explicit prerequisite-state error; no lookup-by-name or alternate provider
is substituted.

A zero-initialized local record receives both rows. Only after both checks
succeed is the complete record copied to the caller. Inline name arrays contain
copied bytes; no function, string, table or descriptor pointer escapes. There
is no heap allocation, loader, interrupt processing, callback or handler call
in this observation. The result has the lifetime of the caller's own storage;
it has no observer context or release operation.

The public table and linked symbol definitions are inputs from the bound native
build. Equality establishes the relationship between those live inputs at the
observation, not original executable contents, complete source-to-binary
derivation or a provider's later execution path. The component does not inspect
actual FmgrInfo values, selected pg_am rows, AM routine contents, fmgr hooks,
callback/registry state or entered reference owners. Those separate
[bootstrap obligations](native-catalog-bootstrap.md) remain open. This is not
an allowlist or a certificate derived merely from matching symbol names.

## Cost and ordinary verification

The product does two indexed lookups, fixed scalar comparisons, two bounded
name comparisons and a fixed-size stack copy. It adds no protocol request
itself and traverses no entire builtin table. It retains no native references,
selects no data snapshot and allocates no heap storage. Stack bytes are not a
latency, process-memory, throughput or contention measurement. Runtime caller
cadence and complete performance acceptance remain open.

The test-only probe resolves the exported capture before observing it. Its
JSON projection contains scalars and inline names, not native addresses. Two
ordinary fixtures require the exact TEXT description, one row, SHOW completion
and transaction readiness; they check the independently recorded release
builtin counts, OIDs, signatures and names. Unselected and selected RR data
views and both owner identities are preserved within each invocation. Repeated
captures across savepoint creation and release agree; owner identity equality
across different subtransactions is not required.

On each current PostgreSQL 17.11/18.6 product/probe primary profile run:

    cargo test -p darmok-postgres-tests --test server_builtin_dispatch --locked -- --nocapture

Dependencies are required, not skipped. The fixtures introduce no new forced
error, stress, interruption, recovery or profile lifecycle experiment. Changed
native packages require fresh builds and required existing suites. Complete
entry/startup/provider/registry/reference/writer/sequence/descriptor/table
execution, serving, performance and release gates remain open.
