# Lane `bench-extract-ratchet` (opus): recall, precision and wall ratchets over the committed oracles

User word (2026-08-29): "diet-scip ratcheted as high as possible so it is
fast". `diet_scip` is plain `--resolve` (`src/project.rs:491`). Nothing
today fails when a PR lowers recall against an oracle or raises the wall.
Build the ratchet.

## First action
```
git merge --ff-only 2423127ad687a8773f8c2f76acc987f159e83d70
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```

## Deliverable
`plans/extract-bench-2026-08-29/ratchet.py` and `RATCHET.tsv`.
- `ratchet.py measure`: for each corpus in COMMON.md, ONE process
  `timeout 30 extract --resolve --family call,type --project-root <corpus> <files>`
  (go adds `--deps` for module rows, ORACLES.REPORT.md section 11), 3 runs,
  median wall and peak RSS (`/usr/bin/time -l`), normalise with
  `normalize.py`, then recall and precision against every oracle tsv present:
  `ts5.oracle.call.tsv`, `ts.madge.module.tsv`, `go.oracle.call.vta.bare.tsv`,
  `go.oracle.module.tsv`, `go.oracle.type.typedecl.tsv`, `rust.oracle.call.tsv`,
  `rust.oracle.type.typedecl.tsv`, and against `go.codeql2.call.tsv`,
  `ts.codeql2.call.tsv` (the tools to beat). Print one table.
- `ratchet.py check`: rc=1 if any recall or precision is below RATCHET.tsv
  by more than 0.1 point, or any wall is above its row by more than 15%, or
  RSS above by more than 10%. Prints the offending rows.
- `ratchet.py bump`: rewrites RATCHET.tsv to the measured values only where
  they improved (additive ratchet, never lowers a floor or raises a ceiling
  without `--force`).
- Columns: `lang family oracle recall precision wall_ms rss_mb measured_at_sha`.
- `just extract-ratchet` in `v6/justfile` (read it first, follow its style)
  runs `check`. Add the leg to `.github/CI-KNOWN-RED.md` ONLY if it cannot
  run in CI (the corpora are local paths; say so in one line in COMMON.md
  and mark the recipe local-only).
- If a corpus is absent the row reads `absent` and `check` skips it with a
  printed line, rc stays 0 for that row.

## Ownership
`plans/extract-bench-2026-08-29/ratchet.py`, `RATCHET.tsv`, `COMMON.md`
(one paragraph), `v6/justfile` (one recipe). No `src/`.

## Receipt
Commit RATCHET.tsv at the measured values of this sha. Push
`bench/extract-ratchet`, `gh pr create --base main`, hail
`boop beep --no-wait --as bench-extract-ratchet sprefa-coordinator "ratchet: PR #N, go call x% ts call y% rust call z%, walls a/b/c ms"`.
Laws: no em dashes, no words provenance/substrate/load-bearing/regime,
never "ground truth" (say oracle), every extract call under timeout 30.
