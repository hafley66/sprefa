# LANE: is oracle_scale_ceiling already answered? (research, tiny)

## FIRST ACTION, NON-NEGOTIABLE
```
git merge --ff-only 9ecd3341
```
Failure or missing trees = STOP AND REPORT.

## THE QUESTION
`v6/prolog/ARCH.pl:873` carries a "User call" that may be STALE:

```
task(oracle_scale_ceiling, unbuilt, [bench_cli]). % RULING CARD (gates rust
phase 1): swipl oracle walls before 10k rows (s1/1k 1.4s vs tsv2 33ms) --
rust cannot be graded at PERF-REPORT 960k scale by tick-log byte-diff. Exits
in bench-cli/CONTRACT.md section 7: (a) reference that scales, (b) tiered
grading (tick log where oracle reaches, final-state hash beyond). User call.
```

The suspicion: the `bench_reference` decision made 2026-07-31 ALREADY names
both exits (a) and (b), which would make this row answered, not open.

## YOUR JOB, RESEARCH ONLY, NO CODE
Answer ONE question with citations:

> Does the bench_reference decision of 2026-07-31 already settle
> oracle_scale_ceiling's exits (a) and (b), or does a real open choice remain?

Read at minimum:
- `v6/prolog/ARCH.pl` row `oracle_scale_ceiling` (:873) and row `bench_reference`
- `bench-cli/CONTRACT.md` section 7 (the two named exits)
- `v6/prolog/rulings.pl` for any row touching bench, oracle scale, or grading
  tiers
- `plans/` docs dated 2026-07-29 through 2026-08-02 mentioning bench or grading
- git log around 2026-07-31 for the decision itself

## THE STANDING LAW THAT APPLIES HERE
"A refusal is a hypothesis, never an edict." Measured 2026-08-01: of 248
inventoried decisions 152 are `agent-verdict` and 155 carry `evidence: none`.
A "User call" annotation written by an agent is NOT the user's word unless a
`rulings.pl` row says so. Check whether a rulings.pl row exists. If the
annotation traces back to an agent with nothing measured, say that plainly.

## DELIVERABLE
`REPORT.md` at the worktree root, answering with one of exactly three verdicts:

| verdict | means |
|---|---|
| ANSWERED | bench_reference settles both exits; quote the text that settles them and name the file:line. Propose the ARCH.pl row edit, do not apply it |
| PARTIAL | one exit settled, one open; name which, with the citation for each |
| OPEN | a real choice remains; state the choice in ONE sentence the user can answer yes/no to, with the throw site or contract line it turns on |

Every claim carries file:line. No verdict without a citation.

## FILES YOU OWN
Nothing. This is READ-ONLY research. Write ONLY `REPORT.md` in your worktree.
Do not edit ARCH.pl, rulings.pl, or any source file.

## ANTI-CHEAT
| banned | why |
|---|---|
| a verdict with no file:line | unverifiable, the whole point is citations |
| reading a comment and reporting it as the answer | "Comments are not the language". A header is not evidence |
| guessing what the 2026-07-31 decision said | find it in git or the plan docs, or report you could not |
| editing any source file | read-only lane |

## STYLE LAWS
No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
`load-bearing`, `regime`. "refusal" banned in prose, say TODO or "not built
yet" (the literal identifier in existing code is fine to quote). Tables over
paragraphs. Under-word everything.

## SIZE
This should take well under an hour. If it is ballooning, stop and report what
you found with the question still open. Do not spawn subagents; lanes never
fan out.
