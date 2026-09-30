# Darmok

Darmok is an experimental MySQL-to-PostgreSQL compatibility proxy, licensed
under Apache-2.0. It is being prepared for its first release, **0.1.0**.

The project has two equally important uses:

- Run MySQL applications against PostgreSQL through the MySQL wire protocol.
- Use MySQL clients to access independently created PostgreSQL schemas.

The first release will connect to one PostgreSQL server, expose explicitly
authorized database/schema aliases, and map separate MySQL credentials to
PostgreSQL roles. Compatibility objects require an explicit initialization
command. PostgreSQL grants remain authoritative.

**No runnable release is available yet.** The development workspace and release
gates are being established. Parser acceptance does not imply that Darmok can
execute a statement correctly.

See the [execution plan](docs/release-plan.md), [architecture contract](docs/architecture.md),
and [compatibility policy](docs/compatibility.md). Each release gate requires
recorded evidence from the candidate commit.
