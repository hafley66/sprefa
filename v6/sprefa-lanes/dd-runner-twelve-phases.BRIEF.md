# BRIEF: dd-runner sqlite arm, 1 tick phase -> 12, with a battery leg

## Base
- Branch: `feature/dd-runner-twelve-phases`, worktree of `/Users/chrishafley/projects/sprefa`.
- Base sha: `4dd8ef3a` (main). Verify with `git log --oneline -1` FIRST.
  Any other base = STOP AND REPORT.

## User direction 2026-08-11
"we want to move from tsv2 into rust finally". The rust x sqlite arm is the
production target. It executes ONE of twelve tick phases today. Close that.

## The defect, exactly

`v6/dd-runner/src/main.rs:80-94`:

```rust
for phase in &plan.tick_order {
    if phase == "level_before_edges" {
        execute_rules(conn, plan)?;
    }
}
```

`plan.tick_order` carries twelve phases. Eleven are silently skipped. The
comment above it claims "empty phases deliberately perform no SQL", which is
true only because nothing else was wired; it is a description of the gap, not a
reason for it.

`v6/dd-runner` is 404 lines total: `main.rs` 189, `kernel.rs` 215. This is a
small program. Read all of it before you change any of it.

## Work

1. **Name the twelve.** Find where `tick_order` is produced
   (`v6/prolog/6_emit_dd_plan.pl` is the emitter; grep for `tick_order`). List
   all twelve phases, in order, with what each one is supposed to do and which
   emitted plan term feeds it. That list is the spine of your report.
2. **Establish the oracle.** tsv2 is the reference: it compiles 270 of 370
   fixtures and its tick log is the semantic authority. `v6/dd-runner/grade.sh`
   already grades 3 fixtures byte-clean. Read it and state exactly what it
   compares before you widen anything.
3. **Implement the eleven, one phase per commit.** After each, run `grade.sh`
   and report the fixture count that stays byte-clean. A phase that cannot be
   implemented without a design decision STOPS and gets reported, it does not
   get guessed.
4. **Widen the fixture corpus.** 3 graded fixtures against tsv2's 270 compiled
   is the real gap. Add fixtures until `grade.sh` covers every fixture whose
   plan the dd emitter can produce. Report the number you reach and name what
   blocks the rest.
5. **Wire a battery leg.** Neither Rust arm is in any gate today, so nothing
   stops them rotting. Add a `just` recipe that runs `grade.sh`, and put it in
   `v6/tools/green-parallel.sh`'s leg list. Follow the shape of the legs already
   there exactly.

## Known blocker you will hit, and what to do
`dd_plan` throws on mutual recursion at `v6/prolog/6_emit_dd_plan.pl:468`. If a
fixture needs it, that fixture is out of scope: report it in the blocked list
with its name, do not go build mutual recursion.

`ARCH.pl:873 oracle_scale_ceiling` is unbuilt and marked "User call". It gates
grading Rust at 960k. You are grading at FIXTURE scale, which does not need it.
Do not touch it.

## A property this move loses, and what to do about it
The user's stated reason for going through TypeScript first: rxjs modelling,
and TypeScript OOMs made hotspots visible when SQLite rows got unloaded into JS
RAM. A crash was the detector. In Rust that same defect becomes silent memory
growth with no signal.

So: `grade.sh` must record peak RSS per fixture alongside its byte-clean
verdict, and the battery leg fails on a ceiling. Pick the ceiling from a
measured baseline, state the measurement, and write the ceiling where a human
can find and ratchet it DOWN, the way `v6/dl6/budget.json` already works. Read
that file's shape and match it rather than inventing a second convention.

## Files you own
| path | permission |
|---|---|
| `v6/dd-runner/**` | full |
| `v6/tools/green-parallel.sh` | one leg added |
| `v6/justfile` | one recipe added |
| `v6/prolog/6_emit_dd_plan.pl` | READ ONLY unless a phase provably needs an emitted term that does not exist; then say so and stop |
| `plans/2026-08-11-dd-runner-twelve-phases.md` | create |

Forbidden: `v6/boop/**`, `.github/**`, `v6/labs/**`, `chat_log/**`, and every
other file under `v6/prolog/`. Two flash lanes are live in the boop and tools
trees; coordinate by staying out.

## Gates
```bash
cd v6/dd-runner && cargo build --release
cd v6/dd-runner && ./grade.sh
cd v6/dd-runner && cargo clippy --all-targets -- -D warnings
cd v6/dd-runner && cargo fmt --check
cd v6 && just green-all      # report the delta against your own stashed diff
```

**KNOWN RED ON BASE, do not chase:** `plunit`
(`catalog_plane_rail:level_plane_family_corpus_counts`, 1 of 598),
`rtkq-golden` (missing release extractor binary), `compile-speed` (baseline
2026-08-07), `tsv2-test` (`hostDecode.test.ts:144`). Measure green-all on your
base FIRST so you have something to diff. Zero legs may turn red.

## Worktree setup you need first
`node_modules` is absent in a fresh worktree: `pnpm install` in `v6/tsv2` and
`v6/sprefa-store/js`. The text-door corpus is generated: `cd v6/tsv2 && bash
scripts/sweep.sh`.

## Known fatal
- The 10-second law: any single operation over 10s is a defect to investigate,
  never a budget. Named exception: SCIP indexing.
- Surrogate keys law: stored rels key on INTEGER ids, a composite TEXT PRIMARY
  KEY is a defect. Read `.claude/skills/sql-relational-design` and
  `.claude/skills/sqlite-costs` before any schema or query change.
- N+1: never a per-row write. Collect the set, one insert.
- Do not reinvent SQLite or SQL. The store is plain SQLite.
- Eight commits of dd work landed with NO ARCH task row. Yours adds one to
  `v6/prolog/ARCH.pl`... except that file is forbidden to you. Put the row text
  in your plan doc under a heading "ARCH ROW TO ADD" and the coordinator lands
  it.

## Deliverable
`plans/2026-08-11-dd-runner-twelve-phases.md` with, in order:
1. The twelve phases named, in order, each with its emitted plan term.
2. A per-phase table: implemented yes/no, commit, fixtures byte-clean after.
3. The fixture corpus count reached, and the blocked list with reasons.
4. The battery leg, the RSS ceiling, and the measurement it came from.
5. "ARCH ROW TO ADD" with the row text.
6. Gate output verbatim, and the green-all delta.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" is banned in prose; unbuilt work is "TODO" or "not built yet".
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no arc references, max 2 consecutive comment lines.
- Construct names use rxjs, prolog, or SQL words only. "support" is banned.
- dl variable names are descriptive, never single-letter.
- Tables and lists over prose. Numbers come from tool output only.
