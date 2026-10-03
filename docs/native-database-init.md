# Explicit database initialization

Status: independently reviewed design with implementation candidate; runtime
checks passed at the recorded implementation revision; independent implementation
review and current packaged-artifact checks are pending. This extends the functional [schema contract](native-schema.md) without
admitting table execution or certifying a serving proxy.

## Problem and selected behavior

The existing setup command commits the `darmok` compatibility schema alone.
Native catalog operations separately require the `darmok_server` extension in
the selected physical database and the preloaded module's utility handler.
Reporting setup success before checking both leaves an incomplete installation.
A dotted `SET` acknowledged by PostgreSQL's placeholder mechanism is not proof
that the handler ran.

`darmok init --database-url-env ENV_NAME` will explicitly install or validate
both components in one owned transaction. `darmok verify` will validate both
without creation. The library entry points are `initialize_database` and
`verify_database`. The old schema-only entry points and nested CLI command are
removed, with no aliases or migration machinery.

The physical database must already exist. PostgreSQL 17/18, UTF8, installed
extension files and an already preloaded `darmok_server` module are prerequisites.
Setup cannot load a shared-preload module into a running server. Concurrent
native PostgreSQL two-phase transactions remain supported; this component does
not require `max_prepared_transactions=0`, change server settings or certify the
full execution profile. Existing native profiles remain running unchanged.

## Fixed requests and actual receipts

Only a confirmed idle exclusive owner can begin setup. No caller SQL, native
client or custom manifest is exposed. Initialization uses read-write access;
verification uses read-only access. Both use read committed and not deferrable.

The first request contains:

```sql
BEGIN ISOLATION LEVEL READ COMMITTED READ WRITE NOT DEFERRABLE;
-- One fixed DO manifest checks both components; verification uses READ ONLY.
SET LOCAL darmok_server.catalog_request_v1 = E'[]';
SHOW darmok_server.catalog_request_v1;
```

The checker drains and requires exactly `BEGIN`, `DO`, `SET`, one unmodified
native TEXT description, one non-NULL text cell, `SHOW`, and transaction
ReadyForQuery. The description has the exact setting name, type OID25, size-1,
typmod-1, text format0 and absent table/column identities. The existing strict
protocol1 decoder validates the empty relation request and native identity
header. A placeholder echo of `[]`, wrong protocol, malformed identity, extra
fields or nonempty facts is an error. The shared SET/SHOW checker accepts only
the private fixed prefix; public empty catalog discovery still returns no stamp
and performs no SQL.

Only after the complete request and decoded response agree does the owner send
the second request, `COMMIT`. Success requires its actual `COMMIT` tag and idle
ReadyForQuery. A `NativeDatabaseCompletion` retains the actual commit receipt,
action, compatibility format version and checked extension version. It cannot
be fabricated from the first request, an expected version, a placeholder echo
or a generic command stream that discards rows. This receipt is functional
setup evidence, not a persistent catalog lease, artifact fingerprint or admitted
statement. CLI success additionally requires awaited local driver disposal.

## Functional manifest

The existing qualified compatibility manifest and version1 kernels remain
unchanged. The same database-scoped transaction advisory key
`4922526098346491905` covers both components through the checked commit.
Initializers take it exclusively and verifiers share it. Ordinary competing
owners observe committed artifacts after acquiring the lock. This serializes
explicit setup, not arbitrary DDL or application execution.

When absent, initialization uses literal `CREATE EXTENSION darmok_server
VERSION '1.0'`; verification reports absence. There is no `IF NOT EXISTS`,
`CASCADE`, replacement, repair or automatic update. A reserved `darmok_server`
namespace without its extension is a partial installation and is rejected.
The extension's fixed control schema can create the absent namespace.

An existing installation must have version1.0, fixed `darmok_server` namespace,
nonrelocatable status, NULL configuration arrays, no extension-membership
objects, and the expected normal dependency on its namespace without prerequisite
extensions. The namespace has no SQL object inventory: the shipped installation
script creates no functions, types or relations. Incoming nonmembership
application dependencies are not inferred as extension members. Ownership,
ACLs and credential policy are outside this functional manifest and remain
excluded. Changed or partial artifacts fail without altering their committed
identity. Existing helper/table OIDs and row/catalog xmins remain stable on
successful repeated setup and verification.

