# BRIEF: enumerate the blast radius of EDGE ARM DECLARATION ORDER (v6)

You are a READ-ONLY enumeration lane. You produce ONE file: `REPORT.md` at the
root of this worktree. You do not edit source. You do not commit. You do not
"fix" anything. If reality deviates from this brief, STOP and report the
deviation in REPORT.md; do not improvise.

Your worktree is already at the correct base sha `80ba9db6`. Verify with
`git rev-parse --short HEAD` and STOP if it is not `80ba9db6`.

## The established facts (do not re-derive, do not doubt, just use)

In the v6 reference engine (`v6/prolog/conformance/engine.pl`), when ONE
occurrence fires TWO edge rule arms that share a head:

- head is a KEYED set, two different rows, same key -> engine throws
  `keyed_conflict/3` (engine.pl:397) and the compiler refuses by name in
  `v6/prolog/analyze.pl` `check_no_edge_head_conflict_risk/2` (near line 1332).
- head is an UNKEYED set -> engine throws `edge_into_unkeyed_set`.
- head is a LOG rel -> BOTH rows append, in DECLARATION ORDER, silently.
  Nothing refuses it and nothing warns.

MEASURED, so you may rely on it: a program with

    kind(journal/1, log), keep(journal/1, count(1))
    journal(first)  <+ ping(_).
    journal(second) <+ ping(_).

ends with `journal(second)` surviving. Swapping the two rule lines makes
`journal(first)` survive instead. Text order of two rules decides which row
survives retention.

## What to enumerate

Answer each numbered question with a TABLE of `file:line` citations. Every row
must carry a real path and a real line number you actually read. If a question
has zero hits, say "zero hits" and show the command you ran. Never guess.

**Q1. Where does the shape already exist in this repo?**
Find every program (conformance fixtures in `v6/prolog/conformance/fixtures/`,
`.dl6` files in `v6/dl/fixtures/`, and any other program text you find) that
declares TWO OR MORE edge rules (`<+`) with the SAME head relation. For each:
file:line, head rel/arity, the trigger relation of each arm, and whether the
head is declared `log`, `keyed`, or plain set.

**Q2. Which of those are order-sensitive in their FINAL STATE?**
Of the Q1 rows whose head is `log`, which declare `keep(count(N))` (order
decides which row survives) versus `keep(all)` (order only shows in the delta
sequence)? Table with the keep declaration's file:line.

**Q3. Who consumes rule-list ORDER?**
Enumerate every site that depends on the ORDER of the rule list or of a derived
statement list. Start from these and follow what you find:
  - `v6/prolog/conformance/engine.pl`  process_occurrences / apply_edge_writes
  - `v6/prolog/lower.pl`               statement_rule_ids/3 and its callers
  - `v6/prolog/emit_ts.pl`             INCREMENTAL_EDGE_STATEMENTS rendering
  - `v6/prolog/analyze.pl`
  - `v6/prolog/3_clock_check.pl`       (it indexes rules with nth1/3)
For each: file:line, the predicate, and one sentence on what order decides
there. Distinguish "order decides OUTPUT" from "order is merely traversal".

**Q4. What artifacts would change if two arms were reordered?**
Enumerate the checked-in artifacts whose BYTES depend on arm order. Candidates
to check: `v6/tsv2/gen_emitted/*.ts` (`ruleId:` fields), `v6/prolog/compile/out/`,
`v6/tsv2/goldens/`, `v6/prolog/compile/out/manifest.json`. Give counts and an
example file:line for each class.

**Q5. What refusal names already exist in this area?**
Enumerate every named refusal / violation term related to edge heads, keyed
heads, conflicts, or retention, with file:line, so a NEW finding name cannot
collide with an existing one. Include the ones in
`v6/prolog/0_refusal_messages.pl`, `v6/prolog/conformance/engine.pl`
(`engine_refusal/3`), and `v6/prolog/3_clock_check.pl` (`clock_violation/2`).

**Q6. Where is `clock_violation/2` shaped?**
List every `clock_violation/2` clause in `v6/prolog/3_clock_check.pl` with
file:line and a 5-word summary of what each one refuses. This is to judge
whether an order-conflict finding fits that predicate's existing shape. Report
the list only. Do NOT recommend.

## Commands that work here

    cd v6/prolog && swipl -q -l conformance/go.pl -g go -g halt | tail -3
    grep -rn '<+' v6/prolog/conformance/fixtures/ | head
    grep -rn 'keep(' v6/prolog/conformance/fixtures/ | head
    grep -c 'ruleId:' v6/tsv2/gen_emitted/golden-flex.ts

Run only read-only commands. Do not run `just`, do not run the test battery, do
not build anything.

## REPORT.md format

    # REPORT: edge arm order blast radius
    base sha: <output of git rev-parse --short HEAD>

    ## Q1 ... ## Q6      (one section each, tables of file:line)

    ## Deviations
    <anything in this brief that did not match reality, or "none">

## Style laws (apply to REPORT.md)

- No em dashes. Use a comma or a semicolon.
- Banned words in prose: provenance, substrate, load-bearing, regime. Use
  source, base layer, critical, mode.
- No "you're absolutely right", no "great question", no praise.
- No recommendations, no opinions, no "should". You enumerate; someone else
  decides.
- Tables and file paths over prose. Under-word everything.
