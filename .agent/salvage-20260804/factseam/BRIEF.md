# Lane: dl6 text-door fact seam (finding F1)

## Mission, one sentence
Make a bodiless ground clause in a `.dl6` file (`max_run(2).`) compile through
the text door as a seed row, by routing parsed facts into the `Initial`
argument the term door already consumes, instead of leaving them in the rules
list where the level check refuses them.

## FIRST ACTION (worktree dispatch law)
```bash
git merge --ff-only 857bff30a52c2b57c071c8be6fcb4765c63c3225
```
If this fails, STOP and write the error into REPORT.md. Do not work around it.

If reality deviates from ANY claim in this brief, STOP and record the
deviation in REPORT.md; do not improvise.

## Receipts (verified 2026-08-04 on 857bff30)
The probe file (also saved at `PROBE/probe_fact_seed.dl6` in this worktree):
```
rel max_run(limit_lines: int).
rel doubled_limit(limit_doubled: int).

max_run(2).

doubled_limit(limit_doubled) <-
  max_run(limit_lines), limit_doubled := limit_lines * 2.
```
Running `bash v6/prolog/compile/scripts/compile_dl6.sh PROBE/probe_fact_seed.dl6 /tmp/probe.ts` today emits:
```
{"code":"level_rule_no_positive_body/1","message":"...:4: unsupported_construct: compiler refused rule 'level_rule_no_positive_body' for rel 'max_run/1' ..."}
```
So the PARSER already accepts the bodiless clause (LANG.md:39 documents the
syntax); it lands in `ParsedRules` (v6/prolog/compile/parse_dl.pl:122-136
assembles `prog(Decls, Rules)`; `normalize_host_rule/3`'s catch-all clause at
parse_dl.pl:297 passes a bare head term through unchanged) and the level-rule
check refuses it downstream. No grammar change is needed.

The term door's seam: `compile_fixture/4` (v6/prolog/compile.pl:191-194) reads
`fixture(Name, Prog, Initial, Schedule, Expectations)` and passes `Initial`
to `compile_program/6`. `Initial` is a flat list of ground terms, e.g.
`[ route_row(settings, body_settings), route_row(profile, body_profile) ]`
(see v6/prolog/conformance/fixtures/scopes.pl:31-38). Downstream already
handles it: `check_world_shapes/3` (compile.pl:196), `seeded_refs/2`
(analyze.pl:216), `boot_statements(Decls, RelPlans, Initial, ...)`
(compile.pl:274). Emission of seed rows therefore comes free.

The defect: `compile_dl6/2` (compile.pl:207-230) builds
`fixture(Name, Prog, [], [], [])` at compile.pl:223 with `Initial` hard-coded
to `[]`.

## The change
1. In `v6/prolog/compile.pl`, inside `compile_dl6/2` after the parse phase:
   partition `Prog = prog(Decls, Rules)`'s `Rules` into:
   - facts: a member that is NOT of the form `(_ <- _)`, `(_ <+ _)`, or
     `match(_, _)` AND is ground -> collect into `Facts`, preserve order.
   - everything else stays in `RealRules`.
   Pass `fixture(Name, prog(Decls, RealRules), Facts, [], [])` to
   `compile_program_phases/7`. Keep `program(Decls, Rules, Queries)` handling
   (parse_dl.pl:135-136 can produce either form) exactly as it behaves today:
   if the query form reaches compile_dl6 unchanged today, do not alter it.
2. A NON-ground bodiless clause (e.g. `max_run(Limit).`) must keep refusing.
   Do not add a new refusal code; the existing path may keep firing for it. Add
   a test pinning that it still refuses (any `unsupported_construct` is fine;
   record the exact code you observe in REPORT.md).
3. Second text-door caller: compile.pl:232-234's comment says
   `v6/prolog/compile/scripts/bop_check.pl` calls `compile_program/6` itself.
   Read bop_check.pl; if it builds its own fixture term from parsed .dl6 text
   with an empty Initial, apply the SAME partition there (extract a shared
   helper predicate in compile.pl, exported, rather than duplicating the
   partition logic). If bop_check.pl turns out not to parse .dl6 text, record
   that in REPORT.md and leave it alone.
4. Profiled path: `compile_dl6_profiled/2` in v6/prolog/6_profile.pl (used
   when DL_PERF_LOG is set, see compile/scripts/compile_dl6.sh). If it
   duplicates the fixture construction, thread the same partition through it.

## Tests (fail-first)
Add to `v6/prolog/compile/test/plunit_tests.pl`, following that file's
existing test style exactly:
- `dl6_fact_seeds_initial`: compile the probe text through the text door;
  assert it compiles (no exception) and the emitted .ts contains the seeded
  row (grep the output for `max_run` in a boot/insert statement; pin the
  exact spelling you observe, e.g. an INSERT with value 2).
- `dl6_fact_nonground_refuses`: same probe with `max_run(Limit).`; assert
  refusal.
- `dl6_fact_derives`: assert the probe's `doubled_limit` rule compiles in the
  same program (the fact participates as a joinable rel).
Before writing the fix, run the first test and record its RED failure output
in REPORT.md (this is the fail-first receipt). Then fix, then GREEN.

## Validation (run all, record outputs in REPORT.md)
From `v6/`:
```bash
just conformance   # expect 294 pass / 0 fail
just text-door     # expect 206/206, byte-identical
just plunit        # expect 324 existing + your new tests, 0 failures
bash prolog/compile/scripts/compile_dl6.sh ../PROBE/probe_fact_seed.dl6 /tmp/probe_out.ts && echo PROBE_COMPILES
```
(Adjust the probe path to wherever you keep it in the worktree; state the
path used.) Every command must be under 10 seconds except conformance/plunit
which have their own budgets in the justfile. Toolchain is swipl + just only;
no npm/pnpm/cargo installs.

## Ownership (touch NOTHING else)
- v6/prolog/compile.pl
- v6/prolog/6_profile.pl (only if step 4 applies)
- v6/prolog/compile/scripts/bop_check.pl (only if step 3 applies)
- v6/prolog/compile/test/plunit_tests.pl
- PROBE/ and REPORT.md at worktree root
Do NOT touch v6/tsv2/**, v6/prolog/conformance/fixtures/**, LANG.md, ARCH.pl,
parse_dl.pl. If the fix seems to require touching one of these, STOP and
report why instead.

## Style laws (repo, non-negotiable)
- Comment budget: max 2 consecutive comment lines; comments state only
  constraints the code cannot show; no change-log narrative, no dates, no
  "F1" references in source.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime, support.
- Follow the surrounding file's existing prolog style (predicate naming,
  clause layout).

## Deliverable
Commit everything to branch `lab/dl6-fact-seam` in this worktree (one commit
is fine; message states what changed and the validation numbers). Do not
push. REPORT.md at worktree root: what changed (file:line), RED receipt,
validation outputs verbatim, deviations (empty section if none).
