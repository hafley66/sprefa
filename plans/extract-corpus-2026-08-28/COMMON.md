# Common contract: sprefa-extract corpus battery (2026-08-28)

## Mission
Run the `extract` binary over a large real-world corpus in ONE language and
record every gap: crashes, non-zero exits, parse errors, wrong or missing
facts, throughput outliers, and timeouts. You measure and record. You may fix
ONLY defects inside the language arm you own (see your brief). Everything else
is a FINDING.

## First action, before anything else
```
git merge --ff-only 8e946ada99368b186f367631f93e8f5ae243d712
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure or missing worktree: STOP AND REPORT via
`boop beep --no-wait --as <your-lane> sprefa-coordinator "<one line>"`.
Never archive, copy, `--no-verify`, or work around a blocked command.

Binary: `v6/sprefa-extract/target/release/extract` (relative to your worktree).
`extract --help` documents every flag. `extract --schema` prints record shapes.

## The 10-second law (user law)
No single `extract` invocation may run over 10s. Wrap EVERY call in
`timeout 10`. rc=124 is a FINDING (file path, byte size, node count if
obtainable), never a thing you wait out. Batteries run in background with
`nohup ... &` and a log file; poll the log, never foreground-wait.

## Battery shape (do all five, in order)
1. **Per-file default** (`extract <file>`): every file in the corpus, one
   process each, parallel via `xargs -P 8 -n 1`. Record rc, wall ms, stdout
   line count, stderr first line. Write a TSV: `path\trc\tms\tlines\terr`.
2. **Per-file by family**: `--family cst`, `--family type`, `--family call`,
   `--family df` on a 200-file sample (largest 100 + random 100). Diff line
   counts against the default run; a family whose sum exceeds the default is
   a finding.
3. **--resolve** on each package/crate/module directory (2+ files): record
   `resolved_edge` count, `unresolved` count, rc, ms. Rank packages by
   unresolved ratio; open the top 5 and classify WHY (ambiguity, missing
   import handling, generic, macro, re-export). Cite the file:line.
4. **--family diet_scip** on the same directories. Compare to step 3.
5. **--family scip** on 3 roots where the toolchain exists (check
   `extract --help` EXACT MODE for marker files). Record `scip_skip` rows
   verbatim. Compare `scip_fn_edge` count vs `resolved_edge` from step 3 on
   the same root; sample 20 edges present in one and absent in the other and
   classify each.

## Perf
For every run record wall ms and bytes; compute bytes/ms. Files below the 5th
percentile of bytes/ms get opened: what construct is slow? Also run the 20
largest files under `/usr/bin/time -l` and record max RSS.

## Classifying findings
Every finding row: `lang | class | path:line | repro command | observed | expected`.
Classes: `crash`, `timeout`, `parse_error`, `missing_fact`, `wrong_fact`,
`unresolved`, `perf`, `rss`. Every `missing_fact`/`wrong_fact` row needs a
minimal repro file (under 30 lines) checked into
`v6/sprefa-extract/tests/fixtures/<lang>/corpus_<n>.<ext>` with a comment
stating the expected fact.

## Deliverables (commit all)
- `plans/extract-corpus-2026-08-28/<lang>.REPORT.md` opening with a TOC, then
  a results table per battery step, then the findings table, then a section
  "what stays untested and why".
- `plans/extract-corpus-2026-08-28/<lang>.runs.tsv` (step 1 raw table).
- Repro fixtures as above.
- Any fix you land inside your arm: a failing test FIRST (`cargo test
  --features cli <test>` red), then the fix, then green, then
  `cargo test --features cli` whole-crate count in the report.
- Post a PR: `gh pr create --base main` with the report link.
- Then: `boop beep --no-wait --as <lane> sprefa-coordinator "<lang>: PR #N, files=X rc!=0=Y timeouts=Z findings=W"`.

## Style laws (user-set, non-negotiable)
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`,
  `ground truth` (say oracle), `refusal` (say TODO / not built yet),
  `honest`, `actually`, `clearly`, `obviously`.
- Reports are tables and lists. Prose is a one-line caption. Open with a TOC.
- Numbers only from tool output. Never estimate.
- Comments state only what code cannot show. No dates or arc references in code.
- No `eprintln!` in `src/**`; `tracing` only.
- No em dashes anywhere.

## Forbidden
- Lanes never spawn subagents.
- Never edit `v6/prolog/**`, `v6/sprefa-engine-rs/**`, `v6/tsv2/**`,
  `CLAUDE.md`, or any language arm other than yours.
- Never push to main. Never `--no-verify`.
- Never write outside your worktree except the scratch dir named in your brief.
