# BRIEF: green-all triage, every leg, on a quiet machine

## Base
- Worktree of `/Users/chrishafley/projects/sprefa`. Base sha `154ae23c`.
- FIRST action: `git log --oneline -1`. Any other base = STOP AND REPORT.

## One sentence
Run every `just green-all` leg one at a time, decide for each whether it is a
real defect, a stale ledger entry, or load-flaky, and rewrite
`.github/CI-KNOWN-RED.md` so it tells the truth.

## Why now
The ledger was measured 2026-08-11 at base `91c5ea6e` and allowlists ELEVEN
legs. The coordinator spot-checked four of them tonight and **two were already
green** (`extraction-live`, `lsp-diags`). A ledger that allowlists green legs
hides the next real regression. One leg is failing that the ledger does NOT
cover at all: `roundtrip`.

## You FIX NOTHING
Triage only. Two other lanes are editing `compile.pl`, `6_emit_dd_plan.pl` and
the bench-cli adapters right now. A fix from you collides. Where the fix is
obvious, write it as a diff INSIDE the plan doc and leave the tree alone.

## Files you own
| path | permission |
|---|---|
| `.github/CI-KNOWN-RED.md` | rewrite |
| `plans/2026-08-12-green-all-triage.PLAN.md` | create |
| `plans/2026-08-12-green-all-triage.PLAN.visual.human.unga.md` | create |

Everything else is READ-ONLY. Zero other paths in `git status`.

## Method
1. `cd v6 && just --list` or read `v6/justfile` for the green-all leg list.
   Run each leg SEPARATELY, `just <leg>`, never the whole gate. The whole gate
   under load produced two different failing sets on the same tree; that is the
   measurement error you are here to remove.
2. Nothing else may run while you measure. Check with `boop beep ps` before a
   timing-sensitive leg (`scale-floor`, `memory-soak`, `compile-speed`,
   `endurance`, `leak-soak`, `serve-leak-soak`). If another lane is burning
   CPU, WAIT and say in the doc that you waited.
3. Run each leg THREE times. A leg that passes 3/3 is green. A leg that fails
   3/3 is real. Anything in between is flaky, and flaky is its own verdict, not
   a pass.
4. For every real failure: capture the verbatim text, then trace it to a
   `file:line`. An error message is a hypothesis; the throw site is the finding.

## Known state, so you do not rediscover it
| leg | coordinator's spot check tonight |
|---|---|
| `extraction-live` | **PASS**, ledger entry is stale |
| `lsp-diags` | **PASS**, ledger entry is stale |
| `flagship` | real: "the corpus MOVED since the v5 golden was captured", golden `b8d03946...` now `8e3874d5...` |
| `getting-started` | real: "block 24: output does not match the doc" |
| `golden-flex` | real: `json_object/2` excused as `refused` but its registry status is now `live`; `json_patch/2` unexercised. 69 registry constructs, 2 unaccounted |
| `tsv2-test` | real: `hostDecode` expected `[0,1,2,3]` actual `[1,2,2,3]` |
| `roundtrip` | real AND **NOT IN THE LEDGER**: `mutual_recursion_matches_oracle` -> `fail(not_variant)` |
| `leak-soak`, `serve-leak-soak` | ledger says stale `$TMPDIR` `mktemp` files, not a defect. Verify with a clean `$TMPDIR` |
| `plunit`, `compile-speed`, `scale-floor`, `memory-soak`, `rtkq-golden` | not retriaged; these are yours |

Confirm each of the rows above yourself. A row you confirm is a receipt; a row
you contradict is a better finding. Do not copy this table into your doc as if
you measured it.

## The ledger rewrite
`.github/CI-KNOWN-RED.md` keeps its exact current shape: a "Red legs" table
with the exact failure text, then `allow:` lines. Rules:
- A leg measured green 3/3 is DELETED from both the table and the allowlist.
- A real failure keeps its row, with the failure text refreshed to what YOU saw
  and a new `throw site` column carrying `file:line`.
- A flaky leg gets its own section, `## Flaky`, with the pass/fail count out of
  3 and what it is sensitive to. It stays allowlisted, marked flaky.
- A real failure NOT currently listed gets added.
- Header states the base sha and date YOU measured at.
- The existing warning line stays: do not edit this list to make CI green.

## Deliverable
`plans/2026-08-12-green-all-triage.PLAN.md`:
1. TOC.
2. The verdict table: leg, 3-run result (e.g. `FAIL 3/3`), verdict
   (real / stale / flaky), root cause in one line, throw site `file:line`.
3. One section per REAL failure: verbatim output, the throw site with 3 lines
   of surrounding code, and the fix as a diff you did NOT apply.
4. The legs you deleted from the ledger, with the 3/3 green receipt for each.
5. Ranked "fix these first" list, cheapest real fix at the top.

Plus `plans/2026-08-12-green-all-triage.PLAN.visual.human.unga.md`: plain
words, ascii or mermaid, ZERO citations. A plan without it is undelivered.

## Anti-cheat
| rule | why |
|---|---|
| every verdict carries 3 runs | one run under load is how the current ledger got wrong |
| a leg is never marked green from reading the ledger | the ledger is the thing under test |
| `just green-all` as a whole is NOT your measurement | it is load-contaminated |
| no leg is deleted from the allowlist without its 3/3 receipt pasted | deleting entries is how a gate silently rots |
| you edit no code | two lanes are in the compiler right now |

## Worktree setup, before your first commit
```bash
mkdir -p v6/sprefa-extract/target/release
cp /Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract \
   v6/sprefa-extract/target/release/extract
(cd v6/tsv2 && pnpm install)
(cd v6/sprefa-store/js && pnpm install)
```
Four legs in the ledger fail with "no release extractor". Copying the binary
is setup, not a fix; if a leg goes green ONLY because you copied it, say so in
the doc, because that means the leg is really a build-step gap.

`git commit -n` and `--no-verify` are FORBIDDEN.

## Rails
- Commit after each leg's verdict, leg name in the message.
- The 10-second law applies to your own commands, not to the legs you measure;
  record any leg over 10s as a finding with its wall time.
- Never spawn a subagent.

## Style laws, inline
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" banned in prose; an unbuilt construct is "TODO" or "not built yet".
- No `here is`, `here's`, `below is`, `the following`, `clearly`, `obviously`.
- Comments state only constraints the code cannot show.
- Tables and lists over prose. Docs open with a TOC.
