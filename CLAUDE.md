# Engineering policy

Keep this policy synchronized with `CLAUDE.md`.

1. Understand the root cause before changing behavior. Explain options and
   tradeoffs. Implement the correct boundary; do not mask failures with stubs,
   heuristics, swallowed errors, or degraded success.
2. Specifications in `docs/` define intended behavior. Update them with a
   rationale when a deliberate design change is needed. Describe planned and
   implemented behavior separately.
3. Analyze hot-path allocations, round trips, cache behavior, lock contention,
   and resource limits. Measure performance with reproducible workloads.
4. Keep the project independent of applications and external control planes.
   Configuration, catalog metadata, SQL semantics, and PostgreSQL grants are
   the inputs to generic mechanisms.
5. This is a greenfield project. Make clean breaking changes; do not retain
   deprecated APIs, transitional shims, or migration machinery for unreleased
   behavior.
6. Work on feature branches. Push branches, open pull requests, and squash-merge
   reviewed changes. Never push directly to `main`.
7. Review changes for interactions, scoping, NULL behavior, protocol state, and
   untested variants. For nontrivial changes, delegate an independent second
   review with explicit instructions to hunt for missed cases.
8. Tests must exercise real semantics. Required database tests fail when their
   dependencies are unavailable; they must not silently pass by skipping.
   Historical results do not certify the current revision.
9. Preserve upstream licenses and attribution. Keep modified third-party files
   identified and dependencies self-contained and reproducible.
10. Never log credentials or SQL parameter values by default. Enforce routing
    authorization at every statement and protocol transition, including
    prepared statements. PostgreSQL grants are the final authorization boundary.
11. Update `docs/release-plan.md` when a milestone or gate changes. Record the
    revision, command, environment, result, and evidence location. Do not mark
    a gate complete because a related gate passed.
