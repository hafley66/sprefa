# Lane `feat-extract-corpus-stats` (glm53f): one script, any language, N repos, stats + sha

User 2026-08-31: "a generic script that runs per language on a bunch of repos
and collects stats and the sha it was testing/reading."

## Starting point (read first, then generalize)
`plans/extract-bench-2026-08-29/rust-corpora/run_repo.sh` (PR #606) does this
for rust with an oracle. This lane builds the language-generic version WITHOUT
requiring an oracle: stats collection first, oracle hookup optional.

## Ownership
- NEW `plans/extract-bench-2026-08-29/corpus-stats/` : `run.sh` (or
  `run.py`, stdlib argparse — pick ONE language for the whole tool),
  `REPOS.tsv` (name, url, lang, pinned sha), `STATS.tsv` output committed,
  `CORPUS-STATS.md` (usage doc, 20 lines max)
- APPEND a short section to plans/extract-crawl-2026-08-29/rust.REPORT.md?
  NO — this is infrastructure; document only in CORPUS-STATS.md.
- FORBIDDEN: src/**, tests/**, RATCHET.tsv, justfile.

## The contract
`run.sh --lang <go|ts|rust|python> --repos <REPOS.tsv> [--arm diet|checker]`
Per repo: shallow-clone to ~/corpora/<name> if absent (else reuse; ALWAYS
re-resolve and record `git rev-parse HEAD`), select the language's file set
(document the glob per language), run `extract --resolve` (checker arm only
for rust, `--features cli,rust-checker` build), and append ONE row to
STATS.tsv:
  repo, lang, arm, sha, files #, loc, rows call #, rows type #, rows module #,
  unresolved #, wall s, peak rss MB, extract binary git describe
`/usr/bin/time -l` field 1 is REAL (field 3 is USER — the known trap).
Idempotent: rerunning replaces that repo+lang+arm row, never duplicates.

## Seed run (the receipt)
REPOS.tsv seeded with: go: ripgrep? NO — go repos: gin, hugo, caddy; ts:
zod, hono, express; python: flask, requests, click; rust: the 5 from #606
(reuse existing ~/corpora clones). Run all four languages, diet arm, plus
rust checker arm. STATS.tsv committed with every row filled. Caps: nice
-n 15, one repo at a time, 15 min / 4 GB per run; over-cap = record the
kill in the row (wall = CAP), continue.

## Laws
Commit after each language completes. PR to MAIN. Numbers carry units.
If a run surfaces a crash/defect, add a row to
plans/extract-bench-2026-08-29/OPEN-PROBLEMS.md in the same PR (repo rule).
Done/blocked: `boop beep --no-wait --as feat-extract-corpus-stats sprefa-coordinator "<one line>"`.
