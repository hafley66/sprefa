# Race lane: implement the interning contract, as far as you can get

FIRST ACTION: `git rev-parse HEAD` in this worktree. It must be 84541acd.
Anything else: STOP and write RACE-LOG.md saying so.

You are one of three agents given this identical brief in separate worktrees.
Distance is graded afterward: milestones completed, with gates green, verified
by the coordinator's own runs. Quality beats speed; a milestone with red gates
counts as zero.

## The spec
plans/2026-08-08-interning-contract.md (rev 3) is the whole spec; its plain-
words twin is plans/2026-08-08-interning-contract.visual.human.unga.md.
Mandatory reads before any code: both contract docs,
.claude/skills/sql-relational-design/SKILL.md, .claude/skills/sqlite-costs/SKILL.md,
and the offload contract's §2.4 + amendments in
plans/2026-08-07-plan-ir-offload-contract.md (the IR storage/encoding handshake).

## Work order
The contract's lane table is your work order, in its own stated sequencing
(I-A first; respect its lower.pl ownership ordering). One milestone = one
lane's scope complete: its gates green, one commit on this worktree's branch,
message `race: <lane-id> <one line>`. Then the next lane.

## Gates, run per milestone from this worktree
- cd v6/tsv2 && bash scripts/sweep.sh          (RUN/FINAL wrong=0 required)
- cd v6/prolog/compile && swipl -f none -g "load_files(['test/plunit_tests.pl'], []), run_tests." -t halt
- cd v6/tsv2 && pnpm exec tsgo --noEmit        (0 errors)
- the contract's own G-gates as they become reachable (G9 A/B, G11, G12, ...)
If node_modules is missing in a package: pnpm install --frozen-lockfile, never npm.

## Rules
- You own this entire worktree; touch nothing outside it. Never push. No
  subagents. No --no-verify; a blocked command ends that approach.
- Style laws: comments state only what code cannot show; banned words in prose
  and identifiers: provenance, substrate, load-bearing, regime; descriptive
  variable names; one rel = one rule kind; N+1 law (set-based writes only).
- The 10-second law: any single test/receipt over 10s (except sweep's known
  full runs and the dl6 chain/layered benches) is a defect; flag it in the log.
- RACE-LOG.md at the worktree root, append-only, one entry per milestone:
  lane id, what landed, gate outputs pasted, wall-clock timestamp. If you hit
  a contract defect or ambiguity: record it with file:line, choose the
  smallest faithful interpretation, and continue; if truly blocked, log why
  and move to the next lane the sequencing allows.

## Stop condition
Stop when no lane can advance without violating the contract or a gate.
Final RACE-LOG.md entry: summary table of milestones reached, gates green,
defects found. Commit the log with your last milestone commit.
