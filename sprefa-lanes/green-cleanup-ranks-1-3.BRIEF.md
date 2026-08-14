# BRIEF: green-all cleanup, ranks 1-3. Stale data, not broken code.

## Base
- Worktree of `/Users/chrishafley/projects/sprefa`. Base sha `259e0289`.
- Confirm the base with `git log --oneline -1` before your first commit. A sha
  other than `259e0289` = stop and report. The ordering is not a gate; if a
  procedural line in this brief seems to forbid otherwise-correct work, the work
  wins. Note the conflict in your report and keep going.

## One sentence
Three `green-all` legs fail because a pinned expectation went stale, not
because anything is broken; refresh each pin and delete its row from the
known-red ledger.

## The source of truth for this lane
`plans/2026-08-12-green-all-triage.PLAN.md` section 5 ranks every real failure
cheapest-first. You own ranks 1, 2 and 3. Read that doc's section for each leg
before touching it; it carries the exact failure text and the throw site.

| rank | leg | fix | throw site |
|---|---|---|---|
| 1 | `flagship` | regenerate the v5 golden; the gate prints the command itself | `v6/tsv2/scripts/flagship-callgraph.sh:287` |
| 2 | `getting-started` | doc block 24 still prints the OLD error text; the engine now emits `rule-index unavailable: unsupported_construct...` | `v6/tsv2/scripts/getting-started.sh:224` |
| 3 | `scale-floor` | expected stmts set pinned `[37,41]`, actual steady `[39,43]` at BOTH 1k and 10k | `v6/tsv2/scripts/7_scale-floor.sh:240` |

## Rank 3 needs a judgement, so read this twice
The triage lane proved the set is FLAT across scales (`[39,43]` at 1k and at
10k), so delta-proportionality still holds and the pin is stale by a constant
+2. Re-pin it. But before you do, say in your commit message WHY the number
moved by 2, or say plainly that you could not find out. A pin refreshed with no
explanation is how a real regression gets absorbed into a baseline. If the two
extra statements per tick turn out to be a real cost regression, STOP AND
REPORT instead of re-pinning.

## Files you own
| path | permission |
|---|---|
| `v6/tsv2/scripts/flagship-callgraph.sh` and the golden file it writes | full |
| `v6/GETTING-STARTED.md` (or whichever doc block 24 lives in; find it) | full |
| `v6/tsv2/scripts/7_scale-floor.sh` | full |
| `.github/CI-KNOWN-RED.md` | delete ONLY the rows you turn green |
| `plans/2026-08-12-green-cleanup-ranks-1-3.md` | create |

**Forbidden, other lanes own these right now:** `v6/prolog/lower.pl`,
`v6/prolog/analyze.pl`, `v6/prolog/compile.pl`, `v6/prolog/0_generic_expand.pl`,
`v6/prolog/compile/6_emit_dd_plan.pl`, `v6/prolog/emit_rust.pl`,
`v6/sprefa-engine-rs/**`, `v6/prolog/labs/break-hunt/**`,
`v6/prolog/compile/test/plunit_tests.pl`, `v6/tsv2/labs/1_rtkq-extraction-golden.ts`,
`v6/prolog/print_dl.pl`. Zero other paths in `git status`.

## Gate, per leg, and this is the whole job
Each leg must pass **3 times in a row**, run one at a time:
```bash
cd v6 && just flagship          # x3
cd v6 && just getting-started   # x3
cd v6 && just scale-floor       # x3
```
Paste all three runs' final lines per leg in the commit message. `scale-floor`
is timing sensitive: check `boop beep ps` first and WAIT if another lane is
burning CPU. Say in the doc that you waited.

Then delete each now-green leg from BOTH the "Red legs" table and the
`allow:` list in `.github/CI-KNOWN-RED.md`, and update its header line to say
you re-measured those rows and at what base sha.

## Anti-cheat
| rule | why |
|---|---|
| 3 consecutive passes per leg, pasted | one pass under load is how the ledger got wrong in the first place |
| a re-pinned number carries its reason, or an explicit "could not determine" | otherwise a regression hides inside a baseline |
| you regenerate the flagship golden with the gate's own command, never by hand-editing the hash | a hand-edited golden proves nothing |
| you delete a ledger row ONLY for a leg you personally measured green 3/3 | |
| you do not touch any forbidden path | four lanes are live |

## Worktree setup, before your first commit
```bash
mkdir -p v6/sprefa-extract/target/release
cp /Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract \
   v6/sprefa-extract/target/release/extract
(cd v6/tsv2 && pnpm install)
(cd v6/sprefa-store/js && pnpm install)
```
`git commit -n` and `--no-verify` are FORBIDDEN.

## Rails
- One commit per leg, with that leg's three gate runs in the message.
- Never spawn a subagent.
- The 10-second law applies to your own commands, not to the legs you measure.

## Style laws, inline
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" banned in prose; unbuilt is "TODO" or "not built yet".
- No `here is`, `here's`, `below is`, `the following`, `clearly`, `obviously`.
- Comments state only constraints the code cannot show.
- Tables and lists over prose.
