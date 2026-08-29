# Lane `bench-extract-tools` (glm53f): third-party static-analysis tools on the same corpora

Read `plans/extract-bench-2026-08-29/COMMON.md` first. Build-vs-buy law:
every tool gets a real run or a cited reason it cannot run here; no one-line
dismissals.

## Tools, in this order, each with install command, version, wall, disk
| tool | lang | family | how to get edges |
|---|---|---|---|
| madge (installed, `which madge`) | ts | module | `madge --json <root>` on TypeScript-5.9/src; v5 used it as the module oracle (grep `madge` in `DEVLOG.md`, `v6/sprefa-extract/src/deps.rs`, `tests/7_diet_deps_cli.rs`) |
| dependency-cruiser | ts | module | `npx depcruise --output-type json` |
| codeql CLI | ts, go, rust(beta) | call, module | `codeql database create` per language then a `.ql` per family printing caller/callee locations; ts and go first, rust only if the beta pack installs |
| GitHub stack-graphs (`tree-sitter-stack-graphs-typescript`) | ts | module + name binding | cargo install, run over src, dump definitions/references |
| glean | any via scip | all | glean consumes scip (`glean-scip`); state install cost on macOS; if it needs a Linux/docker build over 30 min, record that and skip |
| joern | go, ts | call | `joern-parse` + a scala query dumping call edges; record if the JVM install exceeds 15 min |
| kythe | go | all | record install cost; skip if bazel is required |
Emit the normal-form tsv for every run that succeeds.

## Ownership
Only `plans/extract-bench-2026-08-29/**` (`TOOLS.REPORT.md`, `<lang>.<tool>.<family>.tsv`, tool scripts under `tools/`). No `src/` edits. Do not write `bench.py`; the sibling lane owns it, pull it from `origin/bench/extract-oracles` when it appears, else write your comparison inline in the report.

## Receipt
`TOOLS.REPORT.md` opens with a TOC, then one table per language: tool, family, edges, overlap with our parse resolve tsv (run `extract --resolve` yourself and normalize), wall, install cost, ran yes/no with the exact error when no. Push a branch `bench/extract-tools`, `gh pr create --base main`, hail `boop beep --no-wait --as bench-extract-tools sprefa-coordinator "tools: PR #N, <k> of 7 tools ran, madge module edges <n> vs ours <m>"`.
