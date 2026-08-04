# lab/host-edge REPORT

Three commits: 96cc9d10 (cone edge), a3f34367 (host-free pruning fixture),
3deb25c4 (statement-count tests). Base 3d97fd4f verified before edit.

## The edge
1_host_expand.pl splits a probe into a demand rule plus a response ATOM; the
response rel is EDB, so the backward cone walk over rule bodies stopped there
and a live host under SPREFA_TSV2_SUBSCRIBE_PRUNE=on never derived demand.
Fix: host_edge/3 in 2_subscribe.pl reads the demand/response pairing from the
host DECLARATION (sh_decl/4 post-expansion, via host_relation_refs/3 newly
exported from 1_host_expand.pl), so a subscribed response rel keeps its demand
rel in the cone, at rel-kind level, no per-program case. Both doors compute
the cone from the one predicate (compile.pl:182, conformance/engine.pl:560),
so oracle and emitted constant moved together; 4 emitted modules regenerated
(subscribedRels line only).

## Fixture deltas
native_ts_query_term under the flag now prunes NOTHING (its only off-cone rel
is interval, an arrival target): statements 47 -> 56 == flag-off, demand rows
0 -> 1. The pruning receipt moved to the new host-free fixture
host_free_query_leaves_a_derived_rel_unsubscribed (5_compiler_quality.pl D3):
flag on drops statements 45 -> 23, boot 7 -> 3, audited/audit_trail rows and
statements to 0, watched untouched.

Receipts, both verified red-first:
- edge removed: query-bearing tests red (demand rows expected [] got the
  identity/witness rows).
- SubscribeCone.levels unfiltered under "on": host-free test red with
  "incremental level head relation missing: audited".

## Gates
Lane runs and coordinator re-runs agree: conformance 290 PASS exit 0
(+host_free fixture), plunit 324/324, TEXT_DOOR 203/203/0, tsv2 146 tests
144 pass 2 skip. Extras from the lane: sweep 203/202 identical, 1 known
rejection, 0 crashes; typecheck, import-gate, prolog-lint, arch all exit 0.

## Deviations
1. Worktree needed pnpm install --frozen-lockfile (tsv2 + sprefa-store/js);
   lockfiles unchanged.
2. just staleness-gate red at HEAD and after, PRE-EXISTING (verified by
   stashing the lane diff): door-handwritten.ts stale vs its .dl6 source
   since the ladder step 2 merge. Not this lane's file; owner needed.
3. Pre-commit hook regenerated v6/INDEX.md dropping 20 main-tree-only rows;
   self-heals on next main-tree commit.
4. Sweep churns SWI variable numbers in two 2_hosts_wiring.pl manifest
   reasons; MANIFEST_REASON_DIFF args=0 restated=0 gates exactly that.
