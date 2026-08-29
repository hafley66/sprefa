# mbe vs scip: macro invocation sites on rust-analyzer

Inputs: `mbe.macro_sites.tsv` (PR #553, in-process `macro_rules` expansion) and `scip.macro_sites.tsv` (PR #557, scip occurrences inside macro invocation spans). Key: (path, start, end).

## Counts

| set | rows |
|---|---:|
| mbe | 1057 |
| scip | 877 |
| both (same span) | 0 |
| mbe only | 1057 |
| scip only | 877 |
| union | 1934 |

## Macro names, mbe only (top 8)

| macro | sites |
|---|---:|
| w | 321 |
| not_supported | 75 |
| rustc_attr | 75 |
| wln | 67 |
| ungated | 55 |
| set | 49 |
| gated | 37 |
| size_and_align | 37 |

## Macro names, scip only (top 8)

| macro | sites |
|---|---:|
| assert_eq | 199 |
| assert | 159 |
| matches | 79 |
| try_default | 56 |
| format | 56 |
| from_bytes | 45 |
| write | 45 |
| vec | 45 |

## Macro names, both (top 8)

| macro | sites |
|---|---:|

## Files with sites in only one set (top 8 each)

| file | mbe | scip |
|---|---:|---:|
| crates/hir-expand/src/inert_attr_macro.rs | 168 | 0 |
| crates/mbe/src/tests.rs | 0 | 105 |
| crates/hir-ty/src/mir/eval.rs | 91 | 0 |
| crates/hir-ty/src/mir/pretty.rs | 82 | 0 |
| crates/rust-analyzer/src/global_state.rs | 0 | 68 |
| crates/project-model/src/workspace.rs | 0 | 64 |
| crates/rust-analyzer/src/config.rs | 51 | 0 |
| crates/hir-ty/src/method_resolution/probe.rs | 0 | 47 |
| crates/hir-def/src/item_tree/pretty.rs | 47 | 0 |
| crates/hir-ty/src/layout/tests.rs | 43 | 0 |
| crates/hir-expand/src/span_map.rs | 0 | 42 |
| crates/ide-db/src/path_transform.rs | 0 | 39 |
| crates/ide-completion/src/completions/attribute.rs | 37 | 0 |
| crates/hir-ty/src/next_solver/normalize.rs | 0 | 35 |
| crates/profile/src/stop_watch.rs | 0 | 29 |
| crates/span/src/map.rs | 0 | 29 |

## Reading

| fact | where |
|---|---|
| mbe sees only `macro_rules` defined in the same file; `format!`/`vec!`/`assert!`/derives are scip-only by construction | PLAN.md Option 1 fixture table f4-f6, f8 |
| scip sees any call the compiler resolved inside an invocation span, but needs the index build | PLAN.md Option 4 |
| the two sets are complementary, not competing: union is the number to carry | counts table above |
