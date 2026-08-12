# BRIEF: research, mutual recursion in the dd plan emitter

## Base
- Read-only research on `4dd8ef3a` (main). Verify with `git log --oneline -1`.
- Branch: `plans/dd-mutual-recursion-research`.
- FIRST action after the worktree exists: `git merge --ff-only 4dd8ef3a`.
  Failure = STOP AND REPORT.
- You WRITE exactly two new plan files. You EDIT no source file.

## The question
`v6/prolog/compile/6_emit_dd_plan.pl:460-470` throws
`unsupported_construct(mutual_recursion(HeadRef))`:

```prolog
reject_mutual_recursion(Rule, Rules) :-
    rule_head_ref(Rule, HeadRef),
    rule_body_uses(Rule, Uses),
    (   member(use(HeadRef, _, pos, _), Uses)
    ->  true
    ;   member(use(BodyRef, _, pos, _), Uses),
        BodyRef \== HeadRef,
        rules_reach_ref(BodyRef, HeadRef, Rules)
    ->  throw(unsupported_construct(mutual_recursion(HeadRef)))
    ;   true
    ).
```

A rule may call itself. Two rules may not call each other.

| shape | today |
|---|---|
| `path(X,Y) :- path(X,Z), edge(Z,Y).` | compiles, head appears in its own positive body uses |
| `even :- odd.` plus `odd :- even.` | throws |

Establish what it would take to lift that stop. **You do NOT decide it and you
do NOT write the fix.** The repo law is explicit: language and type-system
design happens with the user in the room, and this is a scheduling-semantics
decision. Bring back cited forks.

## What to establish, in this order

### 1. Is the stop unfinished work or a real impossibility
The repo's standing law: a stop is a hypothesis until traced. Read
`rules_reach_ref/3-4` immediately below the throw and say exactly what it
computes. Then answer: does anything downstream of the emitter actually
prevent mutually-recursive rules, or does the check simply run first?

### 2. What the OTHER two backends already do
This is the core of the research. Three backends exist:

| backend | where |
|---|---|
| tsv2, ts + sqlite, ships today | `v6/prolog/emit_ts.pl` |
| rust x sqlite, rust x rust | `v6/prolog/compile/6_emit_dd_plan.pl` + `v6/dd-runner/` |
| the swipl oracle | `v6/prolog/` level evaluation |

`v6/prolog/emit_ts.pl:2042` carries a comment that
`strat.pl:topo_order_group/2` refuses mutual recursion inside a stratum. Read
`strat.pl` and find out whether tsv2 ACCEPTS mutually-recursive rules by
stratifying them into separate groups, or stops the same way. Cite the code.

Report the truth table: for each of the three backends, does a mutually
recursive program compile, and what does it produce.

If tsv2 already accepts it, then the dd emitter is BEHIND its own reference,
and the fork is much narrower than a language question. Say so plainly.

### 3. What differential dataflow does natively
Read what the repo already recorded, do NOT research the upstream crate:
`plans/2026-08-10-dd-source-hunt.CLOSEOUT.md`, `.RECON.md`,
`plans/2026-08-11-dd-line-recon.md`. The closeout describes DD carrying
updates through product-timestamped iteration inside an iterative scope.
Answer: does that scope naturally hold several mutually-recursive collections
at once, per what those docs say, and which fork of the four ranked transfer
forks would carry it.

Where the docs say nothing, write "no prior work found". Do not reason a gap
closed.

### 4. The cost of the stop, measured
How many programs does this actually block? Count with the manifest, not with
an impression:
- `v6/prolog/compile/out/manifest.json` has 370 fixture rows, each with a
  `bucket` and a `reason`. Grep it for `mutual_recursion`.
- Grep the conformance fixtures and the dl6 corpus for mutually-recursive rule
  pairs that would trip the check even if they compile today on tsv2.

Report a number. If the answer is zero fixtures, say zero; that is a finding,
not a failure.

### 5. The forks
One table per fork, with columns: what it would do, what it costs, which code
would change and where, what it would break. Rank nothing. The user ranks.

At minimum consider: stratify like tsv2 does, put mutually-recursive rules in
one iterative scope the way DD does, and keep the stop but name it accurately.
If the code suggests a fourth, add it.

## What you must NOT do
- Do not edit any source file.
- Do not write the fix, not even as a probe.
- Do not decide the semantics.
- Do not research external libraries or the upstream differential-dataflow
  crate. Read only what this repo already recorded.
- Do not spawn subagents.
- Do not report a limit you have not traced to a line of code.

## Files you own
`plans/2026-08-11-dd-mutual-recursion.md` and
`plans/2026-08-11-dd-mutual-recursion.visual.human.unga.md`. Nothing else.

Forbidden: every source file, and specifically `v6/dd-runner/**`,
`v6/boop/**`, `v6/tools/**`, `.github/**`. Four other lanes are live.

## Deliverable
Two new files:
1. `plans/2026-08-11-dd-mutual-recursion.md` — citations everywhere, file:line,
   opens with a table of contents and the one-sentence answer to "is this
   unfinished work or a real impossibility".
2. `plans/2026-08-11-dd-mutual-recursion.visual.human.unga.md` — plain words,
   diagrams, ZERO citations, for a reader with no context. A plan without this
   second doc is undelivered.

Both contain the three-backend truth table from step 2 and the fixture count
from step 4.

## Commit note
The pre-commit rail needs `v6/sprefa-extract/target/release/extract` and
`node_modules` in `v6/tsv2`. If your commit is blocked by it, STOP, leave the
files written but uncommitted, and report that. Do NOT use `--no-verify` and do
not route around it; a permission denial ends the approach.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" is banned in prose; a stop for unbuilt work is "TODO" or "not built
  yet". The word survives only in literal code identifiers.
- Tables, lists, and mermaid over prose. Prose is a caption under a diagram.
- Numbers come from tool output only. No vague quantity claims.
- Construct names use rxjs, prolog, or SQL words only. "support" is banned.
- Never announce location in text ("here is", "below is", "the following").
