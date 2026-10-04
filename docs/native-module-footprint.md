# Private loaded-module observation

Status: private C observation component. Current paired product/probe builds,
ordinary fixtures and independent review are required before merging changes.
It constructs an owned native pathname image. The complete startup/entry,
provider, registry and reference proof remains OPEN. Concurrent PostgreSQL
native two-phase transactions remain required.

The entry base needs an actual loaded-module observation alongside its closed
startup and command history. Configuration strings and matching module names
cannot supply actual loaded state. The existing public native APIs provide a
copying mechanism without opening a relation descriptor; implementing this
instrumentation does not admit any new collector descriptor or execution path.

`module_footprint.h` exposes a private opaque capture beneath an ordinary native
transaction. The common unused-invocation check excludes an active relation
attempt, native refresh, module fences, publication and transaction cleanup.
Normal processing is required. The capture opens no catalog reader, acquires no
relation lock, selects no data snapshot and invokes no module initializer or
registered callback. It does not create a SQL function or frontend command.

## Copy window and lifetime

Both pinned majors expose `EstimateLibraryStateSpace` and
`SerializeLibraryState`. The selected native implementations walk the existing
loaded-file list. The serializer's capacity assertion alone is insufficient
for a production buffer bound. See the paired native
[PG17 implementation](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/fmgr/dfmgr.c)
and [PG18 implementation](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/fmgr/dfmgr.c).

Capture creates its own AllocSet child context and estimates the allocation
capacity before allocating the record and buffer. It then estimates the current
size again. Growth beyond the allocated capacity fails before serialization;
an estimate above 1 MiB fails before allocation. Between the final estimator
and serializer there is no allocation, interrupt processing, loader or callback
call. The normal synchronous backend loader cannot mutate its list in that
copy window. The observer uses the public functions and never reads a private
DynamicFileList layout or restores/loads a library.

The completed representation is checked within the exact final size: nonempty
NUL-terminated pathname records followed by one final NUL, or one NUL for an
empty list. More than 4096 records or an incomplete representation is an error.
Native order and raw bytes are preserved, including bytes that are not valid
server-encoded text. No sorting, deduplication, truncation, normalization or
fallback result is used.

The returned record owns its image in the child context. Accessors borrow that
image and report its exact byte length, pathname count and requested record plus
buffer bytes. The last metric excludes context and allocator overhead. Release
deletes the child context; resetting its parent also expires the record. Callers
must not retain a borrowed pointer beyond either event or release a foreign,
stale or already released C pointer. Native errors delete the observer's child
before propagating. The observer retains no native file-list links or pathname
pointers.

## Meaning, cost and verification

This is an observation of published native pathname entries at one instant.
It is not a module-content hash, source-to-binary certificate, original provider
binding, registry/callback census, initializer history or descriptor-owner
witness. An initializer can have effects before its entry is published. The
[bootstrap construction](native-catalog-bootstrap.md) must still bind those
separate facts and preserve them through each permitted command and cleanup.
The observer does not compare a caller-supplied allowlist or turn pathname
equality into an entry certificate. Existing collector admission gates remain.

Two estimator walks, one serializer walk and one bounded image-validation pass
are linear in module count and pathname bytes. A capture requests one record
plus its first estimated capacity in a fresh AllocSet; it adds no protocol
request itself. No runtime caller or cadence is integrated yet, and there is
no measured hot-path latency, memory, throughput or contention acceptance.

The separate test probe resolves the product's private API before capture,
copies the image to a hex text projection and releases it before receiver work.
It reports scalar snapshot and owner-identity observations, not all references.
Two ordinary fixtures independently decode the full image and require the exact
two-module primary test profile. Repeated observations across savepoint creation
and release retain the image and data-snapshot state. The two owner identities
are checked for preservation within each invocation; they need not be the same
owners across a native subtransaction boundary. Run on each current
product/probe primary:

```text
cargo test -p darmok-postgres-tests --test server_module_footprint --locked -- --nocapture
```

Missing dependencies fail. These fixtures introduce no stress, forced error,
interruption, recovery or profile lifecycle experiment. Current builds, required
existing suites and an independent implementation review are still needed.
Security/authentication/TLS/roles/grants, project compiler work, hosted CI and
release publication remain outside scope. No entry, descriptor, writer,
sequence, table execution, serving, performance or release gate is completed.
