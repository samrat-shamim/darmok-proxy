# Darmok server module

This Apache-2.0 PostgreSQL 17/18 extension supplies a catalog generation and
transaction-owned read lease. It is a prerequisite for catalog-dependent
execution, not a runnable proxy or a MySQL transaction implementation.

Build against the exact server's PGXS development files:

```sh
make PG_CONFIG=/path/to/pg_config
make PG_CONFIG=/path/to/pg_config install
```

Add `darmok_server` to `shared_preload_libraries` before starting PostgreSQL.
Leases require a primary server. Concurrent native two-phase transactions may
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

The nonrelocatable `darmok_server` namespace contains server mechanisms. The
separate `darmok` namespace contains SQL compatibility functions. Current
`darmok init` and `schema verify` still cover only the compatibility namespace;
combined installation/version verification remains required before serving.
The server module rejects late loading rather than supplying partial hooks.

A self-contained Alpine image builds the shared object and LLVM bitcode against
the pinned official PostgreSQL image. Development tools stay in its build stage:

```sh
docker build --tag darmok-server:18 postgres/darmok_server
```

Its default command explicitly preloads the module. Override `POSTGRES_IMAGE`
with the pinned PostgreSQL 17 image from `.github/workflows/ci.yml` for that major.
Packages for other target platforms and the release artifact gate remain pending.

Global leases cover only admitted catalog phases and must end before row
execution or dependency-lock waits. Full dependency guards and the table
executor remain required. The API, lock scope, required caller checks and
remaining integration work are
specified in [`docs/server-catalog-lease.md`](../../docs/server-catalog-lease.md).
Run its ordinary native fixtures with a disposable module-enabled database and
`max_prepared_transactions>=2`, plus a second disposable module server with native
2PC disabled for the default-setting fixture:

```sh
DARMOK_TEST_DATABASE_URL=postgres://postgres:darmok-test@localhost:5432/darmok_test \
DARMOK_TEST_NO_TWO_PHASE_DATABASE_URL=postgres://postgres:darmok-test@localhost:5433/darmok_test \
  cargo test -p darmok-postgres-tests --test server_catalog_lease --locked -- --nocapture
```

The fixture uses actual lock observations and normal transaction completions.
It does not force interruptions or run resource stress workloads. Missing server
dependencies fail the required tests.
