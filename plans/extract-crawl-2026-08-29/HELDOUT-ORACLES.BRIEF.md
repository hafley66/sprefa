# Lane `lab/extract-heldout-oracles` (opus): measure us against oracles on repos nobody tuned against

First action: `git merge --ff-only 89080a58777e1e625766dc58dd02317f41aac077`. Failure or missing tree = STOP and beep the coordinator.

## Goal, one sentence

Build a repeatable harness that picks REAL large projects at random from
several language ecosystems, scores our extractor against that language's own
indexer on each, and reports the OVERFIT GAP: how far the held-out numbers sit
below the three tuning corpora we have been grinding against since 2026-08-29.

## The user's words

"keep running random tests of us vs oracles for random ass checkout of large
common projects in a language eco"

## Why this matters (zero-context reader)

Every accuracy number this repo quotes comes from exactly three checkouts:
TypeScript-5.9, typescript-go, and rust-analyzer
(`plans/extract-bench-2026-08-29/COMMON.md:7-12`). Eighteen ratchet floors in
`plans/extract-bench-2026-08-29/RATCHET.tsv` are measured on those three and
nothing else. Dozens of resolver fixes landed by grinding against them
(PRs #624 through #633).

Nobody knows whether the resolvers generalise or whether they have learned
those three repos. The user has raised overfitting repeatedly; this is arc A
of `plans/extract-eval-2026-08-31/BENCH-RUNTIME.PLAN.md` and section 9 of
`plans/extract-eval-2026-08-31/PLAN.md`.

## The oracle, and why it is cheap

`extract --family scip ROOT` builds or reuses the language's OWN compiler index
and streams it as compiler-resolved facts. That is a real oracle, it needs no
CodeQL database, and the indexer roster is already wired
(`v6/sprefa-extract/src/scip.rs`):

| lang | indexer | status on this machine |
|---|---|---|
| ts | `scip-typescript` | on PATH |
| python | `scip-python` | on PATH |
| rust | `rust-analyzer` | on PATH |
| go | `scip-go` | NOT on PATH; `scip.rs:279` has a `go run` fallback pinned to v0.2.7, so it should still work. VERIFY this rather than assume it. |

A root that cannot be indexed emits `scip_skip` rows naming the reason and
exits 0. Treat a skip as a SKIPPED measurement, never as a zero.

## Repo selection: random, but reproducible

A "random" selection nobody can reproduce is not a measurement. Rules:

- Use `gh search repos` to build the candidate pool per language: at least 200
  stars, not archived, not a fork, size between 5 MB and 200 MB. Write the
  full candidate list to a committed file with the exact query and the date.
- Pick from that pool with a SEEDED shuffle. Record the seed. Someone must be
  able to rerun your selection and get the same repos.
- Exclude the three tuning corpora by name, and exclude anything under
  `hafley66`.
- Target 3 repos per language for the first batch: ts, go, rust, python.
  12 repos. If a language yields fewer, say so and move on.
- `git clone --depth 1`. Delete each checkout after it is scored; disk floor
  is 15G free and you check `df` before every clone. There is 295G free right
  now, and it took real work to get there, so do not leave 12 checkouts behind.

## Per repo, what to measure

Two rows per repo, syntax tier and checker tier where the checker exists
(rust needs `--features rust-checker`, ts needs `ts-checker`; go and python
have no checker tier).

Use the 4-column normal form the whole lab already uses,
`src_path src_name dst_path dst_name`, paths relative to the repo root
(`COMMON.md:23-28`). The scorer is the same protocol as
`v6/sprefa-extract/tests/bench/mod.rs`: recall = overlap / |oracle|,
precision = overlap / |ours|, plus the 3-bucket split
(matched / contradicted / unjudged, `mod.rs:656`).

**PASS ABSOLUTE PATHS TO `--resolve`.** `docs/failure-modes.md` 106, landed
today: relative paths silently drop crate-root import edges. Absolute is a
strict superset. If you pass relative paths every number you produce is low.

## The deliverable

Three files.

1. `plans/extract-eval-2026-08-31/heldout/run.py` (or `.sh`) — the harness.
   Takes a seed and a language, does selection, clone, both tiers, scoring,
   cleanup. Rerunnable. No number hardcoded in it.
2. `plans/extract-eval-2026-08-31/heldout/SCORES.tsv` — one row per
   (repo, lang, family, tier, oracle) with recall, precision, matched,
   contradicted, unjudged, ours, oracle, wall_ms, and the repo's commit sha.
   Include a `CATEGORY:` column or none at all; if you write rollup rows,
   `plans/extract-bench-2026-08-29/` got burned by summing them by accident,
   so make rollups impossible to confuse with data rows.
3. `plans/extract-eval-2026-08-31/heldout/REPORT.md` — the finding.

The REPORT must lead with the overfit gap as a table:

```
lang.family.tier   tuning corpus   held-out median   gap
```

against the committed `RATCHET.tsv` rows. Then per-repo detail, then a section
naming every repo that SKIPPED and exactly why.

## Laws in force

- Every accuracy number carries the full measure signature of `COMMON.md:58`:
  `<lang>.<family>.<tier>` vs `<oracle>` on `<corpus, n files>`, metric =
  numerator/denominator row unit. A percent with no fraction beside it is
  banned.
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`. The
  word is "oracle", never "ground truth".
- Descriptive names, never single-letter.
- Doubt yourself before asserting. Every claim in this brief is the
  coordinator's; verify against the code.
- 10-second law, waiting posture: nothing foreground over 10s. Indexer builds
  are the NAMED exception and run in the background with a log and a budget.
- `df` before every clone, floor 15G.
- Never `--no-verify`. A blocked command ends the approach.
- **Do not end your turn with an uncommitted deliverable.** Four lanes did that
  today. Poll your background work in a loop; a backgrounded job is not a
  finished job.
- You are a lane: never spawn a subagent.

## Reaching the coordinator

```
boop beep --no-wait --as lab-extract-heldout-oracles sprefa-coordinator "<one line>"
```
