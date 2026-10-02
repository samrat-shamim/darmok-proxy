# Native portal descriptions before execution

Status: **component implementation; local verification pending**. Semantic
admission, full dependency validity and the public row executor remain pending.

The connector's `Client::bind_described_builtin` queues Bind, portal Describe
and Sync in one request. It queues no Execute. Success requires BindComplete,
one RowDescription or NoData, and transaction ReadyForQuery. A portal cannot
survive an implicit transaction closed by Sync, so Idle readiness is an error.
This follows PostgreSQL's [extended-query flow](https://www.postgresql.org/docs/18/protocol-flow.html#PROTOCOL-FLOW-EXT-QUERY).

## Description and failure receipts

`DescribedPortal` retains the prepared source handle and the actual native
binary description. `columns()` returns Some(columns) for RowDescription,
including Some(empty) for a zero-column result, and None for observed NoData.
It does not use missing metadata as a zero-column description. Built-in result
OIDs resolve locally; an unknown OID fails without an additional type query.
All portal result formats must be binary. Names, result types, typmods and
relation/attribute origins come from the portal response.

The future encodes parameters before queuing any part of its request. Local
arity or encoding failure has `submitted = false`, no observed readiness and
no portal cleanup request. A submitted failure retains BindComplete and
description observations, the first sequence/state mismatch, representation
error, native SQL error and terminal stream error separately. Rejected metadata
and native errors drain through ReadyForQuery when possible; a stream failure
has no confirmed request boundary. A readiness observation is not recovery.

Only observed BindComplete establishes cleanup ownership of a generated portal
name. Before that observation a failed Bind must not close an existing native
portal with a conflicting name. Dropping an established portal handle can queue
Close/Sync; it does not await cleanup or confirm rollback. Abandoning a submitted
future before BindComplete can leave its outcome unknown. The enclosing owner
must keep its scope active and recover or dispose; no asynchronous Drop reset
is inferred. Portal identifiers use a checked atomic counter with relaxed
ordering for uniqueness, not a cross-request ordering promise.

No Execute being queued does not certify that arbitrary SQL has no effects:
Bind includes native parameter processing and planning. Semantics, functions,
nontransactional effects and ownership must already be admitted at the future
execution boundary. This connector primitive cannot admit frontend SQL.

## Representation guard and row output

`NativeStatementUtc::check_portal` verifies exact prepared-handle identity before
comparing every result label, type, typmod and relation/attribute origin against
the prepared description. Clones of one handle match; separately prepared
statements do not match merely because their columns look the same. The returned
`NativePortalUtc` borrows both receipts and retains the NoData distinction.
Matching descriptions still do not establish a complete catalog dependency set.

`NativeResultUtc::from_portal` binds exact frontend encoding metadata to the
checked bound columns. NoData and a zero-column row description are separate
errors for a MySQL result set. Decode and encode checks remain inside the owning
scope. The result wrapper retains a borrowed column slice rather than a copied
metadata vector. Its rows share that slice, permitting the existing pointer
identity fast path after the once-per-bind comparison.

`Client::query_portal_events` queues Execute/Sync for a described portal and
uses its actual bound description for rows. It retains the cleanup handle while
the stream exists and exposes exact command/empty/suspended/error/readiness
events. A positive row limit may suspend; zero fetches all rows; negative limits
fail locally before queuing. NoData portals reject unexpected DataRow messages.
A suspended portal is not successful complete execution. The stream's completion
and the statement scope's finish/recovery remain separate boundaries.

Existing prepared `query_events` still uses cached statement descriptions.
The [lookup fixtures](native-lookup.md) demonstrate why that cached path cannot
prove current origins after native re-analysis. The described path supplies
observed output facts before Execute; views, functions, hidden writes, types,
collations and all other dependencies still need the
[catalog-validity algorithm](native-execution.md). It does not create an OID lease,
establish route authorization or expose the private native owner's Client.

## Costs

Bind/Describe/Sync costs one request before a separate Execute/Sync request.
With a prior prepare request this adds one round trip relative to queuing
Bind/Execute together. That separation permits rejecting output metadata before
Execute. Future pipelining needs its own admission and correctness proof.
There is one generated name, cleanup handle, immutable description and column
vector per bound portal, with native names copied once. Rows share the metadata;
no result-sized buffer, per-row metadata clone, native type query or new object
lock is introduced by this component. Native planning retains its usual locks.
This is an allocation/round-trip analysis, not measured performance evidence.
