# Native entry mechanism decision

Status: mechanism comparison, followed by selection of the
[supported stock runtime contract](native-runtime-contract.md). The selected
architecture trusts ordinary native cache/reference services and establishes
Darmok's own functional obligations. No new collector or execution lease is
accepted here. A supplied PostgreSQL build remains an unselected alternative.
Concurrent native PostgreSQL two-phase transactions remain required.

## The unresolved implementation boundary

The [bootstrap contract](native-catalog-bootstrap.md) requires a defining
profile before a new descriptor can increment a reference or rebuild. Current
pathname and builtin-dispatch observers supply useful inputs to that profile.
They do not enumerate every native reference owner or actual options registry,
or certify callback history and restored descriptor provenance. The selected
runtime contract does not require those integrity censuses. Darmok-owned
references, supported path admission and neutral preparation remain necessary.

The selected PostgreSQL 17.11/18.6 public headers and native storage declarations
make the earlier census premise concrete:

| State named by the earlier census proposal | Selected native boundary | What the current API establishes |
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

Opening a descriptor does not create a passive census; the operation still
needs its own supported-path admission. Parsing an option as a probe likewise
cannot certify registry state. A stronger catalog lock establishes neither
backend-private state nor earlier callback effects. These findings limit
observer claims; the selected native contract does not require an integrity
certificate for the inherited native cache.

## Selected alternative A: supported stock native services

Keep stock PostgreSQL and the project-owned extension. Ordinary correctly
operating native services provide descriptor construction, invalidation,
reference bookkeeping and init-file restoration. The private query owner
establishes its own fixed history and local resource lifetimes. An admitted
extension configuration must preserve neutral preparation and supported
provider/registry paths.

The earlier proposed base demanded every native owner item, registration and
init-file producer history. That was stronger than the functional boundary
needed for a proxy using supported native APIs. Missing census APIs do not
require emulating or independently certifying those APIs. The new contract
states the native assumptions and the complete remaining project obligations.
It preserves both use cases, external DDL and concurrent native two-phase
transactions without requiring a new data directory or server distribution.

Source searches, pathnames, current pointers and readiness retain their limited
meanings; none becomes a project lifetime or statement-validity certificate.
Relevant legitimate callbacks still need functional admission before dependent
operations. No new descriptor implementation is accepted by this selection.

## Unselected alternative B: native inspection and provenance boundary

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

The project entry argument must define the supported native assumptions, its
own acquisitions/transitions and the metadata validity frontier. It must govern
discovery, new opens, invalidation and cleanup. It may not be a caller-controlled
Boolean or a bundle of unrelated observer results. The selected alternative
does not require a census of native state owned by the runtime.

All physical waits precede the catalog Share span, with readers and catalog
snapshots closed. A newly discovered dependency releases the entire attempt
before another acquisition. Callback-capable cleanup and data/XID waits remain
outside that Share span. Disabling native two-phase transactions or changing
the required exact AccessShare ownership is not an alternative mechanism.

Cost analysis must cover selected startup and project reference operations,
reachable registry/parser work, capture bytes, allocation, round trips and lock
contention. A stock API contract cannot infer those costs from module count.
A native census may
visit cache entries, owners, callbacks and registry data; caching its result
requires proved mutation coverage. Neither proposal has a measured latency,
memory or throughput result. Backend-local observation must not introduce a
new cluster-wide lock coupling unrelated native prepared transactions.

The selected [stock runtime contract](native-runtime-contract.md) resolves the
architecture and deployment direction. Further observations need a named role
in the project mechanism. Independently review the supported pre-open paths,
local transition argument and exact guard sequence before expanding collector C.
Then run current required ordinary PostgreSQL 17/18 suites and review the implementation;
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

The architecture selection changes the trust boundary; it does not certify
project reference/path admission, writer coverage, guard sequence, transitive
descriptor/table execution, serving, performance or release. Issues 46/15 and
the overall goal remain open. Security-related work,
project compiler PR4, hosted CI and release publication remain excluded.
