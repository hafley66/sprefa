# BRIEF: wire emit_dd_plan to the emitter seam it already fits

## Base
- Worktree of `/Users/chrishafley/projects/sprefa`. Base sha `154ae23c`.
- FIRST action: `git log --oneline -1`. Any other base = STOP AND REPORT.

## One sentence
`compile_program_phases` already takes the emitter as an argument and calls it
with `(Name, Plan, Lowered, BootStatements, Text)`; `emit_ts` is hardcoded at
both call sites while `emit_dd_plan` computes from the same `Plan` and
`Lowered` through a duplicate fixture-only entry path. Give `emit_dd_plan` the
5-argument entry and let the text door pass it.

## The evidence, already gathered. Verify each line, do not re-derive.
| fact | citation |
|---|---|
| the seam is parameterized | `v6/prolog/compile.pl:438`, `call(Emitter, Name, Plan, Lowered, BootStatements, Text)` |
| `emit_ts` fits it | `v6/prolog/emit_ts.pl:2687`, `emit_program/5` |
| hardcoded at both callers | `compile.pl:291` and `compile.pl:347`, both `emit_ts:emit_program` |
| the dd emitter wants the same two things | `6_emit_dd_plan.pl:38`, `dd_plan_term(Plan, Lowered, DdPlan)` |
| it enters by a duplicate path instead | `6_emit_dd_plan.pl:33-38`, `fixture_dd_plan_json_text/3` re-reads a fixture term and re-runs `program_plan` + `lower_program` itself |
| the one real input gap | `dd_plan_json_dict/6` also needs `Initial` and `Schedule`; `compile_program_phases` already carries `Initial` and does NOT thread `Schedule` |

A prior lane reported this as "no `.dl6`-text-to-dd_plan door exists" and
priced it as a subsystem. That framing is wrong and the coordinator relayed it
without checking the seam arity. The emitter exists and matches. This is
wiring.

## Files you own
| path | permission |
|---|---|
| `v6/prolog/compile.pl` | full |
| `v6/prolog/compile/6_emit_dd_plan.pl` | full |
| `v6/bench-cli/adapters/dd-diet-rust-sqlite.sh` | full |
| `v6/bench-cli/adapters/dd-diet-rust-rust.sh` | full |
| `v6/bench-cli/CONTRACT.md` | edit the section-6 pricing to match reality |
| `v6/prolog/compile/test/6_emit_dd_plan.test.pl` | full |
| `plans/2026-08-12-dd-plan-emitter-seam.md` | create |

Touch nothing else. Explicitly forbidden: `v6/prolog/analyze.pl`,
`v6/prolog/lower.pl`, `.github/**` (a triage lane owns the ledger),
`v6/labs/**`, `chat_log/**`.

## Work, in this order. Commit after each step.

### 1. The 5-argument entry
Add `emit_program/5` to `emit_dd_plan` with the seam's exact signature, so it
is substitutable for `emit_ts:emit_program` with no call-site special case.
`BootStatements` is `emit_ts`'s shape; if the dd plan has no use for it, take
the argument and ignore it, and say so in one comment line.

### 2. Thread `Schedule`
`dd_plan_json_dict/6` needs it. `compile_program_phases` takes the fixture term
`fixture(Name, Prog, Initial, Schedule, Expected)` at `compile.pl:346`, where
the text door passes `[]` for Schedule. Decide and STATE which is right:
- the emitter reads Schedule off the fixture term it already receives, or
- `compile_dl6/3` gains a `schedule(File)` option that the text door fills.
Write the choice and its reason in the plan doc. Do not invent a third door.

### 3. Emitter selection at the text door
`compile_dl6/3` already takes `Options`. Add `emitter(Module:Pred)`, defaulting
to `emit_ts:emit_program` so every existing caller is byte-identical. Prove the
default is unchanged with the text-door gate below.

### 4. The adapters stop exiting 2
`adapters/dd-diet-rust-sqlite.sh` and `dd-diet-rust-rust.sh` currently exit 2
with a named reason. Make them compile the `--program` `.dl6` through the new
emitter option, feed the resulting plan to `dd-runner`, and put the tick log on
stdout and nothing else (`CONTRACT.md` section 2.1). An arm that cannot produce
the tick log keeps its exit 2 and keeps its reason; do NOT fake a row.

### 5. `CONTRACT.md`
Section 6 prices this as unbuilt subsystem work. Replace that pricing with what
it actually cost. If a clause genuinely still cannot be met, name the clause.

## Gates. Every commit.
```bash
cd v6/tsv2 && bash scripts/sweep.sh     # RUN total=286 identical=283 wrong=0, MANIFEST_REASON_DIFF all zero
cd v6 && just text-door                 # TEXT_DOOR compiled=288 byte_identical=288 failures=0
cd v6 && just dd-grade                  # DD-GRADE arm=--dd-diet-rust-sqlite graded=203 byte-clean=134, HOLDS
cd v6 && just bench-cli                 # report the line; see KNOWN RED below
cd v6/prolog && swipl -g go -t halt ARCH.pl
```
`text-door` byte-identity is THE gate on step 3: adding an option must not move
a single byte of emitted TypeScript.

## KNOWN RED on main, do not chase, do not "fix"
- `just green-all` is red; `.github/CI-KNOWN-RED.md` allowlists 11 legs and a
  triage lane is rewriting it right now.
- bench-cli cells `diag_seven_ticks`, `clock_rel_join_storms`,
  `aggregate_retraction` fail because `v6/bench-cli/cases.json` points at
  `v6/prolog/compile/dl_view/`, a LOSSY committed render that drops `: type`
  annotations and whole `rel` declarations. The typed render at
  `v6/prolog/compile/out/text-door/` compiles all three fine. That is a
  separate defect and NOT yours. Do not repoint `cases.json`; `out/text-door/`
  is gitignored (`.gitignore:66`).

## Anti-cheat
| rule | why |
|---|---|
| `text-door` stays 288/288/0 byte-identical | the whole point is that adding an emitter option changes no existing output |
| the dd adapters put ONLY the tick log on stdout | stdout is what gets byte-diffed |
| an arm that cannot meet a clause keeps exit 2 with its reason | a fabricated standings row is worse than a missing one |
| every number in the plan doc comes from a command you ran | no estimates |
| you do not edit `analyze.pl` or `lower.pl` | fenced |

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
- Commit after each numbered step, with that step's gate output in the message.
- Never spawn a subagent.
- The 10-second law: any single command over 10s is a defect to record.

## Style laws, inline
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" banned in prose; unbuilt is "TODO" or "not built yet".
- No `here is`, `here's`, `below is`, `the following`, `clearly`, `obviously`.
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no restating the next line.
- dl variable names descriptive, never single-letter.
