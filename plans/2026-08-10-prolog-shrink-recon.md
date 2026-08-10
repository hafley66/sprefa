# Prolog compiler shrink: stage-1 recon, merged verdict

Succession doc. Three same-brief recon runs (opus5 native, flash4 opencode
lane, gpt-5.6-luna codex lane) ran read-only against main `3b9e9cfd`. The
three originals landed one commit before this file and were deleted in the
same commit that adds it; `git log --follow plans/2026-08-10-prolog-recon.*`
reaches them.

## TOC

1. Verdict
2. Corrected numbers (three stale counts found)
3. Deletion first: the dead trees
4. Defects found by reading (fix regardless of refactor)
5. Duplication inventory (merged, deduped)
6. The live-compiler moves, ranked
7. Traps stage 2 must not bundle
8. Contestant comparison
9. Stage-2 proposal

## 1. Verdict

The compiler is wide, not fat. lower.pl = 402 predicates / 611 clause heads
averaging 9.7 code lines (opus receipt). Duplication detectors found 11
cross-file groups total. The big shrink is deleting unloaded trees (~4.9k
lines); the honest refactor range inside the live compiler is ~250 lines of
pure dedup (opus, high confidence) up to 1.5-3k lines of data-table rewrites
(luna, estimate, higher risk, byte-identity gated).

## 2. Corrected numbers

