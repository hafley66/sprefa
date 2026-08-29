# sprefa-extract corpus battery: kotlin, prolog, markdown (2026-08-28)

Run by the coordinator on `main` at `8e946ada9`; binary `v6/sprefa-extract/target/release/extract`.

## TOC
1. Corpora
2. Step 1: per-file default run
3. Perf
4. Resolve
5. Findings
6. Untested and why

## 1. Corpora
| lang | root | files |
|---|---|---|
| kotlin | square/okio + JetBrains/Exposed (shallow clones) | 1115 |
| prolog | `/opt/homebrew/lib/swipl/{library,boot}` | 496 |
| markdown | `~/projects/{sprefa,hafley-rs,instant,claude-research}/**/*.md`, first 3000 | 3000 |

## 2. Step 1: per-file default run (`kt.runs.tsv`, `pl.runs.tsv`, `md.runs.tsv`)
| lang | files | rc!=0 | timeouts (10s) | zero-output files | max serial ms (file, bytes) |
|---|---|---|---|---|---|
| kotlin | 1115 | 0 | 0 | 0 | 143 (`AbstractFileSystemTest.kt`, 83609) |
| prolog | 496 | 0 | 0 | 0 | 705 (`chr/chr_translate.pl`, 873753) |
| markdown | 3000 | 0 | 0 | 1 (a 0-byte file) | 262 (`docs/failure-modes.md`, 280024) |

TSV `ms` column was measured under `xargs -P 6` with stdout captured into a shell variable; the serial numbers above are the faithful ones.

## 3. Perf
Time by family, single process, stdout to a pipe:
| file | bytes | cst ms / lines | type ms / lines | call ms / lines | df ms / lines | all ms |
|---|---|---|---|---|---|---|
| `chr_translate.pl` | 873753 | 702 / 270865 | 101 / 2470 | 321 / 35890 | 123 / 16176 | 687 |
| `AbstractFileSystemTest.kt` | 83609 | 100 / 39757 | 45 / 195 | 69 / 2036 | 85 / 14995 | 124 |

- Parse is at most the `type` column (101 ms). CST JSON emission is 6x the parse on the prolog file. The default run's cost is the CST line volume, ~83% of all lines.
- Max RSS on `chr_translate.pl`: 166690816 bytes (`/usr/bin/time -l`).

## 4. Resolve
| target | files | rc | ms | resolved_edge | other records |
|---|---|---|---|---|---|
| okio commonMain | 41 | 0 | 779 | 399 | 0 |
| okio jvmMain | 41 | 0 | 372 | 551 | 0 |
| Exposed core | 104 | 0 | 3907 | 2859 | 0 |
| Exposed jdbc | 45 | 0 | 845 | 564 | 0 |
| swipl library+boot | 496 | 0 | 7619 | 39751 | 0 |

Exposed core scaling: n=25 445 ms, n=50 562 ms, n=100 700 ms (linear). `--resolve` never emits an unresolved record; the unresolved ratio is not measurable from the CLI.

## 5. Findings
| lang | class | where | repro | observed | expected |
|---|---|---|---|---|---|
| kotlin | missing_fact | `src/lang/kotlin.rs:773` (call sites only from `call_expression`) | `extract --family call tests/fixtures/kotlin/corpus_1_infix_operator.kt` | sites: `Box Box Box Box` | sites for `plus2` (infix), `plus` (`+`), `invoke` (`()`) |
| kotlin | exposure | corpus | `grep -rhoE '\b(infix\|operator) fun'` | 248 `infix fun`, 115 `operator fun` defs; ~6232 infix use sites (approx grep) | edges for every one |
| kotlin | missing_fact | `src/lang/kotlin.rs` type plane | `extract --family type probe/a.kt` | no node for top-level `val`, `companion object` | property node, object node |
| prolog | missing_fact | `src/lang/prolog/_0_source.rs:264-265` (only `once/1`, `catch/3` args are goals) | `extract --family call tests/fixtures/prolog/corpus_1_meta_closures.pl` | `double` under `maplist/3` and `call/3`: no reference, no site; goals under `forall/2`, `findall/3`: `term_arg` | call edge go/1 -> double/2 for all four |
| prolog | exposure | swipl library, 107396 sites | `pl.sites.txt` | maplist/2..5 604, findall/3,4 317, forall/2 235, call/1..8 251, foldl 42, include/exclude/partition 88, aggregate_all/setof/bagof 56, ignore 40; covered today: catch/3 474, once/1 46 | closure and goal-arg edges for all rows |
| markdown | missing_fact | CLI door, `src/types.rs:272` (`doc_ref` kind exists) | `extract --resolve --family type tests/fixtures/markdown/doc_node.md tests/fixtures/rust/*.rs tests/fixtures/ts/*.ts` | 0 `doc_ref` rows (only field/param/returns edges) | the `doc_ref` edges `tests/22_doc_node.rs` proves through the library API |
| markdown | missing_fact | `src/lang/markdown/_0_source.rs:143` | `extract --family type README.md` | `doc_node` kinds: heading 31, code_block 26 | link targets (`inline_link` has 35 cst nodes in README) as doc edges |
| all | perf | emit path | `--family cst` vs `--family type` | CST emission 6x parse | emission cost proportional to parse |

## 6. Untested and why
- `--family scip`: no Kotlin/Prolog/Markdown indexer exists in EXACT MODE (`extract --help` lists rust-analyzer, scip-typescript, scip-go).
- `--family diet_scip`: same records as `--resolve` for these arms; skipped.
- Kotlin `extract move` / rename: outside the fact-battery scope.

## 7. Fixes

| finding | before | after | test |
|---|---|---|---|
| row 1: call sites only from `call_expression`; no site for infix, operator, or invoke calls | `--family call tests/fixtures/kotlin/corpus_1_infix_operator.kt` -> sites `Box Box Box Box` | sites minted for `plus2` (infix_expression), `plus` (`+`), `invoke` (`f(x)()`), plus the full operator map (`*` times, `/` div, `%` rem, `..` rangeTo, `in` contains, `[]` get/set, `==`/`!=` equals, `<``>``<=``>=` compareTo, `+=` plusAssign family, unary `!`/`-`/`+`/`++`/`--`); each site spans the operator token or infix name | `tests/47_kotlin_operator_calls.rs` (red pre-fix: sites `[(289, 292, "Box"), (417, 420, "Box"), (426, 429, "Box"), (454, 457, "Box")]`; resolve test red: `resolved_edge to plus2 missing`) |
| row 1 `--resolve` COUNT, okio commonMain (41 `.kt` under `okio/src/commonMain/kotlin/okio`, shallow clone) | 399 resolved_edge (reproduced pre-fix binary) | 698 resolved_edge | receipt: `extract --resolve $(find okio -name '*.kt')` before 399 / after 698 |

Whole-crate gate after the fix: `cargo test --features cli` -> 297 passed, 0 failed.
