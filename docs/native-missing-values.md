# Private column missing-value images

Status: implemented candidate; native verification and independent implementation
review remain pending. This extends the finite
[fixed column/type facts](native-attribute-type-facts.md) inside the existing
private heap-storage invocation. It does not admit a TupleDesc, a type codec,
SQL, an executor, default evaluation or a frontend metadata handle.

## Problem and chosen boundary

`atthasmissing` is a declaration, not the missing value. PostgreSQL can add a
column without rewriting existing rows and retain a value in
`pg_attribute.attmissingval`; removing the default does not remove that value.
A future row reader needs the value as well as the physical column layout.
Guessing it from the current default, substituting NULL or invoking a provider
while holding the semantic lease would be incorrect.

This component captures the native one-element array image for every
selected live positive slot whose copied `atthasmissing` is true. The image is
opaque, specific to the current PostgreSQL major and native architecture, and
is not an independently usable SQL datum or an element-decoding certificate.
Actual relation OID, ordinal and element type OID accompany each image. Duplicate
input roots share the same ordered records. Dropped/type-zero slots cannot have
an admitted missing image.

The full variable-catalog alternative would also materialize `pg_attrdef.adbin`
and other TOAST-backed values, with snapshot-preserving fetch, complete physical
catalog storage, provider/effect and source-carrier validation. That remains a
separate required gate. It must account for native fetch through `va_toastrelid`
and cannot be justified by the root's declared TOAST graph. Rejecting legitimate
TOAST-backed defaults forever is not the project plan. This component isolates
the inline missing-array mechanism without admitting those storage operations.

## Source and representation contract

Paired pinned PostgreSQL 17.11 and 18.6 sources describe `attmissingval` as a
nullable one-element array. Ordinary `StoreAttrMissingVal` constructs that array
from the actual column type/layout; ordinary ADD COLUMN only stores a non-NULL
evaluated value. Array construction detoasts variable-size elements before
forming the flat array. Copying the image here does not call that construction
or evaluation path.

Both complete observations must verify the copied actual `pg_attribute` class
is the expected builtin heap with no declared TOAST relation. This is an admitted
native catalog profile, not a claim about arbitrary modified catalog storage.
An external on-disk, indirect, expanded or unknown carrier fails explicitly;
none is dereferenced or passed to a flattener. Short, ordinary four-byte and
inline-compressed carriers are supported. The stored compression tag selects
the native builtin pglz/LZ4 decoder, independently of the user column's current
compression declaration. A build lacking the required decoder errors natively.

The `pg_attribute` descriptor is opened, refreshed and checked outside raw/S
using the existing native-refresh exclusion. The target ordinal must have the
compiled native anyarray/by-reference/varlena layout. A selected tuple claiming
a missing value must physically contain that ordinal. A NULL target is an error.
This precheck excludes `heap_getattr`'s `getmissingattr` path, which can allocate
in native global caches. The admitted accessor is only `fastgetattr`/
`nocachegetattr` over this already prepared builtin descriptor. These routines
walk null/length/alignment information and may populate native offset caches;
they do not open storage, detoast, look up types or call providers. PostgreSQL 18
uses CompactAttribute state; no foreign-version offset or compact state is copied.
Fields preceding the target are traversed only for physical layout and are not
exported or interpreted as application facts.

The carrier pointer, header length and full byte range must fit within the
native tuple before copying. External tags are rejected before any tag-specific
pointer access. Raw copying uses checked lengths and invocation-owned memory.
It does not compare heap transaction/hint/CTID headers or unrelated column
options. It records only the target's actual carrier bytes.

## Invocation ordering

1. The initial coherent five-catalog raw observation copies fixed facts and
   missing-array carriers under its registered nonhistoric catalog snapshot.
   It does not normalize, decompress or follow a datum pointer.
2. Raw/gate bookkeeping ends. All initial scans, snapshot and catalog descriptor
   increments close before physical acquisition, as in the accepted protocol.
   Validate the fixed graph/type/layout and normalize the copied inline carriers
   outside raw and S, before any selected-root physical wait. Only after rejecting
   every external form may native `detoast_attr` normalize the inline image.
3. Before any expansion allocation, validate the stored and expanded sizes with
   overflow-safe arithmetic against the per-observation budget. Validate the
   resulting aligned four-byte array envelope: exact size, one dimension of
   length one, lower bound one, actual element type, no NULL element, and bounded
   data offset. The element bytes remain opaque; no type lookup, array input/
   output, datum provider, nested value interpretation or row decoding occurs.
4. Acquire the complete existing root storage graph. Prepare current SI,
   installation, final catalog descriptors and mapping resolution outside S.
   Take S and perform the final coherent five-catalog observation. It copies
   carriers but performs no decompression, descriptor reload or provider lookup.
