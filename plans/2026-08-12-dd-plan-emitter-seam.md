# dd plan emitter: wire emit_dd_plan to the emitter seam it already fits

## Context

`compile_program_phases/8` (`v6/prolog/compile.pl:438`) already takes the
emitter as an argument and calls it with `(Name, Plan, Lowered, BootStatements,
Text)`. Both call sites hardcode `emit_ts:emit_program` (`compile.pl:291`,
`compile.pl:347`). The dd emitter, `emit_dd_plan`, computes from the same
`Plan` and `Lowered` (`6_emit_dd_plan.pl:38`, `dd_plan_term/3`), but entered
only through the duplicate fixture-only path `fixture_dd_plan_json_text/3`
(`6_emit_dd_plan.pl:33`), which re-reads a fixture term and re-runs
`program_plan` + `lower_program`.

A prior lane priced a "no `.dl6`-text-to-dd_plan door exists" as a subsystem
and both bench-cli adapters refused every case (exit 2). The emitter exists
and matches the seam arity; this arc is wiring, not subsystem work.

Verification gates, base sha `154ae23c`:
- `cd v6/tsv2 && bash scripts/sweep.sh` — RUN total=286 identical=283 wrong=0
- `cd v6 && just text-door` — TEXT_DOOR compiled=288 byte_identical=288 failures=0
- `cd v6 && just dd-grade` — DD-GRADE arm=--dd-diet-rust-sqlite graded=203 byte-clean=134, HOLDS
- `cd v6/prolog && swipl -g go -t halt ARCH.pl` — 7/7 PASS

## Decisions

**`emit_program/5` is the seam entry.** `emit_dd_plan:emit_program/5` takes the
seam's exact signature `(Name, Plan, Lowered, BootStatements, Text)` so it is
substitutable for `emit_ts:emit_program` with no call-site special case.
`BootStatements` is `emit_ts`'s boot shape; the dd plan embeds its own rows, so
the argument is taken and ignored (one comment line states it). Rejected: a
third door, or a wrapper that hides the seam.

**Schedule enters the text door by the `schedule(File)` option, not by the
emitter reading the fixture term.** The `.dl6` TEXT surface has no spelling for
an arrival schedule (Initial comes from ground `dl6` facts via
`dl6_seeded_form/3`; the schedule is an external JSON, the same shape
`sweep.pl` writes). The emitter cannot read Schedule off any term it receives:
the seam's five arguments carry only `(Name, Plan, Lowered, BootStatements,
Text)`. So `compile_dl6/3` gains `schedule(File)`, loads it through the
`json_arrival` type-directed conversion (the same `0_json_arrival.pl` predicates
`dl6_oracle.pl` uses), and fills the fixture term's Schedule slot. Rejected:
baking Schedule into the `Plan` (changes plan/9 arity, which every plan consumer
destructures), or merging the schedule into the JSON after emission with a shell
tool (creates the third door the brief forbids).

**Initial and Schedule reach the emitter out of band.** Because the `/5` seam
cannot carry them, `compile_program_phases/8` asserts them into a thread-local
`dd_emit_context/2` immediately before the seam call and retracts after.
`emit_ts:emit_program` never reads the context, so emitting it changes no
existing output. `compile_program_phases` already holds Initial (argument) and
Schedule (in the fixture term), so no new plumbing is needed to assemble them.

**`emitter(Module:Pred)` defaults to `emit_ts:emit_program`.** Every existing
`compile_dl6` caller passes no emitter option and is byte-identical. The
`text-door` receipt (288/288/0) is the proof.

**The adapter arms run through the emitter and let the referee grade the log.**
`adapters/dd-diet-rust-sqlite.sh` and `adapters/dd-diet-rust-rust.sh` compile
the `--program` `.dl6` through `compile_dl6(emitter(emit_dd_plan:emit_program),
schedule(FILE))`, feed the resulting JSON to `dd-runner`, and stream the tick
log on stdout with nothing else. Exit 2 is reserved for named compiler
refusals. The kernel (rust) arm is a partial engine (byte-clean on ~104 of the
conformance corpus, divergent on aggregates/edges); its rows are graded `wrong`
where they differ from the referee, exactly as any real engine's would be, and
are not faked here.

## Verification

- Emitter seam: `emit_dd_plan` plunit suite passes 19/19, including a new
  `text_door_dd_emit_seeds_initial_and_schedule` test that compiles a real
  `.dl6` through `compile_dl6/3` (emitter + schedule options) and checks the
  emitted JSON's `initial` and `schedule` reflect the seed facts and the
  external arrival schedule.
- End to end: `dd-runner` on the emitted match_classify JSON is byte-identical
  to the oracle tick log (`v6/prolog/compile/out/match_classify_response.oracle.jsonl`).
- Adapters: match_classify via `dd-diet-rust-sqlite.sh` exits 0 with a
  byte-identical stdout tick log; a named refusal (`log_on_level_headed_rel`)
  exits 2; the final-state file is not written because dd-runner emits no
  final-state line (no fabricated row).
- Gates: sweep (286/283/0), text-door (288/288/0), dd-grade (203/134 HOLDS),
  `ARCH.pl go` (7/7 PASS). `just bench-cli` is known-red on three program cells
  whose `cases.json` points at the lossy `dl_view/` render and is not chased here.

## Staffing

- Agent: default flash4 lane. Worktree: this branch
  (`feature/dd-plan-emitter-seam`), base sha `154ae23c`.
- Fenced, never edited: `v6/prolog/analyze.pl`, `v6/prolog/lower.pl`,
  `.github/**`, `v6/labs/**`, `chat_log/**`.

<!-- todo(feature): final-state writer for the dd arms (§2.7) — dd-runner emits no final-state line, so the dd adapters omit <perf-out>.final.jsonl; a final-state SELECT would close the third check when dd-runner grows one. -->