| claim | stale value | measured | receipt |
|---|---|---|---|
| conformance fixtures | 281 (CLAUDE.md/brief), 221 (justfile header) | **346 PASS / 0 FAIL, 0.328s** | fixture( heads across conformance/fixtures/*.pl, 39 files |
| lower.pl | 5652 | 5693 | wc |
| parse_dl.pl | 1970 | 1983 | wc |
| "246 goldens" | ambiguous | 246 = bucket:compiled rows in compile/out/manifest.json (346 total, 100 unsupported); v6/tsv2/goldens/ has 11 dirs | jq + find |

## 3. Deletion first: the dead trees (~4,916 lines, zero risk)

| tree | lines | proof of deadness |
|---|---|---|
| v6/prolog/labs/** | 4,635 | zero loaders: `grep -rn "use_module.*labs" --include=*.pl v6/prolog` empty; justfile refs point at v6/labs/, a DIFFERENT dir (opus) |
| v6/prolog/src/emit_ts.pl | 239 | marked superseded at ARCH.pl:195 (opus) |
| v6/prolog/src/checks.pl | 42 | CORRECTED 2026-08-10: the claimed ARCH.pl:700 superseded marker does not exist, and examples/ghcacher.pl:20 loads it live — NOT dead, kept (flash re-verify caught the false receipt; deletion executed without it) |

Labs-die-on-landing law says these should already be gone. Gate: battery
green after `git rm`, nothing else.

## 4. Defects found by reading (fix regardless of refactor)

1. **balanced_parens_/5 blind to string literals** (parse_dl.pl:1504-1510):
   `not(link(node, "z)z"))` throws dl_parse_error while the same string
   outside a wrapper parses. Hits every wrapper(...) surface: latest, not,
   decode, pre, now, coalesce, seq, finalize. Fix pattern exists at
   cst_block_codes/3 (parse_dl.pl:1398). ~4 lines + a fixture; none covers
   it today. (opus)
2. **analyze.pl vs conformance/level_eval.pl have drifted**: analyze.pl:1697
   requires `no_refs` where level_eval.pl:60 accepts `_`. This pair IS the
   differential oracle; align the semantics, never merge the files. (opus;
   flash independently flagged rule_is_edge/1 + rule_body/2 as copies on the
   same seam)
3. **refCount rename never reached emitted DDL**: ref_count_table_name/2
   still emits `__support_next_~w` (lower.pl:249), baked into all 246
   goldens + plunit_tests.pl + tsv2/runtime/1_incremental.ts. Its own arc,
   never a rider. (opus)

## 5. Duplication inventory (merged, deduped)

Identical-logic pairs, all three contestants' finds, conflicts resolved by
reading:

| logic | site A | site B | found by |
|---|---|---|---|
| module_hash/2 (differ by one cut) | use_resolve.pl:244 | lower.pl:753 | all three |
| catalog id stride walk | lower.pl:1373-1377 catalog_rel_id_map | lower.pl:1385-1390 catalog_rel_block_end | all three |
| column_type_decls/3 | 0_ast_expand.pl:220 | 1_host_expand.pl:566 | flash |
| build_rule/4 | 0_ast_expand.pl:258 | 0_coalesce_expand.pl:103 | flash |
| memberchk_eq/2 | 0_coalesce_expand.pl:179 | 0_dot_expand.pl:647 | flash |
| rule_is_edge/1 + rule_body/2 | 0_program_check.pl:804-806 | analyze.pl:62-71 | flash |
| primitive/list row walkers | lower.pl:1315-1321 | lower.pl:1355-1362 | luna |
| SQL template array render | emit_ts.pl:1127-1130 | emit_ts.pl:1278-1281 | luna |
| optional sql->template (none->null) | emit_ts.pl:1211-1212 | emit_ts.pl:1232-1244 | luna |
| snapshot read entry lines | emit_ts.pl:901-905 | emit_ts.pl:928-932 | luna |
| decl column-list loops | parse_dl.pl:665-668 | parse_dl.pl:854-858 | luna |
| trigger-kind normalization | lower.pl:2805-2807 | emit_ts.pl:1766-1767 | luna |
| json_capture_type mirror | lower.pl:4754-4778 | conformance/body.pl:237-240 | luna — INTENTIONAL oracle mirror, treat like §4.2: align, never merge |
| operator precedence tiers | compile/registry.pl:236-240 (data, printer reads it via print_dl.pl:606) | parse_dl.pl:1727-1758 (hardcoded) | opus — the cheapest grammar-as-data opening |

Dead-predicate conflict: opus found 5 (~50 lines: avg_scope_from/4,
avg_join_equalities/3 at lower.pl:3419-3435, incremental_carry_expr/2 at
emit_ts.pl:2451, normalize_float_json_atom/2 at 0_type_plane.pl:825
compiler-dead/oracle-live, avg_delete_scoped_sql/5 passthrough at
lower.pl:3373). Luna's textual scan reported zero and said so (textual only,
no meta-call proof). Opus's list carries the receipts; stage 2 re-verifies
each before deleting. Opus also documented 5 false positives so nobody
re-chases them (see the opus original, §6).

## 6. The live-compiler moves, ranked

Luna's 10-row plan carries the granular line ranges; opus's caveats cap the
expectations. Merged ranking:

| rank | move | lines saved (est) | gate |
|---|---|---|---|
| 1 | DONE #105: delete dead trees (§3) | 25,111 actual | battery green |
| 2 | CLOSED 2026-08-10: pilot #111 took the one profitable slice (-45, six level-plane families). A follow-up lane converted the two remaining candidates (per-rel planes, views) and MEASURED +34 net: at 3-row family size the walker overhead exceeds the savings, so the 700-1,000 estimate is falsified and the unmerged branch refactor/descriptor-families holds the receipt. No further rank-2 lanes. | -45 actual (est was 700-1,000) | conformance 346/0 + byte-identity 246 + green-all |
| 3 | DONE #108: operator tiers, parser reads registry data | ~30 | TEXT_DOOR 246 |
| 4 | first slice DONE #114: fixpoint_*_text via js_shape/2 descriptors measured -7 (est 250-400 for the whole rank; treat the estimate as optimistic and re-size per family before dispatching more). Remaining families: DDL, snapshot, arrival, relation-plan, aggregate, expand/dred. | -7 so far | byte-identity 246 |
| 5 | dedup pairs from §5 | ~250 | battery |
| 6 | unify A/B decl grammar loops (parse_dl.pl:598-887) | 180-260 | TEXT_DOOR + plunit |
| 7 | production-table for the regular parser surface (~1,150 of 1,983 lines regular; luna est 600-800 saved; opus counter: variable-identity threading through 114 predicates resists, a lexeme//1 wrapper hits the same 153 ws0 + 119 lit_dcg noise without redesign) | 300-800 OR the cheap lexeme route | TEXT_DOOR + plunit + conformance |
| 8 | JSON pattern ops table-driven, stateful path driver stays (lower.pl:2496-2709) | 180-300 | conformance |

## 7. Traps stage 2 must not bundle

- refCount `__support_next_` rename: own arc (§4.3).
- Oracle mirrors (§4.2, §5 json_capture_type): align semantics, keep two
  copies — merging destroys the differential oracle.
- Byte-identity is the gate for anything touching emit; the 246 compiled
  manifest entries are the corpus.

## 8. Contestant comparison

| axis | opus5 | flash4 | luna |
|---|---|---|---|
| deliverable | 427 lines | 278 lines | 204 lines |
| unique wins | dead labs tree, parser defect, drift finds, dead preds, refCount trap | most dup pairs, stale-count triple audit, cleanest mass map | granular lower.pl 16-section map, ranked plan with line ranges, parse feasibility fractions |
| misses | fewer dup pairs than flash | no defects found | dead-pred scan came up empty |
| wall time | ~15 min | ~40 min (exit-hail) | ~3.5 min work, then hung at prompt 8h (harness defect, see failure-modes entry this date) |
| character | auditor | accountant | cartographer |

All three agreed independently on: corrected masses, both brief seeds,
conformance 346. Zero contradictions on facts, one on dead preds (resolved
§5).

## 9. Stage-2 proposal

1. Dead-tree deletion PR (rank 1) — mechanical, today-sized.
2. balanced_parens_ fix + fixture (defect, not refactor).
3. One descriptor-table slice of rank 2 as a pilot with byte-identity
   receipts before committing to the family.
4. Operator-tier unification (rank 3).
Order 5+ awaits the pilot's verdict. Stage 3 = execution wave, worktree
lanes, one move per lane.
