# Native reloptions registry construction

Status: paired PostgreSQL 17.11/18.6 source findings. Actual registry/provider
construction remains OPEN before new descriptor admission. This extends the
[bootstrap obligations](native-catalog-bootstrap.md); it supplies no new C
reader or runtime acceptance. Concurrent native two-phase transactions remain
required.

## The registration footprint affects builtin parsing

The global parser table is private native state. `initialize_reloptions` counts
the five builtin arrays and custom registrations, allocates their combined
pointer array in TopMemoryContext, initializes builtin types/name lengths and
appends custom records. `add_reloption` retains a registration, grows its private
array and marks the combined table for initialization again. The record is not
a transaction-owned catalog row.

Allocating a new kind is separate from registering an option. The typed global
registration functions accept a kind mask; `allocate_reloption` stores that
mask without requiring a newly allocated kind. Neither that allocation nor
`add_reloption` checks the name against the builtin parse table. Consequently,
registration is not confined by construction to new custom kinds. The actual
history must establish which records intersect HEAP, TOAST and BTREE, including
their names, types, defaults, order and referenced data. See paired
[PG17 registry construction](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/common/reloptions.c)
and [PG18 registry construction](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/common/reloptions.c).

`init_enum_reloption` retains the supplied member array and detail-message
pointers rather than copying their data. A supplied string validator runs on
the default before record publication; registration then copies a global
non-NULL default into native long-lived memory. Provider identity alone does
not bind these values or earlier callback effects. A later module-path sample
cannot substitute for their construction and preservation proof.

The selected public header declares mutation and parsing APIs, not an actual
global-registry census or generation observation. No private-structure cast,
probe parse, successful SHOW or observed module count is selected as that
missing witness. See paired
[PG17 registry API](https://github.com/postgres/postgres/blob/REL_17_11/src/include/access/reloptions.h)
and [PG18 registry API](https://github.com/postgres/postgres/blob/REL_18_6/src/include/access/reloptions.h).

## Parsing and local registration are different paths

`parseRelOptions` selects every global definition whose kind mask intersects
the requested kind, including unset definitions. Input matching chooses the
first matching name. `fillRelOptions` matches a parse-table name and dispatches
on the registered definition's type; the table's `opttype` is not a production
type-equality check there. `build_reloptions`' count comparison is an Assert.
Correct registry/parse-table correspondence must therefore precede the call;
checking the resulting bytes afterward cannot establish it.

The global string registration supplies a NULL filler. False validation skips
string validators during parsing, but allocation/filling still contain filler
calls for records that have one. Local registration accepts such fillers,
holds its own option/validator lists and builds its own parse table. Its whole
set validators run only with validation enabled. These paths must remain
separate in the admission proof; false validation alone is not a universal
callback-free certificate.

Heap and TOAST use the global default parser; TOAST then overrides its native
fillfactor and analyze defaults. The selected builtin btree parser uses the
same global construction with its three-field parse table. This finding requires
admission of the exact `btoptions` dispatch; it does not establish that admission
or cover an arbitrary access-method callback.
See paired [PG17 btree parser](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/nbtree/nbtutils.c)
and [PG18 btree parser](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/nbtree/nbtutils.c).

## Major-specific fields and presence

The five builtin arrays contain these numbers of definitions intersecting each
individual kind; shared definitions count in each applicable column:

| Native source | HEAP | TOAST | BTREE |
| --- | ---: | ---: | ---: |
| PostgreSQL 17.11 | 22 | 16 | 3 |
| PostgreSQL 18.6 | 24 | 18 | 3 |

PostgreSQL 18 adds `autovacuum_vacuum_max_threshold` and
`vacuum_max_eager_freeze_failure_rate` to the heap/TOAST definitions and default
parse table. It also adds `isset_offset` to `relopt_parse_elt`. Its fill routine
uses a positive offset to write whether a definition was explicitly set; the
default table uses this for `vacuum_truncate_set`. The local builder initializes
that offset to zero. These differences forbid treating the majors as one
parsed-options layout or flattening presence into the Boolean value. The
int-array diff also contains tablespace prefetch changes; those rows are
outside this heap/TOAST/btree admission scope.

Raw catalog NULL, empty and nonempty option images must remain independent of
the parsed result. Unset global definitions still participate in parsing and
default filling. The [startup trace](native-startup-references.md) also requires
provenance for serialized parsed options from their producing backend; a bound
consumer registry does not retroactively certify that producer.

## Construction and cost still required

The entry construction must bind actual registry contents and referenced data
at its base, then preserve them through every allowed callback and command.
Physical catalog locks do not establish backend-private registry state. The
existing module's utility/callback route must be admitted, rather than requiring
every hook to be absent. Unknown registry history must fail before parsing or
opening a descriptor that depends on it. The concrete witness and complete
startup/reference/provider induction remain OPEN; this selection closes no
entry, writer, sequence or table-execution gate.

Initialization visits the combined registry and allocates its pointer array;
the retained custom array grows geometrically. Parsing scans the global table,
then can compare each input entry with every definition for its kind. Filling
can compare every selected definition with every parse-table entry. Name,
enum-member and string bytes add separate work and storage. Module count does
not bound registration count or those bytes. Registry initialization/parsing
belong in admitted setup outside S, with separate bounds on counts and bytes.
This checkpoint adds no
runtime operation or measured performance result.

## Finite evidence

`logs/native-registry-profile-primary-v1` rehashes four named source bodies and
captures two actual HTTP-200 public headers. Selected v1 retains 60 complete
function bodies, ten builtin arrays, two enum-member arrays, twenty native type
declarations and four bounded declaration spans. Seven of48 pairs differ;41
have equal selected bytes. Equality and extraction counts do not certify whole
source files or all captured functions. Other-kind rows in the full arrays are
outside semantic admission.

Primary and selected facts SHA256 values are respectively
`7c30c41a8aa9681eae2117deac6886f0ddbcd9d265b49d3e2959c2e42728c144`
and `a1c0ad77c5ce57d567b3a6859b9ffd09a2c66df4ebfe5c7b882efa0b7ec527df`.
The selected112-member nonself seal is
`da2ea5a827b21679f020bc286d64e635a604078c9450f4a46952e5c6ccb23570`.
Root03/04 actually exit0. Evidence remains outside the Apache distribution.
No native build, runtime/profile lifecycle or new stress/recovery experiment
was executed. Security, compiler, hosted-CI and release-publication exclusions
remain in force.
