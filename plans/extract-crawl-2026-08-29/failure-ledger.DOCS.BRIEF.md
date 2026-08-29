# Lane `chore-extract-failure-ledger` (glm53f, docs only): three failure-modes entries

Append entries 96, 97, 98 to `docs/failure-modes.md`, same five-field shape as
entries 93 to 95 (Incident, RCA, Fail-pre-fix, Rail, Entry). Every claim cites
a `path:line` or a PR number you read with `gh pr view N --json body,files`.
No numbers invented: a number comes from a PR body, a test name, or a file.

## First action
```
git merge --ff-only ed833559a99b9bb53976ab6acda60e3a71242b7a
```

## Entry 96: a corpus run split across processes lost cross-partition edges
Source: PR #560 body and `plans/extract-crawl-2026-08-29/go.REPORT.md` Fixes 3
and 4, `go.crawl.py` (the xargs line). Rail: the tests in
`v6/sprefa-extract/tests/62_go_module_plane.rs` named in #560 (caller-only
partition run). Entry: import recall 84.11% -> 100%, reachable 4,580 -> 4,833.

## Entry 97: own_blob picked the owner by HashMap iteration order
Source: `git log --oneline origin/main -i --grep=own_blob` gives:
7297fdfef extract go: the module plane (GoModuleIndex, resolved_import, import_resolve) (#558)
f1467e8cb own_blob: resolve identity rides the seam, fallback is deterministic (#555)
1de4d763b own_blob brief: base sha
Read that PR's body and diff of `v6/sprefa-extract/src/types.rs` (fn
`own_blob`). Rail: sorted ContentIds, max-count, tie -> None, membership by
binary_search. Cite the test the PR added.

## Entry 98: a bench normal form mismatch reported a 5.6% recall that was 45.3%
Source: `plans/extract-bench-2026-08-29/ORACLES.REPORT.md` sections 11 and 12,
`normalize.py`, PR #559 and #561 bodies. Two shapes: go call oracle named
`Type.Method` where ours emitted bare `Method`; ts module oracle (madge)
counted file-imports-file where ours emitted binding targets (the `--deps`
file_edge row, 2,010/2,011). Rail: `normalize.py` and the `.bare.tsv` files.

## Ownership
`docs/failure-modes.md` only. No `src/`, no `plans/`.

## Receipt
Push `chore/extract-failure-ledger`, `gh pr create --base main`, hail
`boop beep --no-wait --as chore-extract-failure-ledger sprefa-coordinator "ledger: PR #N, entries 96-98"`.
Laws: no em dashes, no words provenance/substrate/load-bearing/regime/refusal,
no "ground truth" (say oracle), no one-word sentences. Under 10 minutes total;
if a fact is missing write "not measured" rather than a guess.
