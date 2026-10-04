# Darmok server module

This Apache-2.0 PostgreSQL 17/18 extension supplies catalog publication and
one-shot snapshot-neutral discovery. It is a prerequisite for catalog-dependent
execution, not a runnable proxy or a MySQL transaction implementation.

Build against the exact server's PGXS development files:

```sh
make PG_CONFIG=/path/to/pg_config
make PG_CONFIG=/path/to/pg_config install
```

Add `darmok_server` to `shared_preload_libraries` before starting PostgreSQL.
Discovery requires a primary server. Concurrent native two-phase transactions may
be enabled; they are separate from MySQL prepared statements. Then
explicitly install in each selected physical database:

```sql
CREATE EXTENSION darmok_server;
```

Every native SQL prepared commit or rollback takes the catalog publication
fence, including pure DML. PostgreSQL retains exact GID handling and native
errors. PREPARE keeps metadata private and does not retain that global fence.
Prepared completion can delay catalog readers while native completion runs and
advances the catalog generation even when no metadata changes.

The private C statement interlock adds semantic reader/publisher modes and a
compatible coverage marker in native prepared lock records. This mechanism has
finite local PG17/18 verification. It exposes no SQL lease or application statement
path; full physical/candidate closure and preparation remain separate gates.
Its installed C header is internal and has no stable public ABI promise. See
[`docs/native-statement-guards.md`](../../docs/native-statement-guards.md).

The private `module_footprint.h` component captures an owned byte image of the
native loaded-module pathname list using public PG17/18 copying APIs. It opens
no catalog descriptor and supplies no entry/provider/reference certificate.
Its current build and verification status, lifetime and cost are recorded in
[`docs/native-module-footprint.md`](../../docs/native-module-footprint.md).

The private C relation-reference attempt preserves ordered explicit OID/mode
increments under the native transaction owner. It releases only its own
increments or retains them for native transaction cleanup, including prepare.
Native relation acquisition now delegates exact local cache clearing to core
outside module exclusion, with finite local PostgreSQL 17.11/18.6 verification
at `e829f50745ea9eb2de1531444c2e04fb9c2eac81`. Per-mode clear does not establish
globally fresh catalog facts, supported callbacks or complete dependency closure.
Neutral statement preparation and application admission remain separate gates. See
[`docs/native-relation-guards.md`](../../docs/native-relation-guards.md).

Catalog preparation uses an exact transient AS seed before opening its initial
readers with NoLock. The SHOW seed owns four heaps plus two critical indexes;
the storage seed owns six heaps plus those indexes and releases before complete
graph acquisition. Reader/scanner/snapshot increments close separately from
physical counts. Failed invocation storage remains in its native transaction
context until abort releases resources. This current draft requires fresh
verification; previous native package receipts do not certify it. See
[`docs/native-bootstrap-references.md`](../../docs/native-bootstrap-references.md).

The nonrelocatable `darmok_server` namespace contains server mechanisms. The
separate `darmok` namespace contains SQL compatibility functions. `darmok init`
installs or validates both components in one owned transaction; `darmok verify`
checks both without creation. Both require the live native handler's checked
response before a separate confirmed COMMIT. Extension files and preloading
must already be present. These commands establish functional setup, not table
execution or serving readiness. See
[`docs/native-database-init.md`](../../docs/native-database-init.md).
The server module rejects late loading rather than supplying partial hooks.

A self-contained Alpine image builds the shared object and LLVM bitcode against
the pinned official PostgreSQL image. Development tools stay in its build stage:

```sh
docker build --tag darmok-server:18 postgres/darmok_server
```

Its default command explicitly preloads the module. Override `POSTGRES_IMAGE`
with the pinned PostgreSQL 17 image from `.github/workflows/ci.yml` for that major.
Packages for other target platforms and the release artifact gate remain pending.

The fixed SET LOCAL/SHOW API returns immutable facts and a publication stamp.
It retains no lease after returning. Catalog heap waits, cleanup and output
occur outside internal Share spans so native prepared transactions can finish.
Full dependency guards, semantic admission, MySQL data-view/lock semantics and
the table executor remain required. See
[`docs/catalog-discovery.md`](../../docs/catalog-discovery.md) for the continuous
private-owner profile, protocol and phase budgets, and
[`docs/server-catalog-lease.md`](../../docs/server-catalog-lease.md) for publication.

Publication fixtures require a separate test image with a native synthetic
Share probe. The product Docker build contains no probe library or exported
SQL lease API:

