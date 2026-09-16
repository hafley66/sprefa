# FUSE-REPORT

Mechanical fusion of the three laziness impact documents (base/opus legacy,
fable, flash) into one doc pair. Editorial decisions were pre-adjudicated;
this lane performed assembly only. No deviations from the brief were found,
so no STOP-and-report event. Bounds respected: wrote only the three files
below; the other two worktrees were read-only; no mutating git commands; no
subagents.

Outputs:
- IMPACT.fused.md (1106 lines)
- IMPACT.fused.visual.human.unga.md (232 lines)
- this file

## Corrections applied to BASE (receipt fixes)

| old | new | location in fused |
|---|---|---|
| `readBoundary` cited `1_incremental.ts:1030-1063` | `1_incremental.ts:993` | section 1.1 table row and section 5 conditional claim 2 |
| analyze.pl triple cite: `program_refs/2`, `derived_refs/2`, `body_ref_uses/2` all at `:104-107` | `program_refs/2` at `:231`, `derived_refs/2` at `:80`, `body_ref_uses/2` at `:104-107` | section 4.3 |
| fixture `struct_host_output_schedule_answer_interned` at `4_struct_values.pl:757` (2 cites) | `:421` | section 1.6 and section 6 fixture-bill table |
| "tsv2 imports exactly one thing from the store package, stmt_counter" | `runtime/scratchStore.ts:14-15` imports `open_db` and `SqlRunner`; `runtime/types.ts:4` imports five types. Scope conclusion kept: store lowering is off the tsv2 emitted path. Quantifier dropped. | section 1.10 |

## Grafts applied, with destination section

| # | source | graft | destination in fused |
|---|---|---|---|
| 1 | fable §4 | NEW shared module `2_demand_cone.pl` (`demand_cone/4`, `prune_to_cone/3`), argued from `compile.pl:88-98` (1_host_expand.pl already shared compiler/oracle). REPLACES base's analyze.pl+compile.pl placement. Keeps base's `0_graph:collect_reachable/6` reuse note. | section 4.3 |
| 2 | fable §2 | "INGRESS IS NOT EVALUATION" as section-opening framing sentence | top of section 2 |
| 3 | fable | F-fix-A added as a FOURTH fork option (C4) in base's C1/C2/C3 fork, with base's C2 objection (second source of truth) noted against it | section 1.6 |
| 4 | fable §6 | revert-boundary sequencing overlay (steps 0-1 trivial to revert, 2-3 semantic commit, 4-7 machine quiets, 8 last hole) | section 6, "Sequencing overlay" |
| 5 | fable §3 | LANG.md:15-16 (external/register died; bind is the unbundled survivor) as evidence against a new keyword | section 3.2, after Fork C |
| 6 | flash §1 | final/2 precision correction: expectation filters an always-fully-computed union (engine.pl:604-608); "asserts the union of ALL rels" wording imprecise | section 1.6 |
| 7 | flash §5 | "byte-identity is per surviving STATEMENT, not per module" | section 5 opening |
| 8 | flash | rows(rel) pull-API meaning change (3_engine.ts:127-134 returns live rows for an unqueried rel today; empty under pruning) | section 1.10 |
| 9 | flash §7 | UNKNOWN convention: unresolved items say UNKNOWN inline | applied in sections 1.6, 1.10, 3.3, 7 |
| 10 | adjudicated | REPLACED base §3.3-3.4 entirely with three-spellings section (A compiles clean; B REFUSED `clock_path_conflict(pre_commit, gate_fire, 0, 1)` via `3_clock_check.pl:129-138`; C compiles clean; receipts adjudicate.pl/sample.pl; two distinct hazards (a) silent latch D-fork, (b) two-offsets C-fork) | section 3.3 |

## Reset fork

Every claim that BASE framed as "the user's ruling" about reset behavior was
rewritten to an OPEN FORK (never-reset / rx-default reset-on-refcount-zero /
per-rel declaration), unruled, no recommendation. The `share()` defect is
kept and reframed on its own merits (3_engine.ts:112, finalize at :104-111,
submit at :116-124, masked by permanent subscription at 4_http.ts:164,
prior outage at 3_engine.ts:180-193). Ladder step 5 presents the narrow fork
"should running depend on ticks$ subscription at all" and does not prescribe
`resetOnRefCountZero:false` as settled. Affected: sections 1.10, 2.3, 2.4,
3.3, 6 step 5, 7 question 6.

## Kill list (0 occurrences in the fused pair)

Each item verified with grep returning no hits across both output files:

```
# wrong-form struck receipts
rg -n "1030-1063|:757|exactly one thing|stmt_counter" IMPACT.fused.md IMPACT.fused.visual.human.unga.md       # 0
# general "canonical ruled composition, written naively, REFUSED" claim
rg -n "written naively" IMPACT.fused.md IMPACT.fused.visual.human.unga.md                                      # 0
# edge_departure / sampled current sets as +1 mechanism
rg -n "edge_departure|sampled current" IMPACT.fused.md IMPACT.fused.visual.human.unga.md                       # 0
# "2 term fixtures with query(" (ladder bills 5)
rg -n "2 term fixtures" IMPACT.fused.md IMPACT.fused.visual.human.unga.md                                     # 0
# flash §3.3 dl program (bare 0-arity atoms, interval(1000,...), after_hook, first_time)
rg -n "1000-second|after_hook|undeclared first_time" IMPACT.fused.md IMPACT.fused.visual.human.unga.md        # 0
# event_source(pre_commit). untyped decl surface
rg -n "event_source\(pre_commit\)" IMPACT.fused.md IMPACT.fused.visual.human.unga.md                          # 0
# bare labs/rel_as_stream path
rg -n "labs/rel_as_stream|rel_as_stream" IMPACT.fused.md IMPACT.fused.visual.human.unga.md                    # 0
# "share-no-reset is the user's ruling" phrasing
rg -n "share-no-reset is the|share-no-reset ruling" IMPACT.fused.md IMPACT.fused.visual.human.unga.md         # 0
```

Note: the kill list is clean including the multiple-refs triple-cite range
`:104-107` retained only as the corrected single `body_ref_uses/2` clause
range (with `program_refs/2` at `:231` and `derived_refs/2` at `:80`
separated), so no wrong-form triple remains.

## Style laws (both docs)

Banned words (provenance, substrate, load-bearing, regime): 0 occurrences.
Em dashes: 0. rxjs/prolog/SQL vocabulary only. dl snippets carry their rx
lowering and descriptive variable names. IMPACT.fused.visual.human.unga.md
uses plain words, ascii diagrams, and contains zero citations/file paths/code
blocks beyond single `?` query lines (verified via grep).

## Output line counts

| file | lines |
|---|---|
| IMPACT.fused.md | 1106 |
| IMPACT.fused.visual.human.unga.md | 232 |
| total | 1338 |