5. Existing publication/private/backend/database/context, literal bindings,
   complete storage graph and all fixed fields must agree. Ordered missing
   identities, stored sizes and exact carrier bytes must also agree. An unexplained
   same-identity difference errors. Completed pre-effect lifecycle/generation
   changes retain the existing bounded retry; native ERROR still requires abort.
6. The sole pure consumer receives the checked initial normalized images together
   with the agreeing final facts, while S and all exact physical references are
   owned and raw has ended. It may copy them; no borrowed pointer escapes. S ends
   before reader cleanup and native physical release/retention. Copies are then
   historical. Native transaction/subtransaction and concurrent 2PC ownership
   rules remain unchanged.

For inline carriers, closing the initial catalog snapshot before normalization
is deliberate: the complete bytes are already owned and no storage fetch is
admitted. This reasoning does not extend to external TOAST pointers. No extra
third observation or pre-S default fetch is introduced by this component.

## Costs and limits

Both observations still scan all five catalogs. Only selected missing carriers
are copied, twice, and only the initial copies are normalized. Accessor walking
adds work for missing slots; final byte comparison and initial expansion scale
with actual stored/expanded bytes. A highly compressed inline value can expand
far beyond its heap-page footprint, so the expansion limit must be checked before
the native allocation. Checked cumulative missing-image bytes and the existing
64MiB observation-phase checks bound admitted requests; periodic context checks
can still overshoot and do not establish a peak/process-memory limit.

Expose missing count, normalized image bytes and each observation's stored
carrier bytes separately from fixed array bytes, scanned rows and context
allocated bytes. Normalization/copying allocation events, cache effects,
throughput, contention and full performance acceptance remain separate gates.
Ordinary measurements retain variance and their exact source/profile scope.

The candidate checks stored-byte and normalized-byte cumulative totals separately
against 64MiB, with subtraction before addition. Before each carrier, metadata
array or expansion allocation, the current observation context allocation plus
the requested bytes must also fit that budget. Allocation-block overhead can
still overshoot; the subsequent periodic context check detects it. Ordinary
four-byte images alias their owned carrier rather than making a third copy.
Missing record-array bytes are reported separately. Carrier tags `s`, `u`, `p`
and `l` describe actual short, four-byte, pglz and LZ4 representations. No element
bytes are interpreted by that classification.

## Required verification and remaining gates

Required bounded ordinary fixtures on both native majors must prove no images
for ordinary/NULL defaults, actual scalar and variable-size missing images,
short/ordinary/inline pglz and available LZ4 representations, default removal,
array/domain/enum/composite declarations remaining opaque, duplicate roots,
inheritance and a dropped formerly-missing column. Oracles must check actual
native array identity/envelope and image bytes, native old-row behavior and
complete fixed/storage bindings; the product image contains no SQL probe.
Create a row before ADD COLUMN and a later row with physically present explicit
NULLs. A separate ordinary heap oracle must confirm their different physical
column counts. Before and after DROP DEFAULT, the old row receives the retained
missing value and the present NULL remains NULL. Neither the missing image nor
the column declaration proves that every row is non-NULL.
Fixtures cover unset and established repeatable-read data-snapshot state,
ordinary native release/retention/subtransaction outcomes and prepared DDL
commit/rollback where the new value differs. No new stress, malformed-catalog,
forced-error, interruption/recovery or profile lifecycle experiment is admitted.

Strict product/probe native builds, required native suites, appropriate Rust
formatting/boundary/Clippy checks, artifact/input/environment binding, bounded
ordinary costs and independent second-pass review are required before finite
acceptance. Current fixed-fact images cannot certify changed executable inputs.

This component does not close variable defaults/expressions, transitive types,
constraints/rules/triggers, providers/collation, name/candidate/effect closure,
immutable IR, native preparation/Parse/Bind/Describe/reanalysis, data-derived
storage/table execution, MySQL row/read-lock equivalence, serving, cache/full
performance, artifact or release gates. Issues 46/15 and the goal remain open.

The primary source capture and audit are recorded in the release ledger. Useful
pinned entry points are the paired
[17 accessor](https://github.com/postgres/postgres/blob/REL_17_11/src/include/access/htup_details.h),
[18 accessor](https://github.com/postgres/postgres/blob/REL_18_6/src/include/access/htup_details.h),
[17 missing writer](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/catalog/heap.c),
[18 missing writer](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/catalog/heap.c),
[17 inline normalization](https://github.com/postgres/postgres/blob/REL_17_11/src/backend/access/common/detoast.c)
and [18 inline normalization](https://github.com/postgres/postgres/blob/REL_18_6/src/backend/access/common/detoast.c).