```sh
docker build --build-arg DARMOK_PRODUCT_IMAGE=darmok-server:18 \
  --tag darmok-server-test:18 tests/probes/darmok_catalog_probe
```

Use a disposable test-image server with `max_prepared_transactions>=2`, a second
test-image server with native 2PC disabled for its default-setting fixture, and
an installed but unpreloaded product-image server for the negative owner fixture.
The disabled profile is a test case, not a serving requirement:

```sh
DARMOK_TEST_DATABASE_URL=postgres://postgres:darmok-test@localhost:5432/darmok_test \
DARMOK_TEST_NO_TWO_PHASE_DATABASE_URL=postgres://postgres:darmok-test@localhost:5433/darmok_test \
  cargo test -p darmok-postgres-tests --test server_catalog_publication \
    --test catalog_discovery --locked -- --nocapture
```

The fixture uses actual lock observations and normal transaction completions.
It does not force interruptions or run resource stress workloads. Missing server
dependencies fail the required tests. The private-owner catalog fixtures also
require `DARMOK_TEST_UNPRELOADED_DATABASE_URL` pointing to the third server and
run explicitly with `native_backend::tests::native_catalog -- --ignored`.

Private interlock fixtures additionally require a fourth disposable test-image
server with `shared_preload_libraries=darmok_catalog_probe,darmok_server` and
native 2PC enabled. `DARMOK_TEST_ORDERED_TWO_PHASE_DATABASE_URL` selects that
server. The ordered callback removes coverage solely in synthetic lock
representation fixtures; it is not a production configuration or recovery test.
Run `--test server_statement_guards` with the same primary environment.
Run `--test server_relation_guards` on that primary for physical reference
ownership and prepared-completion fixtures.

`heap_storage.h` defines a private source-admitted C consumer boundary for
catalog-declared ordinary heap, live btree index and TOAST storage facts.
It acquires and rechecks exact native references and literal names, uses bounded
no-CV lifecycle retry, and ends metadata exclusion before reader cleanup and
reference release/retention. It supplies no SQL admission, executable plan or
data-derived TOAST closure. The continuous builtin private-owner profile and
pure consumer contract are required. See
[`docs/native-heap-storage.md`](../../docs/native-heap-storage.md) for its
contract and the separately recorded implementation verification status.

The same private invocation adds positive column slots and directly referenced
fixed type declarations, filtered by exact root and live-type OIDs. A successful
attempt makes three coherent observations of six builtin catalogs. Dropped
slots retain layout/type0; only live length/by-value/alignment redundancy is
cross-checked. Column storage/compression, typmods, dimensions, collation and
NOT NULL remain independent declarations. Type/default/generation OID edges and
flags are descriptive. The combined physical attempt also owns catalog heaps,
live indexes and declared TOAST storage, deduplicating each exact OID/mode while
retaining application/catalog use provenance.

Selected column default/generation expressions and direct type defaults become
opaque owned native varlena images. Absence and present empty values are distinct.
The middle observation keeps its registered catalog snapshot alive while
admitted builtin TOAST heaps are scanned directly outside raw/semantic exclusion;
the final observation requires the exact source carriers and defining facts to
agree. Missing singleton arrays normalize in that same middle phase. Images do
not evaluate expressions, admit providers/element decoding, replace a physically
present NULL or establish frontend column flags. The metadata-TOAST descriptor
path requires the continuous builtin bootstrap profile, freshly checked NULL
options and initialized native critical catalog indexes before opening targets.

All three scan costs, raw/normalized bytes, cumulative requested copy bytes,
context allocation and TOAST heap/chunk counts are reported to the private
consumer. Direct TOAST scans include unrelated values; these figures are not
allocation-event counts or full performance acceptance. See
[`docs/native-attribute-type-facts.md`](../../docs/native-attribute-type-facts.md)
and [`docs/native-variable-catalog-payloads.md`](../../docs/native-variable-catalog-payloads.md)
for the finite contracts, separately recorded verification status and remaining
gates. Changed native code requires current paired PostgreSQL 17/18 product/probe
builds, required suites and independent implementation review.

The private builtin-dispatch observer copies the running backend's selected
heap/btree builtin rows after checking their mappings, signatures, names and
linked function-pointer equality. It opens no catalog, invokes no handler and
retains no native pointers. It is passive instrumentation, not descriptor or
provider admission; see [the finite contract](../../docs/native-builtin-dispatch.md).