## Failure and ownership

Setup has its own in-flight owner state. Dropping a polled operation before a
confirmed completion leaves the owner uncertain; no asynchronous cleanup is
invented. A pre-submission invalid state leaves existing caller work unchanged.

An ordinary backend error with an otherwise exact stream and confirmed failed
transaction readiness permits an explicit awaited rollback. A completely
observed transaction followed by a response-decoding error also permits that
rollback, before any commit is submitted. Original submission/completion/SQLSTATE
or decoding error and rollback outcome remain separate. Unexpected event order,
tags, description/cell shape, readiness, stream failure or missing readiness
leave uncertain state and require disposal. No success, retry, reset or commit
receipt is inferred. The checked commit retains the existing owner's failure
rules; an uncertain commit is never reported as a rollback or setup success.

## Cost and required verification

Setup uses two operation round trips instead of the schema-only batch's one.
The extra trip places validation before commit. SQL/response allocations occur
once per setup, not per row. The empty native request does not scan relation
facts. Native mechanism sources and build definitions need no change. The packaged
module README is updated for the new setup contract, so native artifacts must
be rebuilt with that new documentation input. The existing running profiles
remain unchanged. Their reuse requires exact equality of the 19 mechanism,
header, control, script and build inputs plus actual library/header hashes;
their old packaged README is explicitly historical. Current package acceptance
requires its rebuilt README to match the new source, not just matching tags.

Required ordinary PG17.11/18.6 fixtures cover both installed components,
unchanged repeats/read-only defaults, ordinary two-owner initialization and
verification, absent/partial/changed functional inventories, preservation of
caller transactions, qualified temporary-type lookup, the existing private
postcreation manifest mismatch and 810-case helper corpus. Actual CLI children
check both artifacts, selected independent databases, output/exit status and
local disposal. The existing unpreloaded profile supplies an ordinary unsupported
installation boundary: placeholder output must fail decoding before commit and
leave created artifacts uncommitted. A second cooperating owner must positively observe the advisory lock after the
checked SHOW and before the separate COMMIT, then observe both committed
components. The fresh unpreloaded-database fixture must retain the decoding
failure and actual rollback receipt and prove both namespaces and the extension
absent afterward. A public function with an ordinary DEPENDS ON EXTENSION
dependency must be accepted, while added extension members and extra objects in
the reserved namespace must be rejected. Offline sequence/decoder checks cover
malformed replies without introducing a native forced-error experiment.

Record exact source, native-input/profile reuse, commands, original outcomes and
independent second review before merging. Do not create new stress, forced
native error, interruption, recovery or profile-lifecycle experiments. Security,
compiler PR4/source/resources, hosted CI/account work and release publication
remain excluded. M2/M3, issues46/15 and the overall goal remain open.

Functional source references: PostgreSQL [17 CREATE EXTENSION](https://www.postgresql.org/docs/17/sql-createextension.html)
and [18 CREATE EXTENSION](https://www.postgresql.org/docs/18/sql-createextension.html),
paired pinned [`pg_extension.h`](https://github.com/postgres/postgres/blob/REL_18_6/src/include/catalog/pg_extension.h)
and [`InsertExtensionTuple`](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/commands/extension.c).
The paired source/HTML capture is `native-database-init-primary-v2`, manifest
SHA256 `8bf7fdfae4921c30896a9e693715db979a1ed697effbab74fff50407e11431a5`
and completed original seal SHA256
`1fad3314e253f7987c39fe245b06d4bfb3155450ab347119559fe8a4e02073d5`.
That original seal is invalid because its self entry records an empty file; it
remains preserved with review finding P1 open at the original design revision.
The distinct nonrecursive correction `native-database-init-primary-v3` verifies
the same six sources and seven nonself members. Its 11-member seal SHA256 is
`1aad1279e0b23ffadbbfe7170747a3ec98e86200b58632b566bf3d12e605028c`,
with correction facts SHA256
`6cebe6b5f618eb472b23551f61844b2938ffa9c0f85b6133682e774db3d3d8a9`.
This proves saved HTTP source provenance, not Git blob identity, a new native
build or binary identity.
