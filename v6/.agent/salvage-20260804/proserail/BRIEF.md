# Lane: comment rail counts PROSE lines, not delimiter/decoration lines

## Mission, one sentence
Change the dl6 comment-budget rail so the >max_run threshold counts only
comment lines whose stripped text contains a letter (`[A-Za-z]`), so a
`/*` or `*/` delimiter line, a `// ----` divider, and a `#!` shebang line
never count against the budget, while still keeping runs contiguous.

## FIRST ACTION (worktree dispatch law)
```bash
git merge --ff-only 4e7a32d6aab4dbb6830df82f99d341f589ab3085
```
If this fails, STOP and write the error into REPORT.md. Do not work around it.

If reality deviates from ANY claim in this brief, STOP and record the
deviation in REPORT.md; do not improvise.

## Ruling this implements (user, 2026-08-04)
1. Delimiter-only lines (`/*` alone, `*/` alone) never count.
2. A comment line counts only if its text, after token stripping, matches
   `[A-Za-z]`.
3. The shebang line (`#!...` at line 1) never counts, even though its
   stripped text contains letters (finding F5 dies with this).
4. Non-counting comment lines are GLUE: they keep a run contiguous but add
   nothing to its measure. A code or blank line still breaks the run.

## Receipts (verified on 4e7a32d6, the dl6 rail is the pre-commit default)
Staged `v6/tsv2/try_rail.ts` = `/*` + `one prose line` + `*/` + code:
`bash v6/tsv2/scripts/comment-budget-rail.sh` exits 2 reporting
`1-3 (3 comment lines)`. After this lane it must exit 0 (1 prose line).
Three `// narrative` lines must STILL exit 2, two must still exit 0.

## Design (settled; implement, do not redesign)
Host side, `v6/tsv2/scripts/comment_node.py`:
- `strip_tokens/1` (line 11) already strips `//`, `/*`, `*/`, `#`, `#!`,
  leading `*`. Prose test = stripped line text matches `[A-Za-z]`.
- Shebang: physical line 1 whose raw text starts `#!` is forced non-prose.
- The `comments` projection emits per-node span rows today
  (`line`, `end_line`, `kind`, see the `print(json.dumps(...))` at line 88).
  Add a per-LINE projection (new subcommand or extra rows; follow the file's
  existing verb style, `comments|lines PATH` at line 124): one row per
  physical line covered by a comment node, with
  `prose_flag` (0/1 per the rules above) and `prose_seq` (per-file counter
  that increments ONLY on prose lines; non-prose rows carry the seq of the
  previous prose line so a range min/max never lands between numbers; the
  first rows before any prose line carry 0).
- `prose_seq` exists so the rail can measure prose-line count inside a run as
  `max(seq) - min(seq) + 1` over the run's extent with min/max aggregates,
  never a COUNT over a nodes-x-runs join. min/1 is already used by
  `run_extent` in the .dl6; sum/avg/min/max are the aggregate set
  (ARCH row aggregate_text_refusal names them). If max/1 is NOT available in
  dl6 surface, STOP and report; do not emulate it.

Feed side, `v6/tsv2/scripts/comment-budget-feed.sh`: the `nodes` verb feeds
`comment_fact`; extend it (or add a verb, matching its existing style) so the
program receives the per-line rows against the STAGED BLOB (`{digest}`), same
content-addressing as today.

Program side, `v6/tsv2/goldens/comment_rail_golden/0_comment_rail_golden.dl6`:
- `comment_fact` host signature gains the per-line shape (or a second host
  rel beside it if mixing breaks the one-rel-one-rule-kind law; your call is
  ONLY between those two spellings, record which and why in REPORT.md).
- Coalescing (node_successor_line / node_predecessor_line / run_start /
  run_end / run_end_candidate / run_extent) stays over ALL comment lines so
  glue keeps runs contiguous.
- `long_run` changes from `end - start + 1 > max_run` to the prose measure:
  min/max of `prose_seq` over prose rows inside the extent, count =
  `max_seq - min_seq + 1`, threshold `> max_run`; a run with NO prose rows is
  never long.
- The violation message's `(N comment lines)` becomes the PROSE count; the
  rest of the 5-line stderr contract stays byte-identical
  (comment-budget-rail.sh consumers rely on it).
- Formerly-quadratic law: the new min/max joins get EXPLAIN-pinned
  SEARCH-not-SCAN entries in `7_assert.py` beside the existing four, plus
  fixture-vs-20x count rows proving prose rows scale with comment lines,
  never with added lines.

## Fixtures and goldens (all in v6/tsv2/goldens/comment_rail_golden/)
- `5_gen_schedules.py` grows cases: (a) block `/*` + 3 prose + `*/` =
  VIOLATION at 3; (b) block `/*` + 1 prose + `*/` = clean; (c) shebang +
  2 prose `#` lines = clean, shebang + 3 prose = VIOLATION; (d) `// ---`
  divider between 2+2 prose lines = VIOLATION (glue joins them, 4 prose).
  Keep the existing eight files' verdicts unchanged EXCEPT src/block.ts:
  re-derive its verdict under prose counting from its fixture content and
  record the before/after in REPORT.md.
- Regenerate `1_schedule.json`, `2_expected.tick.jsonl`,
  `3_expected.final.jsonl`, scale twin; update `7_assert.py` counts;
  `6_gate.sh` must end `COMMENT_RAIL_GOLDEN_HOLDS`.
- `9_failfirst.sh` gains the block-delimiter leg: BEFORE your change it
  exits 2 on the 1-prose block (run it at lane base, paste output in
  REPORT.md as the RED receipt), AFTER it exits 0.
- `8_parity.sh`: bash-tool divergences grow (bash counts delimiters and
  shebangs). Update the expected table; every divergence row states which
  side is right and why (prose ruling). The bash tool is retired as default;
  do NOT modify it.
- `README.md` of the golden: update the check table, fixture table, parity
  table, linearity table. No history narrative.

## Validation (record outputs verbatim in REPORT.md)
```bash
bash v6/tsv2/goldens/comment_rail_golden/6_gate.sh      # HOLDS
bash v6/tsv2/goldens/comment_rail_golden/9_failfirst.sh # HOLDS red+green legs
bash v6/tsv2/goldens/comment_rail_golden/8_parity.sh    # HOLDS
cd v6 && just conformance && just text-door && just plunit
```
Plus the three staged-file receipts from the Receipts section, live via
comment-budget-rail.sh. Every script under 10s except the just gates'
own budgets. Toolchain: swipl, just, python3, node. No package installs.

## Ownership (touch NOTHING else)
- v6/tsv2/scripts/comment_node.py
- v6/tsv2/scripts/comment-budget-feed.sh
- v6/tsv2/goldens/comment_rail_golden/** (all ten files)
- REPORT.md at worktree root
Do NOT touch v6/prolog/**, .githooks/**, the bash comment-prod tool,
comment-budget-rail.sh, or v6/dl/fixtures/comment-*.dl6. If a change seems
to require one of them, STOP and report why.

## Style laws
- Comment budget: max 2 consecutive comment lines in anything you add;
  constraints only, no narrative, no dates.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime, support.
- dl variable names descriptive, never single-letter.
- Every .dl6 rule you add or change keeps a pure-rxjs lowering writable; the
  golden's header shows the pattern, extend it for the prose measure.

## Deliverable
Commit to branch `lab/comment-prose-count` (one commit; message = what
changed + validation numbers). Do not push. REPORT.md: changes (file:line),
RED receipts, validation outputs, src/block.ts verdict before/after, the
comment_fact spelling decision, deviations (empty section if none).
