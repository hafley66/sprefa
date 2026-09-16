# plan-prolog-split (PLAN lane, opus; a coordinator review follows)

You are lane `plan-prolog-split`. Coordinator is `sprefa-coordinator`.
Base sha 9e4b468157bb2a189960b8ec69daad10af372862. Branch `plan/prolog-split`.
FIRST ACTION: `git merge --ff-only 9e4b468157bb2a189960b8ec69daad10af372862`; on failure STOP and hail.

## Goal
A refactor PLAN (no code) that breaks every large v6/prolog file into parts in the hundreds of lines, never thousands, using the SAME mechanism `0_generic_expand.pl` already uses. Chris's words: "keep file name as folder name or keep filename and re-export from that folder, we want smaller files in the hundreds not thousands scale of loc".

## The precedent, copy it exactly
`v6/prolog/0_generic_expand.pl` keeps `:- module(generic_expand, [...exports...])` plus the `use_module` lines, then `:- include('0_generic_expand/0_expand.pl')` ... `:- include('0_generic_expand/8a_key_wrappers.pl')` (lines 50-72). The parts live in a folder named after the file, numbered by pipeline order (`0_expand`, `0a_type_apply_requests`, `0b_expansion_pipeline`, `1_annotations`, ... `8a_key_wrappers`). Commit that did it: b5c5effa0. Read that diff first.

## Targets (wc -l on base sha)
| file | lines |
|---|---:|
| lower.pl | 7795 |
| emit_ts.pl | 2786 (paused door, user 2026-08-21: output must stay byte-identical; plan the split, mark it lowest priority) |
| analyze.pl | 1891 |
| compile/parse_dl_dcg.pl | 1776 |
| 0_type_plane.pl | 1037 |
| ARCH.pl | 1026 (data, not code: task/5 + fork/5 rows; propose a split by arc family or say why not) |
| compile.pl | 997 |
| 0_program_check.pl | 985 |
| print_dl.pl | 905 |
| 0_dot_expand.pl | 835 |
Anything else over 600 lines: list it with a one-line verdict.

## For EACH target the plan states
1. Part list: folder name, each part file name (numbered), its line range in the current file, its line count, and one sentence on what the part owns. Parts are cut at predicate-family boundaries; a predicate never straddles two parts. No part over ~700 lines; target 200-500.
2. The exact head the module file keeps (module decl, exports, use_module lines, the include list).
3. Cross-part dependencies: which parts call predicates defined in which other parts. `include` makes them one module so nothing is exported between parts, but the reader needs the map. Produce it with a script (grep predicate heads per part, grep calls per part) and check the script's output into the plan folder as a receipt.
4. Discontiguous risk: SWI warns when clauses of one predicate are split across parts; list every predicate whose clauses would be discontiguous under your cut and either move them together or state the `:- discontiguous` directive already present.
5. Receipt: the split is byte-identical in behavior. Name the exact gates: `cd v6/prolog/conformance && swipl -g go -t halt go.pl` (445/0), `cd v6 && just plunit` (1115/0), `bash v6/sprefa-engine-rs/grade.sh` (445/341), `swipl -g go -t halt v6/prolog/ARCH.pl` (7/0), and for emit_ts.pl the sweep digests `v6/prolog/compile/out/sweep.digests` unchanged. Plus the oracle digests `compile/out/oracle.digests` unchanged.

## Ordering and collision
- A codex lane holds `/private/tmp/sprefa-temporal-v2` (branch feature/temporal-relations-v2) with dirty: 0_compiler_relations.pl, 0_generic_expand.pl, 0_generic_expand/0_expand.pl, 2_compiler_plane.pl, 5_type_freeze.pl, 0_unsupported_messages.pl, compile/test/4_braced_nested_relations.test.pl, plunit_tests.pl, typegen_golden.sh. Your plan sequences every split of a file in that list AFTER temporal-v2 merges and says so per file.
- Chris's main tree has uncommitted work in lower.pl and 0_generic_expand.pl. The lower.pl split lands only after that commits; say so.
- Propose an order: smallest-risk first, lower.pl last, one PR per file, each PR byte-identical on every gate above.

## Deliverables (two docs, both required, in `plans/`)
- `plans/2026-08-24-prolog-split.PLAN.md`: TOC first, then the per-file sections above, citations as `file:line`, the dependency-map script under `plans/2026-08-24-prolog-split/` with its output.
- `plans/2026-08-24-prolog-split.PLAN.visual.human.unga.md`: plain words, one mermaid per target showing parts and the call edges between them, zero citations, for Chris.
Commit both, push branch, post a DRAFT PR titled `plan: split the large prolog files into hundreds-scale parts`.

## Yield results over time
Hail after the first target (parse_dl_dcg.pl) is planned, after lower.pl is planned, and at done with the PR number:
`boop beep hail sprefa-coordinator --from plan-prolog-split --body "<one line>"`.
STOP and hail on: a file that cannot be cut at predicate boundaries under 700 lines, or a gate that is not green on the base sha when you measure it.

## You own
`plans/2026-08-24-prolog-split.PLAN.md`, `plans/2026-08-24-prolog-split.PLAN.visual.human.unga.md`, `plans/2026-08-24-prolog-split/**`. Forbidden: every file under v6/**. This is a plan; no code moves.

## Style laws (CLAUDE.md)
No em dashes. Banned words: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount), honest, grounded. rxjs/prolog/SQL vocabulary only. Tables and lists over prose. No narrative of what you tried.
