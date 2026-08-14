# 2026-08-14 recursion fixtures report

Certifies two self-referential spellings with conformance fixtures. No compiler
source touched. Verdicts: all fixtures pass, both cyclic probes store and
render (no loop). One gate blocked by environment (RUST-GRADE).

## TOC
- [1. Verdict table](#1-verdict-table)
- [2. Cyclic probe transcript](#2-cyclic-probe-transcript)
- [3. Gate numbers](#3-gate-numbers)
- [4. Findings](#4-findings)

## 1. Verdict table

| fixture | expectation | observed | verdict |
|---|---|---|---|
| `recursive_enum_acyclic_tree_round_trips` (17) | two leaves then a branch referencing them by id; finals on `tree_leaf/2`, `tree_branch/3`, `tree_tag/2`, derived `tree_kind/2`; retract one leaf and assert deltas | branch arrives as the flat `tree_branch(Id, Left, Right)` 3-column row (left/right are the referenced instance ids); tag join and derived kind track every retraction | pass |
| `recursive_enum_cyclic_values_store_and_render` (17) | a branch whose left is its own id, and a two-row mutual cycle, store and render | both cycles terminate on the same tick; tag and kind derived for every id; no loop | pass |
| `recursive_list_arg_parent_holds_child_node_values` (18) | `node(name: text, children: list(node))`; a parent whose children list holds child node values | minted `__gen__list_node_4205b0871c875897__member(list_id, idx, value)` with value column typed `node`; the nested child node value normalizes into its own `node` row plus the member row | pass |

All three fixtures grade PASS in the conformance gate (see gate numbers).

## 2. Cyclic probe transcript

Probed through `dl6_oracle.pl` on `rel tree(leaf(value: int) ; branch(left: tree,
right: tree)).`, each run under `gtimeout 60`. Both cases terminate immediately
(exit 0) and are stored-and-rendered, not a loop.

Self-cycle (branch id 9, left = its own id):

```
{"tick":1,"deltas":{"tree_leaf":{"add":[[1,5]],"del":[]},"tree_tag":{"add":[[1,"leaf"]],"del":[]}}}
{"tick":2,"deltas":{"tree_branch":{"add":[[9,9,1]],"del":[]},"tree_tag":{"add":[[9,"branch"]],"del":[]}}}
```

Two-row mutual cycle (branch 2 left = branch 3, branch 3 left = branch 2):

```
{"tick":1,"deltas":{"tree_leaf":{"add":[[1,5]],"del":[]},"tree_tag":{"add":[[1,"leaf"]],"del":[]}}}
{"tick":2,"deltas":{"tree_branch":{"add":[[2,3,1]],"del":[]},"tree_tag":{"add":[[2,"branch"]],"del":[]}}}
{"tick":3,"deltas":{"tree_branch":{"add":[[3,2,1]],"del":[]},"tree_tag":{"add":[[3,"branch"]],"del":[]}}}
```

The enum's tag join never recurses into the left/right id fields, so a cycle is
stored as ordinary rows. Behavior is stable, so the cyclic case is pinned by the
`recursive_enum_cyclic_values_store_and_render` fixture rather than left to the
report alone.

## 3. Gate numbers

| gate | command | result |
|---|---|---|
| conformance | `cd v6/prolog/conformance && swipl -g go -t halt go.pl` | 424 PASS / 0 FAIL (baseline 421) |
| sweep | `cd v6/tsv2 && bash scripts/sweep.sh` | RUN total=320 identical=317 wrong=0 rejection=3; FINAL wrong=0 |
| plunit | `cd v6/prolog && swipl -q -l compile/test/plunit_tests.pl -g run_tests -g halt` | 5 failed, exactly the `.github/CI-KNOWN-RED.md` plunit set |
| RUST-GRADE | `bash v6/sprefa-engine-rs/grade.sh` | blocked, see findings |

## 4. Findings

| finding | detail |
|---|---|
| RUST-GRADE gate blocked by environment | `grade.sh` fails at `cargo build` before grading: `failed to read .../sprefa-v6/0_runtime/1_rust_runtime_host/Cargo.toml` (no such file). The dependency path `../../../sprefa-v6/0_runtime/1_rust_runtime_host` in `v6/sprefa-engine-rs/Cargo.toml` resolves to `.boop-worktrees/sprefa-v6`, a sibling checkout that does not exist in this worktree. The runtime host lives at `~/projects/sprefa-v6/0_runtime/1_rust_runtime_host`, reachable only from a checkout sharing `~/projects` as its grandparent. Committed `v6/sprefa-engine-rs/graded.tsv` still reads graded=421 byte-clean=313; no grade run happened. Fixing the path or creating the sibling checkout is outside this lane's file ownership. |
