# Native entry mechanism decision

Status: concrete alternatives for the remaining catalog-entry mechanism.
Neither alternative below is implemented or accepted as an entry witness.
Stock PostgreSQL with the Darmok extension remains the current deployment
direction. A Darmok-supplied PostgreSQL build is a proposed alternative whose
deployment scope has been raised with the user, not an adopted requirement.
Concurrent native PostgreSQL two-phase transactions remain required.

## The unresolved implementation boundary

The [bootstrap contract](native-catalog-bootstrap.md) requires a defining
profile before a new descriptor can increment a reference or rebuild. Current
pathname and builtin-dispatch observers supply useful inputs to that profile.
They do not establish the entered reference owners, actual options registry,
callback history or provenance of restored descriptor state.

The selected PostgreSQL 17.11/18.6 public headers and their native storage
declarations make these three gaps concrete:

| State needed before entry | Selected native boundary | What the current API establishes |
| --- | --- | --- |
| Existing relcache entries, positive references and intrinsic pins | `RelationIdCache` is static in relcache.c | relcache.h exposes open/close, invalidation and initialization operations; it does not declare a passive entry iterator |
| Exact owner items corresponding to those references | `ResourceOwnerData` is opaque in resowner.h and defined in resowner.c | Public remember/forget/release operations and owner pointers do not enumerate the items |
| Actual global options definitions and referenced data | `relOpts`, custom registrations and initialization state are static in reloptions.c | reloptions.h exposes registration and parsing; it does not declare a passive global-registry census |

