---
name: project_want_tier_builtins
description: "want-tier demand built-ins (git_ref, rev_cmp_want->rev_behind, scip_want) + pin-skew example; corpus clones are SHALLOW (guard added); landed main 8111515"
metadata: 
  node_type: memory
  type: project
  originSessionId: 62fb6414-cc3c-4c87-9645-a595ba1a1f36
---

Want-tier demand built-ins, landed local main 8111515 (2026-07-01, branch
feat/dl-want-tier, NOT pushed — Chris pushes):

- `git_ref(repo, refname, kind, sha)` — ref inventory across self+config repos
  (RelKind family in src/rels/git.rs), annotated tags peeled, 2 spawns/repo.
- `rev_behind(repo, refname, upstream, behind, ahead)` — demand set is a
  user-DERIVED rel `rev_cmp_want(repo, refname, upstream)` read by CONVENTION
  (the `org`-allowlist pattern from run_repo_pulls, NOT a reserved rel).
  ahead>0 = diverged. One-tick latency per demand hop; hops COMPOSE (pin-skew
  chain needs 3 ticks: repo rel -> scan -> want -> counts).
- `scip_want(repo)` — user-derived; ScipKind::resolve_index merges self index +
  each wanted repo's ensured index (scip_setup::ensure_index runs installed
  indexers only when index missing) into one temp file, ONE load so cross-repo
  refs resolve. No schema change (per [[project_org_scale_bench]] design — now
  BUILT).
- `repo_want` was NOT needed: repo-sink rules (run_repo_pulls, org allowlist)
  already do dynamic registration.

GOTCHAS learned:
- ~/orgs corpus clones are SHALLOW depth-1 → ancestry counts garbage (2111
  false diverged). rev_behind now skips shallow repos LOUDLY (checked once per
  repo via rev-parse --is-shallow-repository). Tags were also missing; fetched
  2026-07-01 (`git fetch --tags` all 800). go-retryablehttp is the one
  UNSHALLOWED hub (120 real stale pins, cross-org).
- `Engine::rel_rows` used to silently drop rows with int columns (String read
  on INTEGER errors per row) — now stringifies via ValueRef.
- scip_import drops a ref whose symbol has no def in the loaded index — an
  unresolved cross-repo ref is ABSENT, not unresolved-valued.

examples/pin-skew.dl = the proving query (go.mod seam -> pin -> rev_cmp_want ->
stale_pin/diverged_pin; bespoke lockfiles union into pin). Tests:
tests/it/git_ref.rs, scip_want.rs, pin_skew.rs.

Broader recipe catalog (cross-lang/cross-repo fixtures: Online Boutique, eShop,
temporalio api+sdks, zulip OpenAPI; norm column = cross-lang casing bridge) in
chat_log for this session; fixture submodule set decided but not created.
