# Darmok

Darmok is a MySQL-to-PostgreSQL compatibility proxy, licensed
under Apache-2.0. It is being prepared for its first release, **0.1.0**.

The project has two equally important uses:

- Run MySQL applications against PostgreSQL through the MySQL wire protocol.
- Use MySQL clients to access independently created PostgreSQL schemas.

The first release will connect to one PostgreSQL server, expose explicitly
authorized database/schema aliases, and map separate MySQL credentials to
PostgreSQL roles. Compatibility objects require an explicit initialization
command. PostgreSQL grants remain authoritative.

The development workspace provides a `darmok` executable for explicit database
initialization and verification. **The serving proxy and first release are
still in progress.** Parser acceptance does not imply that Darmok can execute
a statement correctly.

Build the setup tool with the pinned Rust toolchain:

```sh
cargo build -p darmok-cli --locked
target/debug/darmok --help
target/debug/darmok --version
```

Select one existing physical PostgreSQL database through an environment
variable, then install or verify both Darmok components:

```sh
export DARMOK_DATABASE_URL='postgresql://db_user@localhost/my_database'
target/debug/darmok init --database-url-env DARMOK_DATABASE_URL
target/debug/darmok verify --database-url-env DARMOK_DATABASE_URL
```

PostgreSQL 17/18 with UTF8 and an already installed and preloaded
[`darmok_server` module](postgres/darmok_server/README.darmok_server.md) are
prerequisites. Initialization checks the compatibility schema, server extension
and live native handler transactionally, and accepts an existing installation
only when it matches. Verification never creates or repairs objects. See the
[command contract](docs/cli.md) for current behavior and requirements.

See the [execution plan](docs/release-plan.md), [architecture contract](docs/architecture.md),
and [compatibility policy](docs/compatibility.md). Each release gate requires
recorded evidence from the candidate commit.
