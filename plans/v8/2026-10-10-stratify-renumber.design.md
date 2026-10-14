# `ryi stratify` renumbering: the corrected mapping

Status: design, nothing applied. Probe: `plans/v8/probes/2026-10-10-stratify-remap.py` over
`ryi stratify --from src/lib.rs --root . src/` (355 records, 4.2 s). Full mapping:
`plans/v8/probes/2026-10-10-stratify-remap.tsv` (119 moves).

## What ryi does now

`hafley-rs` `crates/sprefa-extract/src/0_stratify.rs:180-188`: prefix = the file's global depth,
zero-padded; `prefixed` (`:570`) glues it in front of the whole old name.

| old | ryi today |
|---|---|
| `src/_6_eval/_9b_ir_eval.rs` (depth 4) | `src/_6_eval/04__9b_ir_eval.rs` |

Two defects: the old `_N_` stays, and the number is global depth, not rank among siblings.

## The corrected algorithm

1. Read `stratum` records: `path -> depth`. Every SCC shares one depth (verified: the 12-file
   `_2_lower`/`_3_check` cycle is all depth 4).
2. Key every node. File: its depth. Directory: the max depth of any file under it.
3. Per directory, dense-rank its children by key: 0, 1, 2, ... with no gaps.
4. Siblings with the same rank: first keeps the bare number, the rest get `a`, `b`, `c` (the repo's
   `_5_derived` / `_5a_generic` shape). Order inside a tie: old number, then name.
5. Strip the old prefix `^_\d+[a-z]?_`, write `_{rank}{letter}_{stem}`.
6. `lib.rs`, `mod.rs`, `main.rs`, and the unreached `src/bin/` keep their names.
7. Apply with `ryi move --list <tsv>`; it rewrites `#[path]` attributes (`src/edit/_1_move_cx.rs:116`).
   `src/` carries 108 of them.

Worked example, `src/_2_lower/` (one directory, keys = depths):

| depth | cycle | old | new |
|---|---|---|---|
| 1 | | `_1_forms.rs` | `_0_forms.rs` |
| 1 | | `_3_origins.rs` | `_0a_origins.rs` |
| 1 | | `_9_kernel.rs` | `_0b_kernel.rs` |
| 1 | | `_10_index.rs` | `_0c_index.rs` |
| 2 | | `_0_cx.rs` | `_1_cx.rs` |
| 3 | | `_1_slots.rs` | `_2_slots.rs` |
| 3 | | `_4_promote.rs` | `_2a_promote.rs` |
| 4 | C16 | `_0_api.rs` | `_3_api.rs` |
| 4 | C16 | `_2_declare.rs` | `_3a_declare.rs` |
| 4 | C16 | `_5_derived.rs` ... `_8_express.rs` | `_3b_derived.rs` ... `_3g_express.rs` |
| 5 | | `_11_json.rs` | `_4_json.rs` |
| 5 | | `_13_keep.rs` | `_4a_keep.rs` |
| 6 | | `_12_units.rs` | `_5_units.rs` |

A cycle's members all land on one number with letters; the letters carry no order.

## Top level: dependency order is not pipeline order

| today | key | dependency rank |
|---|---|---|
| `_7_effect.rs` | 0 | `_0_effect.rs` |
| `_0_read` | 5 | `_1_read` |
| `_6_eval` | 6 | `_2_eval` |
| `_9_runtime` | 6 | `_2a_runtime` |
| `_2_lower` | 7 | `_3_lower` |
| `_3_check` | 7 | `_3a_check` |
| `_5_reify` | 9 | `_4_reify` |
| `_4_comptime` | 11 | `_5_comptime` |
| `_8_driver` | 11 | `_5a_driver` |
| `_1_macrotime` | 13 | `_6_macrotime` |

Today's top-level numbers follow the pipeline (read, macrotime, lower, check, ...). Dependency
order moves `_6_eval` to second and `_1_macrotime` to last, because macrotime calls eval.

## Forks for Chris

1. Scope. (a) Files only, directories keep their numbers: 92 renames, 16 unchanged.
   (b) Files and directories: 119 file moves, 11 directory renames.
2. Directory key. Max depth of its files (this probe) vs. a rank from directory-to-directory
   import edges. The max is a proxy; the edges need `ryi fast` `resolved_import` rows.
3. Cycles. Letter them as one rank (this probe), or break the 5 cycles first so every file gets
   its own number. The 20-file cycle is `_6_eval` engines plus every `_9_runtime/_3_executors/*`.
4. Who fixes `stratify`: ryi. Filed as hafley-rs issue `stratify-prefix-stacking`; the probe
   script is the reference.