These findings are limited to the selected headers and declarations. They are
not an impossibility proof about every PostgreSQL API or a complete mutation
census. Source inputs are the official pinned
[PG17 relcache header](https://github.com/postgres/postgres/blob/REL_17_11/src/include/utils/relcache.h),
[PG18 relcache header](https://github.com/postgres/postgres/blob/REL_18_6/src/include/utils/relcache.h),
[PG17 owner header](https://github.com/postgres/postgres/blob/REL_17_11/src/include/utils/resowner.h),
[PG18 owner header](https://github.com/postgres/postgres/blob/REL_18_6/src/include/utils/resowner.h),
[PG17 options header](https://github.com/postgres/postgres/blob/REL_17_11/src/include/access/reloptions.h)
and [PG18 options header](https://github.com/postgres/postgres/blob/REL_18_6/src/include/access/reloptions.h).
The corresponding private declarations are in paired
[PG17 relcache](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/cache/relcache.c),
[PG18 relcache](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/cache/relcache.c),
[PG17 owners](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/utils/resowner/resowner.c),
[PG18 owners](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/resowner/resowner.c),
[PG17 registry](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/common/reloptions.c)
and [PG18 registry](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/common/reloptions.c).

Opening a descriptor to inspect its count can execute the path that still
needs admission. Parsing an option as a probe can reach the unadmitted registry.
Adding a stronger catalog lock establishes neither backend-private state nor
earlier callback effects. None supplies the missing pre-open boundary.

## Alternative A: construct the profile on stock PostgreSQL

Keep the stock server and project-owned extension, and construct entry state
from a closed startup and command history. The existing separation between
setup and query connections is necessary but is not the complete base.

The next deliverable for this alternative is one explicit construction argument
with a finite supported deployment profile. It must identify the native build
inputs, actual module initialization, selected hook/delegate and GUC records,
global registrations, startup pins and owner items, and restored init-file
producer state. Its transition model must then account for every permitted
fixed command, SI receipt, cache reload/rebuild, subtransaction and release
phase. Reference items and native locks require separate models.

Every asserted state fact needs a stated derivation or authoritative observation.
In particular, a source search, loaded pathname, linked builtin pointer,
owner-pointer equality, cleanup assertion or ReadyForQuery is not an entry
certificate. A construction must explain how unavailable state is established
without reading private layouts or opening the dependent path first. Unknown
history must fail before that path is used.

This keeps installation on stock servers, but makes the supported native
history and artifact correspondence part of the correctness argument. It may
restrict which server configuration and plugin histories can be admitted;
those restrictions must be explicit and must preserve both required use cases.
No such complete construction is accepted today.

## Alternative B: add a native inspection and provenance boundary

Provide a small versioned C boundary implemented inside the native files that
own the otherwise private state, distributed in a project-supplied PostgreSQL
build. The extension consumes copied records through that boundary instead of
casting private structs or deriving addresses from an executable symbol map.
This changes the server installation footprint and is not an extension-only
deployment.

The proposed native surface has four responsibilities:

1. Enumerate existing relcache entries without opening, incrementing, reloading
   or invalidating them. Distinguish intrinsic pins, owned references and
   descriptor provenance. Include invalid or rebuilding state explicitly.
2. Enumerate relation-reference items in the actual owner hierarchy and
   reconcile them with the cache counts. Do not turn an equal aggregate count
   into a claim about equal owners. Capture must reject an unsupported release
   or mutation phase rather than invoke cleanup to obtain a convenient state.
3. Copy the actual global registry, relevant referenced strings/enum data and
   callback/delegate records without initialization, parsing or callback
   dispatch. Preserve uninitialized state, NULL, order and major-specific
   fields. Public hook slots alone are not the complete callback footprint.
4. Record construction and mutation provenance at the native paths that own it,
   including restored init-file data and module initialization before list
   publication. Present-state equality alone cannot certify earlier effects.

The exact ABI, fields, bounds, mutation coverage and lifecycle placement are
design work still required. A version or generation scalar is useful only after
every relevant constructor, mutation, restore, rebuild and cleanup path has a
proved update rule. A census alone cannot replace that preservation proof.
The native build must also bind the code that implements the surface; exporting
matching symbol names does not establish its implementation identity.

The native files are implementation owners, not a copied extension ABI:
relcache.c owns cache entries; resowner.c owns items and resource callbacks;
reloptions.c owns its global definitions. Transaction, SI, loader and GUC paths
retain their own obligations. This proposal is neither a private-layout shim
nor an exception to the pre-open admission requirement.

This alternative makes the missing state observable at its owning boundary.
It also adds native source maintenance, per-major builds, upstream attribution,
version checks, packaging and installation instructions. Stock servers lacking
the selected surface would not silently fall back to a weaker collector.
Existing native PostgreSQL schemas and concurrent two-phase transactions must
remain usable in the supported build.

## Requirements shared by either mechanism

The result must be a concrete entry witness whose base, transitions and validity
frontier are defined. It must govern discovery, cache clearing, new opens,
invalidation/rebuild and cleanup. It may not be a caller-controlled Boolean or
a bundle of unrelated observer results.

All physical waits precede the catalog Share span, with readers and catalog
snapshots closed. A newly discovered dependency releases the entire attempt
before another acquisition. Callback-capable cleanup and data/XID waits remain
outside that Share span. Disabling native two-phase transactions or changing
the required exact AccessShare ownership is not an alternative mechanism.

Cost analysis must cover startup, each reference/registration mutation,
capture counts/bytes, allocation, round trips and lock contention. A stock
history proof cannot infer those costs from module count. A native census may
visit cache entries, owners, callbacks and registry data; caching its result
requires proved mutation coverage. Neither proposal has a measured latency,
memory or throughput result. Backend-local observation must not introduce a
new cluster-wide lock coupling unrelated native prepared transactions.

The next implementation selection must resolve these gaps together with the
deployment footprint. Further observations should have a named role in that
construction. Once the mechanism is selected, independently review its base
and transition argument before expanding descriptor-collector C. Then run
current required ordinary PostgreSQL 17/18 suites and review the implementation;
earlier package or fixture results cannot certify the new mechanism.

## Current evidence and limits

`logs/native-entry-boundary-primary-v1` binds twelve selected source inputs to
the official archive inventories, preserves six complete public headers and
fourteen exact private declaration matches, and rehashes those inputs afterward.
Facts SHA256 is
`2105b34133eb6d3ae7a558fe684dce0d8ecdb2b2ada6863bf28c97eb2daf0e03`;
the 29-member seal is
`abbc1ec19ac304fa8ea2e12d992f97c75b6719b044b7bbf5d7b0b0505180d6ea`.
The capture is source evidence only. It performs no native command, build,
profile lifecycle operation, stress or recovery experiment.

Neither alternative closes entry/startup/provider/registry/reference, writer,
sequence, transitive descriptor/table execution, serving, performance or release
gates. Issues 46/15 and the overall goal remain open. Security-related work,
project compiler PR4, hosted CI and release publication remain excluded.
