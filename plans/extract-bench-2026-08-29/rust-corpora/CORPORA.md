# rust-corpora: corpora, file rule, pinned shas

Five repos beyond rust-analyzer, cloned shallow 2026-08-31 into `~/corpora/`.
Every number in RESULTS.tsv was measured at the sha below (shallow clone,
`git rev-parse HEAD` at clone time). Per-repo oracle + ours tsvs sit beside
this file.

| repo | sha (pinned) | files | files rule hit |
|---|---|---:|---:|
| ripgrep | `3fce3b5bb0236da2df6d99672afb8a719642eca7` | 63 |
| tokio | `ea91b33ca57ff0581b38e735cc108f831bccbdaa` | 497 |
| serde | `a874a1b1bb1cc16cf5ee3b1b7b527af5705742bb` | 53 |
| clap | `6982fb1c98c7247e38a6d4f04191b94e30497e7b` | 119 |
| alacritty | `ede2ac144da4dec4c075bfa803aacf3b3739bce6` | 85 |

File rule (per repo, `<repo>.files.txt`): every tracked `.rs` file whose path
carries a `/src/` component (top-level `src/**` and per-member `src/**`
included, workspace members named `tests-integration` included; `target/`
pruned, sibling `tests/`/`benches/` trees excluded). The ratchet's
rust-analyzer rule (`crates/*/src` only) covers one layout; these five repos
use four different workspace layouts (top-level members, `crates/*`,
`<member>/src`), so the rule is the path-shaped generalization of it.

Recipe: oracle = `ra_ide_probe` (ra_ap_ide 0.0.349 call hierarchy,
`plans/extract-bench-2026-08-29/ra_ide_probe/`, built once) run per repo with
budget 900 s. Ours = release `extract` `--resolve --family call,type` (diet)
and the same plus `--rust-checker --project-root <root>` (checker), one run
each under `/usr/bin/time -l`, cap 900 s / 4 GB (all runs inside both caps).
normalize.py + the tests/bench/mod.rs rust projection (python port in
fuzzy_bench.py) + `fuzzy_bench.py --mode exact`. Checker-arm diet check:
record-kind census shows only `resolved_edge`, `resolved_import`,
`resolved_type_edge`, `unresolved` in all 5 checker raw runs (no
`scip_override`/`scip_macro` rows), so no run adopted a cached scip index.
