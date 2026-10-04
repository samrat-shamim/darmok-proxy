# Native descriptor branch declarations

Status: locally verified at `06ce5b026cba6a9c9dc35fb1957f9f3bec14bfcd`
with paired PostgreSQL 17.11/18.6 packages, eight required profiles and 124
required native tests, with none failed or ignored. Independent implementation
review is required before merge. This is a finite addition to the private
storage observation, not new descriptor admission. Historical native binaries
do not certify this change.

Each selected application or metadata storage node returns the actual
`pg_class.relchecks`, `relhasrules` and `relhastriggers` values as
`declared_check_count`, `rules_hint` and `triggers_hint`. They are copied from
the existing class scan into the invocation-owned storage facts. Duplicate roots
share physical facts; application/catalog use provenance and exact modes are
unchanged. Unrelated catalog rows do not become selected facts.

The signed native CHECK count must be nonnegative for a selected node and agrees
across the initial, source and final definition observations. It is the catalog's
declared count, including CHECK constraints that have not been validated. It is
not a copied constraint list, evaluation result or frontend nullability proof.

Rule and trigger flags are conservative native descriptor branch hints. Their
actual final-observation bits are returned. Like index/subclass hints, they are
excluded from definition equality: native maintenance can clear stale hints
without changing an object definition. DROP does not justify inventing a false
bit. Neither bit proves complete object presence or absence, writer exclusion,
provider admission, or permission to open a rule/trigger descriptor path.
The existing metadata-TOAST/critical-index bootstrap checks use these same
fields instead of a second private copy.

There is no extra scan, native lock, SQL round trip, descriptor open, provider
call or options parser. A successful attempt still has twenty-one direct catalog
scans across its three coherent observations, plus selected metadata TOAST
scans. Two booleans and one native int16 join the fixed public facts; redundant
private fields are removed. Actual structure padding is compiler dependent.
Existing `sizeof`-based map/fact allocations, copy and context budgets account
for the changed structures. Throughput, allocation-event and contention
acceptance remain separate open gates.

The ordinary fixture compares complete selected graphs to an independent SQL
catalog oracle before rule/trigger creation, after two CHECK constraints
(including NOT VALID), after validation/drop, and after ordinary VACUUM. It
covers duplicate roots, an absent declaration set, unrelated declared objects,
first-unselected and genuinely established repeatable-read data snapshots,
and earlier owned results.

The prepared fixture covers commit and rollback for both data-view states.
A real read of an unrelated sentinel establishes repeatable read without
blocking access-exclusive target DDL. The prepared writer adds rules, triggers
and CHECK constraints and changes the sentinel. An independent observer sees
the physical acquisition wait, checks that no catalog horizon survives it,
preserves any established data horizon, and completes the native transaction.
Current declarations must match the oracle while the reader's data visibility
follows its original PostgreSQL snapshot. No native 2PC setting is relaxed.

Complete constraint/rule/trigger definitions, transitive type closure, new
catalog/bootstrap descriptor paths, provider effects and executable plans remain
open. These fields supply missing inputs to the
[descriptor bootstrap investigation](native-catalog-bootstrap.md); they do not
complete its pre-open proof. See [release-plan.md](release-plan.md) for current
verification evidence and [native-heap-storage.md](native-heap-storage.md) for
the enclosing private lifetime contract.
