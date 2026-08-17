---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: done
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:med
blocked_by: ['@extract-df-aux-fields-lits']
closed: 2026-08-16
closed_by: extract-driver
---

# df aux: loop_over, allocates and nest

## Description

## Description

The graph-shaped half of v5's df aux: `loop_over`, `allocates`, `nest`. Unlike
fields/lits these are not labels — `nest(call_id, loop_id, depth, collection)`
is what turns `call_edge` into symbolic Big-O.

## Receipts

| fact | receipt |
|---|---|
| v5 rels + semantics | `src/engine/family/mod.rs:455-491` (`loop_over` = each loop's span + variable; `allocates` = fns whose body builds a collection; `nest` = each call's enclosing loop nest, depth + collection) |
| v6 aux | `v6/sprefa-extract/src/types.rs:540-543` — none of the three exist |
| v6 has the node kind already | `src/types.rs:560` `DfNodeKind::Loop` |
| deferral note | `lang/ts.rs:1697` ("the loop FACT is deferred aux"), `src/types.rs:1836` |

Blocked by @extract-df-aux-fields-lits: same aux struct, same four lang files.

## Fix shape

`DfLoop`, `DfAllocates`, `DfNest` on `DfFAux`; three `FlatFact` arms + three
SCHEMA lines; per-lang emission at the loop/call walks that already run.
`nest` depth is computed from the walk's own loop stack, never a second traversal.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test 2_df_aux_cli
cargo test --features cli --test golden_parity
```

## Comments

### 2026-08-16T17:29:18Z · @extract-closeout-driver

NO GRADING PATH TODAY. The committed v5 oracle baselines carry zero loop_over, nest and allocates rows. Facet census over v6/sprefa-extract/tests/fixtures/*/*.v5.jsonl: df_node 267, df_edge 210, type_node 73, call_def 48, df_args 44, df_param_pos 42, type_edge 41, type_sig 35, call_site 28, doc 19, const_value 14, df_lits 8, df_fields 8. The v5 crate is not in this crate's build graph, so no oracle regenerates from here. This arc needs a hand-written expectation or a decision to leave it ungraded; do not dispatch it as a byte-parity port.

### 2026-08-17T03:07:49Z · @extract-driver

DISPATCHED by extract-driver 2026-08-16 as a native opus lane in worktree .boop-worktrees/feature/extract-df-aux-loops-nests, branch feature/extract-df-aux-loops-nests, base origin/main 4531b4297. Brief: TASKS/extract-df-aux-loops-nests.BRIEF.md. Two findings that changed the shape from the card body. (1) THE GRADING PATH IS A NEW FIXTURE, not the goldens: the four golden fixtures contain ZERO loops (tests/fixtures/{ts/sample.ts,rust/sample.rs,go/sample.go,kotlin/sample.kt}, 44/35/35/40 lines, no for/while outside comments), so the lane adds tests/fixtures/df_loops/ plus tests/23_df_aux_loops_nests.rs with hand-derived expectations on the tests/16_python.rs convention. Editing a golden fixture would shift every span in its v5 baseline. (2) THE AUX ROW MUST NOT RIDE A LOOP NODE: which langs mint a DfNodeKind::Loop df NODE is v5-parity-pinned and already correct (rust src/lang/rust.rs:1815,1847,1879 and go src/lang/go.rs:1248 do; ts and kotlin do NOT, and src/lang/kotlin.rs:760-761 states why), so DfLoop carries its own span and adding a node would shift every df index and break golden_parity, 2_df_aux_cli and 4_capability_parity at once. Also pinned in the brief: allocates is RUST-ONLY in v5 (allocators is inserted only at src/graph/typegraph/rust/mod.rs:1149,1176; zero other sites), and the v6 nest post-pass uses byte-span containment, which subsumes v5's fn_sym plus ::closure:: ancestry test at src/graph/typegraph/mod.rs:876-884.

### 2026-08-17T03:34:53Z · @extract-driver

IN FLIGHT status at origin/main 55adcb6a9. Branch feature/extract-df-aux-loops-nests, HEAD a1ca01515, two commits landed: 8df4f0343 (DfLoop/DfNest/DfAllocates wire types at src/types.rs:626/:637/:648, aux vecs :661-663, compute_nests :679, schema block, flatten arms) and a1ca01515 (loop_over rows on the ts and go df walks). Working now: src/lang/rust.rs and src/lang/kotlin.rs modified, tests/fixtures/df_loops/ created untracked. Still outstanding: tests/23_df_aux_loops_nests.rs, the gate, the PR. FENCE BREAK CAUGHT AND CORRECTED: the rig ran a bare cargo fmt and reformatted the whole crate, 15 files outside its fence, all formatting-only (examples/typegraph_d2.rs, src/cpg_decode.rs, src/cpg_types.rs, src/lang/dl6/_0_source.rs, src/lang/prolog/_0_source.rs, src/lib.rs, tests/0_dl6.rs, tests/13_flow_join.rs, tests/19_docs_lang_arms.rs, tests/1_resolve_cli.rs, tests/9a_query_blob_door.rs, tests/golden_parity.rs, tree-sitter-dl6/src/lib.rs) plus three fmt-only hunks inside owned files (types.rs flow_edges ~:842, wire.rs flatten_project_type ~:270, go.rs walk_go_entities ~:126). Rig was given the exact revert and complied; git status is now clean of the churn. A clean git diff --stat against base is a BLOCKING condition on merge. ROOT CAUSE WORTH ITS OWN CARD, not filed by me: origin/main is NOT rustfmt-clean, proved by v6/sprefa-extract/src/wire.rs:270 where 'to_blab: edge.dst_blob.to_string(),' sits at column 0 inside an indented struct literal, so ANY lane running a bare cargo fmt reproduces this exact churn. Either the crate gets formatted once in its own chore PR or every brief carries a no-bare-fmt rule. REBASE PENDING: the rig was based at 4531b4297 and main has since moved to 55adcb6a9 (PR #328, rust module specifiers, which also edits src/lang/rust.rs). Rig instructed to rebase, keep BOTH name sets on the shared 'use crate::family::{...}' import line, stop and report if any other rust.rs conflict appears, and re-run the full gate after the rebase (suite on main is now 33 binaries / 133 passed / 0 failed).

### 2026-08-17T03:38:57Z · @extract-driver

LANDED: PR #329 merged to origin/main as 1e3ee473e. Six commits, rebased clean onto 55adcb6a9: 485c398f9 wire types/schema/flatten, 04145c5f8 ts+go loop rows, 4f2a7c312 rust loop rows + allocates, aaf12e3c0 kotlin loop rows, 326ab7af1 df_loops fixtures + tests/23_df_aux_loops_nests.rs, 41dca2c55 the four df module headers no longer list loops/nests/allocators as dropped. GRADED INDEPENDENTLY by extract-driver, nothing taken on the lane's word. (1) SCOPE: exactly the owned set, 12 files, +727/-29, worktree clean, the earlier crate-wide cargo fmt churn fully reverted, zero files outside the fence. (2) CONSTRAINT C1 HELD, verified three ways: zero new DfNodeKind::Loop pushes in the diff; per-lang Loop node counts unchanged at ts 0 / rust 3 / go 1 / kotlin 0; and no fixture outside tests/fixtures/df_loops/ was touched, so no golden moved. (3) STATUS TABLE VERIFIED TRUE AGAINST CODE, not against itself: loops/nests [x] on all four is backed by compute_nests call sites at ts.rs:1714, rust.rs:1470, go.rs:764, kotlin.rs:783 plus one aux.loops.push arm per lang; allocates is rust-only, backed by aux.allocates appearing only in rust.rs; lits stays ts+rust, backed by aux.lits.push counts of ts 2 / rust 1 / go 0 / kotlin 0. (4) ROW COUNTS RECOMPUTED BY ME through the built binary, not read off the test: sample.rs 3 loops / 7 nests / 1 allocates, sample.ts and sample.go and sample.kt each 3 loops / 7 nests / 0 allocates. (5) GATES run by me in the lane worktree: build rc=0; cargo test --features cli = 34 binaries / 137 passed / 0 failed; tests/23_df_aux_loops_nests.rs run THREE times, 4 passed each; 2_df_aux_cli 1 passed; golden_parity 9 passed; 4_capability_parity 2 passed; 24_rust_specifiers 2 passed. (6) STYLE on added lines: 0 em dashes, 0 eprintln in src, 0 banned words. FLAKE LOGGED, not mine: 8_scip_families_cli::an_indexer_past_its_budget_is_killed_with_its_whole_process_group failed once under lane load then passed in isolation and in both post-rebase whole-gate runs; load-sensitive process-budget test. Closing.

### 2026-08-17T03:39:14Z · @extract-driver

TWO DESIGN CALLS LANDED IN 1e3ee473e, recorded so nobody rediscovers them as bugs. (A) KOTLIN LOOP VAR DIVERGES FROM V5 ON PURPOSE, and v5 is the wrong one. v5 at src/graph/typegraph/kotlin.rs:551-560 finds the loop variable by scanning for_statement's named children for the first node of kind simple_identifier. Under the tree-sitter-kotlin grammar the loop variable is wrapped in a variable_declaration node, so that scan skips it and lands on the COLLECTION: 'for (row in rows)' yields v5 var="rows". v5 also hardcodes collection to the empty string at :566. v6 reads the variable_declaration child for var and the first node after the 'in' keyword for collection (src/lang/kotlin.rs:1225-1230), giving var="row" collection="rows". I verified the grammar shape independently: the v6 arm's variable_declaration match is what makes the test's var assertion pass, which is only possible if the loop var sits under that node and therefore is invisible to v5's direct simple_identifier scan. NO ORACLE CONSTRAINS THIS FACET (the committed v5 baselines carry zero loop_over rows), so byte-parity with a v5 bug was never on the table, and the user decision 'TAKE THE CORRECT AND MOST CONSISTENT ONE EVEN IF IT MEANS MORE WORK' points the same way. Reversible in one arm if anyone wants the v5 shape back. (B) DfAllocates.owner IS CLAIMED, NOT THREADED. The brief allowed a STOP if plumbing an fn span through the rust df walk cost more than the facet was worth. Instead allocator call spans park on aux.allocator_hits and the innermost callable claims and truncates them via claim_allocator_hits (src/lang/rust.rs:1429,1455). The rust closure arm claims at its own span; project_df claims at def_span(ident, block), which is the SAME callable span CallF defs already use, so df_allocates.owner joins straight to a call_def row with no new key. This mirrors v5, where fn_sym is rebound to lam_sym inside a closure so the enclosing fn never sees the hit. Cost was one src param on project_df plus its single call site; no line of rust.rs outside the df walk arms was touched.




