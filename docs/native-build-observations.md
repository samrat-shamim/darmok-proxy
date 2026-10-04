# Native release, headers and executable observations

Status: paired file observations for the existing PostgreSQL 17.11/18.6 primary
test profiles. The selected [stock runtime contract](native-runtime-contract.md)
does not require original-binary attestation or a complete native integrity
census. Functional path and project-lifetime admission remain OPEN for new
descriptors. This extends the [entry census](native-entry-census.md) and
[bootstrap investigation](native-catalog-bootstrap.md), not runtime acceptance.
Concurrent native two-phase transactions remain required.

## Release corpus and declared source identity

The two existing primary profiles retain their recorded container/image
identities, start times, zero restart counts and startup argument setting
max_prepared_transactions to ten. Read-only observations capture declared PG_VERSION and
PG_SHA256, server version, configure output and selected public-file hashes.
Configuration declarations are retained without admitting unrelated features.

| Release | Declared and published archive SHA256 | Regular archive files | src/contrib C/header files |
| --- | --- | ---: | ---: |
| 17.11 | dd27f2b3c59e73ed14aa3324901242bf69a032a6347805f274e6260322d42979 | 7,085 | 2,387 |
| 18.6 | 555610c24d53e4316da5b7d3fc25c279d96856d5e0e23ee308c328c5fa881d9f | 7,284 | 2,463 |

Both archives and their checksum files return HTTP 200 from the official
[17.11 release directory](https://ftp.postgresql.org/pub/source/v17.11/)
and [18.6 release directory](https://ftp.postgresql.org/pub/source/v18.6/).
Each archive matches both its published checksum and the existing profile's
declaration. The C/header inventory has exactly the same membership and bytes
as the corresponding tagged Git corpus: no missing, extra or changed member.
Neither archive adds generated C/header coverage beyond that tagged corpus.

The five literal queries reproduce the earlier counts: registration 35/35,
rd_refcnt 19/20, reference APIs 25/25, callback registration 18/18 and RelationData 16/16.
Every searched file is rehashed before and after. These remain literal matched
lines, including declarations, definitions and comments. No typed-call,
indirect-write or complete compiled mutation proof follows.

## Installed headers are separate build artifacts

The five observed public headers per major—rel.h, relcache.h, resowner.h,
reloptions.h and fmgr.h—match their release-source bytes. The separately captured
fmgrtab.h also matches its release declaration. This is correspondence for those
files, not for all development headers or the executable's compilation inputs.

Installed configuration and generated headers are additional artifacts, absent
from these archive C/header inventories. Capture preserves six selected files
on 17 and five on 18: pg_config.h, pg_config_os.h, fmgroids.h, fmgrprotos.h and
schemapg.h, plus 17's pg_config_ext.h. Native include Makefiles explicitly install
pg_config_ext.h on 17 and omit it on 18. The failed uniform-header read remains
preserved; the corrected capture uses the actual major-specific sets.

The selected headers declare PG_VERSION_NUM 170011/180006, contain the commented
`#undef USE_ASSERT_CHECKING`, define F_HEAP_TABLEAM_HANDLER as 3 and declare the heap handler.
These header declarations cannot prove the executable was compiled with those
exact inputs or supply a production reference census. In particular, the
[startup cleanup assertion](native-startup-references.md) is not promoted into
an authoritative owner check. Other generated declarations remain outside
semantic admission.

## Selected compiled builtin table

The captured executable files have these SHA256 identities:

| Profile | Executable SHA256 | Exported builtin count |
| --- | --- | ---: |
| PG17 primary | ef90f3d87c43b7779903d21478fa8dea85dc48ee9d2961b9bc32f82a537343d4 | 3,023 |
| PG18 primary | de7eb6c98c3dc9cc8d8ce80a9092e9fe027a6de0bddde61dd066e7b559f6c6ab | 3,102 |

The public fmgrtab.h declaration exposes fmgr_builtins, fmgr_nbuiltins,
fmgr_last_builtin_oid and fmgr_builtin_oid_index. A bounded file reader uses
their exported ELF symbols, file-backed load segments and the declared
AArch64 LP64 record layout. It checks array lengths and follows OID3's index
to its actual row. Both rows name heap_tableam_handler, declare one argument,
strict true and retset false. Both selected pointer pairs use
R_AARCH64_RELATIVE relocations; the resolved link-image function target matches
the exported heap_tableam_handler symbol. The other builtin rows are not
semantically admitted by their aggregate count. See the native
[PG17 public table](https://github.com/postgres/postgres/blob/REL_17_11/src/include/utils/fmgrtab.h),
[PG18 public table](https://github.com/postgres/postgres/blob/REL_18_6/src/include/utils/fmgrtab.h)
and [AArch64 ELF specification](https://github.com/ARM-software/abi-aa/blob/main/aaelf64/aaelf64.rst).

The paired native catalog rows declare one internal argument. This differs from
the traced zero-argument OID invocation in GetTableAmRoutine. Declared arity,
actual call arity and the handler's source-admitted body are separate facts;
the entry proof must retain that distinction. The initial reader's zero-arity
assertion failure remains preserved separately from the corrected observation.
No provider is invoked and no backend memory is inspected by these file reads.

The selected file data narrows the original-builtin identity question. It does
not establish actual runtime relocation/binding, continuous loaded-module and
registry history, init-file producer provenance or all reference owners.
Executable hashes and matching declarations also do not prove complete
source-to-binary derivation or generated/compiled transition coverage. Those
integrity certificates are superseded acceptance premises under the selected
native API boundary. Supported dependent paths, local project acquisitions and
statement validity still require their functional arguments and verification.
No private-layout reader is selected.

## Cost, evidence and remaining work

The archive scans, header capture and ELF decoding run outside the proxy. They
add no product round trip, allocation or lock. A future observer's traversal,
copying, bounds and lifetime need their own admission and cost analysis; no
latency, memory or contention acceptance is claimed here.

Primary facts at `logs/native-build-corpus-primary-v1/facts.json` have SHA256
`26c7769e7c9cf1b0df45619f68de2a7610e371fa1935a52e83ff1e0bd3569416`;
the 98-member nonself seal is
`8d8336ee283bfc9e98b57d97b06bb1b564c32a2445d0f0e51192cceff36afac0`.
Header v2 facts/104-member seal have SHA256
`ca29b22c55246addc94c5f606783467f126804891da09bef7c03f79553d98474`
and `d5b2f97df0adb22923a05da23818b7e93e2b6fc4415e4363c1a719d49ad432b6`.
Builtin v2 facts/46-member seal have SHA256
`34d2f712c45128d45f2787bcae8fbc3dfcde77fc9ff992a7aa5dea6c66a6042c`
and `69609503dd8916ca51c70791e8a7c7c005ee834fc080b152733c2b2477c12823`.
The pinned pyelftools 0.32 wheel is separately recorded outside the distribution.
Its initial installer option failure retains actual exit 2; the corrected
temporary environment uses the checked wheel and actually installs successfully.

Root03/05/08/10 actually exit 0; failed root04/07/09 retain exits 1/2/1 and their
raw streams. These are observation/tooling failures, not native product errors.
Archives, installed-file observations and review evidence stay outside the
Apache distribution. All 22 native inputs remain unchanged. No C/Rust/SQL,
native build, new server, profile restart/stop/signal, live-memory or provider
invocation, security or project compiler work, stress/recovery experiment,
hosted CI or release publication is added. Full build/entry/bootstrap/provider/
registry/reference/writer/sequence/table execution, serving, performance and
release gates remain OPEN.
