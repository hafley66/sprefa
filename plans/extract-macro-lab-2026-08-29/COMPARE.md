# mbe vs scip: macro invocation sites on rust-analyzer

Inputs: `mbe.macro_sites.tsv` (PR #553, in-process `macro_rules` expansion) and `scip.macro_sites.tsv` (PR #557 + path fix 098f3fe5a, scip occurrences inside macro invocation spans). Key: (path, start, end).

## Counts

| set | rows |
|---|---:|
| mbe | 1057 |
| scip | 878 |
| both, exact span | 33 |
| mbe rows overlapping a scip range | 33 |
| mbe only | 1024 |
| scip only | 845 |
| union | 1902 |
| files: mbe / scip / shared | 51 / 199 / 20 |

## Macro names, mbe only

| macro | sites |
|---|---:|
| w | 312 |
| not_supported | 75 |
| rustc_attr | 75 |
| wln | 62 |
| ungated | 55 |
| set | 49 |

## Macro names, scip only

| macro | sites |
|---|---:|
| assert_eq | 199 |
| assert | 159 |
| matches | 79 |
| format | 56 |
| try_default | 56 |
| write | 45 |

## Macro names, both

| macro | sites |
|---|---:|
| from_bytes | 9 |
| w | 9 |
| rtry | 5 |
| size_and_align_expr | 5 |
| wln | 5 |

## Reading

| fact | where |
|---|---|
| mbe sees only `macro_rules` defined in the same file; `format!`/`vec!`/`assert!`/derives are scip-only by construction | PLAN.md Option 1, fixtures f4-f6, f8 |
| scip sees any call the compiler resolved inside an invocation span, but needs the index build | PLAN.md Option 4 |
| the union is the number to carry; both arms stay | counts table |
