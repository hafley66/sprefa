# BRIEF: three independent chores, CI gates + ARCH-MAP staleness + worktree audit

## Base
- Branch: `chore/ci-archmap-worktrees`.
- Base sha: `91c5ea6e` (origin/main). Verify with `git log --oneline -1` FIRST.
  Any other base = STOP AND REPORT.
- Your FIRST action after the worktree exists: `git merge --ff-only 91c5ea6e`.
  Failure = STOP AND REPORT. Do not work around it.

Three chores. They do not depend on each other. Do them in order, commit after
each. If one blocks, finish the other two and report the block with its exact
error text.

## Worktree setup you need before ANY gate command
`node_modules` is absent in a fresh worktree. Run `pnpm install` in `v6/tsv2`
and in `v6/sprefa-store/js`. The text-door corpus is GENERATED, not committed:
run `cd v6/tsv2 && bash scripts/sweep.sh` before anything that reads
`v6/prolog/compile/out/text-door/`.

---

## CHORE 1: wire v6 gates into CI

`.github/workflows/ci.yml` is a placeholder. Its whole job body is:

```yaml
      - name: Gate placeholder
        run: echo "v5 gates removed 2026-08-11 per user; v6 gates not yet wired into CI"
```

Replace it with the real v6 gate. The repo's merge gate is already named:
`v6/justfile`'s `green-all` recipe, which runs `v6/tools/green-parallel.sh`.

Requirements:
- Run on `macos-latest`, the runner already in the file.
- Install what the gate needs: `swipl`, `node`, `pnpm`, and the Rust toolchain.
  Read `v6/tools/green-parallel.sh` and `v6/justfile` to learn what each leg
  actually invokes before you write an install step. Do not guess.
- The corpus is generated. CI must run `v6/tsv2/scripts/sweep.sh` before any
  leg that reads it, same as a fresh worktree.
- Cache what is cacheable: pnpm store, cargo registry, cargo target.

**KNOWN RED, and how to handle it.** `green-all` is red on this base. Measured
legs and causes:

| leg | cause |
|---|---|
| `plunit` | `catalog_plane_rail:level_plane_family_corpus_counts`, `plunit_tests.pl:1312`, 1 of 598 |
| `rtkq-golden` | `missing release extractor: v6/sprefa-extract/target/release/extract` |
| `compile-speed` | `COMPILE_SPEED regressions=16 improvements=0`, baseline written 2026-08-07 |
| `tsv2-test` | `hostDecode.test.ts:144`, expected `[0,1,2,3]` actual `[1,2,2,3]` |
| others | see a local `just green-all` run |

Do NOT fix these. Do NOT delete the legs. Wire CI so it runs the gate and
reports honestly, then add ONE file, `.github/CI-KNOWN-RED.md`, listing each
red leg with its exact failure text and the date you measured it. A CI that
lies green is worse than one that is red.

If your first CI run needs a green signal to be useful, make the workflow run
`green-all` and upload the log as an artifact, with the job's own pass/fail
decided by a leg allowlist read from that same markdown file. State in the
plan section of your report exactly which mechanism you chose and why.

**Warning:** edits under `.github/` have been classifier-blocked twice in this
repo. If your commit is blocked, STOP, leave the file written but uncommitted,
and report it. Do not use `--no-verify`, do not copy the file elsewhere, do not
route around it. A permission denial ends the approach.

---

## CHORE 2: ARCH-MAP.md staleness gate

`v6/tsv2/scripts/self-map.sh:23` states the determinism contract:

> ARCH-MAP.md must be byte-stable across runs so a staleness gate can diff it.

No such gate exists. `v6/tools/staleness-gate.sh` IS in `green-parallel.sh:22`,
but it covers two other things: generated `gen_emitted` modules absent from the
manifest, and binaries older than their sources. It never touches ARCH-MAP.md.
Neither `self-map` nor `devlog` appears in the `green` or `green-all` recipe
lists (`v6/justfile:209-219` declares the recipes; grep the recipe bodies to
confirm).

ARCH-MAP.md is a release gate (`conformance/rulings.pl:532`, `release_gate_v620`),
so a stale one is a real defect.

Build it:
- Extend `v6/tools/staleness-gate.sh` with a third half: regenerate ARCH-MAP.md
  into a temp path via the same entry `self-map.sh` uses, diff against the
  checked-in `v6/ARCH-MAP.md`, FAIL on any difference.
- Follow that script's existing structure exactly. It already has a `fail`
  helper and a header carrying sabotage receipts. Match both.
- Add your own sabotage receipt to the header: change one thing, show the gate
  catching it, revert. State the command and the caught output.
- Do NOT add a new recipe. `staleness-gate` is already in `green-parallel.sh`;
  the new half rides it.

The output must be byte-stable, so if regeneration produces a diff on a clean
tree, that is a FINDING, not something to paper over. Report it with the diff.

---

## CHORE 3: audit the registered worktrees

`git worktree list` reports 53 entries. Most predate this session and were
never audited.

Produce `plans/2026-08-11-worktree-audit.md`, one row per worktree:

| column | content |
|---|---|
| path | the worktree path |
| branch | its branch |
| ahead of main | commit count, `git rev-list --count origin/main..<branch>` |
| dirty | `git status --short` line count in that worktree |
| last commit date | `git log -1 --format=%ad --date=short` |
| verdict | `MERGED` / `UNMERGED WORK` / `EMPTY` / `MISSING TREE` |

`MERGED` = zero commits ahead and clean. `UNMERGED WORK` = any commit ahead or
any dirty file. `EMPTY` = registered but the directory is gone.

**Delete nothing.** Removal is the coordinator's call. Your deliverable is the
table plus one summary line: how many fall in each verdict, and the total
disk bytes under `.boop-worktrees/` and `.claude/worktrees/` from `du -sh`.

Two worktrees are LIVE right now and must be reported but never touched:
any path under `.claude/worktrees/` whose branch starts `worktree-agent-`.

---

## Files you own
| path | chore |
|---|---|
| `.github/workflows/ci.yml`, `.github/CI-KNOWN-RED.md` | 1 |
| `v6/tools/staleness-gate.sh` | 2 |
| `plans/2026-08-11-worktree-audit.md` | 3 |

Touch nothing else. Explicitly forbidden: everything under `v6/prolog/`,
`v6/tsv2/`, `v6/boop/`, `v6/labs/`, `chat_log/`. Three other lanes are live in
those trees right now.

## Gates
```bash
cd v6 && just green-all     # report the delta against your stashed diff, never the absolute list
bash v6/tools/staleness-gate.sh   # after chore 2, must pass on a clean tree
```
ZERO legs may turn red versus your own base measurement. Measure the base FIRST,
before you change anything, so you have something to diff against.

The 10-second law: any single operation over 10s is a defect to investigate,
not a budget. Named exception: SCIP indexing.

## Deliverable
A final report with: one section per chore, the file:line you changed, the CI
mechanism you chose and why, the staleness-gate sabotage receipt verbatim, the
worktree verdict counts, and the green-all delta.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime` in prose
  or identifiers.
- "refusal" is banned in prose; unbuilt work is "TODO" or "not built yet".
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no arc references, max 2 consecutive comment lines.
- Tables and lists over prose. Numbers come from tool output only.
- Never announce location in text ("here is", "below is", "the following").
