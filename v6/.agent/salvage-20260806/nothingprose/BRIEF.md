# BRIEF: nothingprose — measure the "nothing sentence" form with a dl6 program

You are the nothingprose lane, worktree `~/projects/sprefa-lanes/nothingprose`,
branch `lab/nothing-prose`, base `70eea8d1`. FIRST ACTION: `git merge
--ff-only 70eea8d1`; failure = STOP, write STOP.md. If reality deviates from
this brief, STOP and report; never improvise. You own ONLY
`v6/labs/prose_nothing/` (new dir). Prod code untouched: zero edits outside
that dir.

## The form being measured

Chris flagged this assistant sentence as a "nothing sentence":

> xrxgraph's trait-object indirection is genuine rather than faked

Definition, mechanical, two halves plus an exemption:

1. EVALUATIVE COPULA: `\b(is|are|was|were|looks?|seems?|remains?)\s+
   (genuine|real|solid|correct|clean|right|fine|good|proper|sound|legit|
   robust|reasonable|sensible)\b` (case-insensitive).
2. STRAWMAN CONTRAST: `\b(rather than|as opposed to|instead of being|not
   merely|not just)\b`.
3. RECEIPT EXEMPTION: a sentence bearing any receipt token is NOT a nothing
   sentence even if 1 or 2 match. Receipt tokens: a digit, a hex run of 8+,
   `.pl`/`.rs`/`.ts`/`.md` path fragments, a `file:line` colon-number, a
   percent sign, or a quoted span (straight or curly quotes).

Classes to derive: `evaluative_no_receipt` (1 without 3),
`strawman_contrast` (2 without 3), `pure_nothing` (1 AND 2 without 3, the
flagged shape).

## Architecture law (why this is a dl6 program)

Hosts fetch RAW FACTS only; ALL classification logic lives in the .dl6
program as `regexp/2` rules. The precedent to copy, clause for clause, is
`v6/dl/fixtures/comment-prod.dl6` (read it first): sh feed verb, probe rel,
rules, count aggregate. No python flags, no host-side grep.

## Deliverables (all inside v6/labs/prose_nothing/)

1. `feed-sentences.sh` + a small node feed script: walk
   `~/.claude/projects/*/*.jsonl`, take assistant text blocks and real user
   text (exclude blocks starting `<` containing system-reminder /
   command-name / local-command / task-notification; tool results never),
   strip fenced code and backtick spans, split sentences (period/!/? plus
   space heuristic is fine for a stats lab), emit JSONL rows
   `{"side":"assistant"|"user","sentence":"..."}` with a stable `seq`.
2. `prose-nothing.dl6`: sh host declaring the feed
   (`-> (side: text, seq: int, sentence: text)`), the three class rels as
   regexp rules, the receipt exemption as its own rel (`receipt_bearing`),
   and count-aggregate rels per (class, side).
3. `run.sh`: boots the tsv2 server exactly the way
   `v6/tsv2/scripts/comment-budget-rail.sh` does (copy its server spin-up:
   in-memory db, random port, POST /program, POST /arrivals probe, GET
   /idb/<rel>, kill on exit), runs the program over the full corpus, prints
   the per-class per-side counts.
4. `REPORT-NOTHINGPROSE.md` at the WORKTREE ROOT (never REPORT.md): the
   measured table (counts, sentences per side, rate per 10k), top 15
   assistant offender sentences for `pure_nothing`, verbatim run.sh output,
   deviations section.

## Build steps a fresh worktree needs (sanctioned)

- `cd v6/sprefa-extract && cargo build --release --features cli --bin extract`
- `cd v6/sprefa-store/js && pnpm install --offline --frozen-lockfile`
  (pnpm, NEVER npm, resolves from the store cache)
- Commit with `SPREFA_COMMENT_RAIL_DL6=0` env if the rail blocks the fresh
  worktree.

## Validation (paste verbatim)

- A 6-sentence inline fixture through run.sh: one sentence per class, one
  receipt-bearing evaluative (must NOT count), one plain sentence; expected
  counts stated in the report before the run output.
- Then the full corpus run.

## Style laws

dl6 variable names descriptive, never single-letter. Comments only for
constraints code cannot show, max 2 consecutive lines. No em dashes. Banned
words prose AND identifiers: provenance, substrate, load-bearing, regime.
Every .dl6 snippet quoted in the report carries its pure-rxjs lowering (one
line each is fine).
