# BRIEF: dd bench holes + the missing ARCH rows

## Base
- Branch: `chore/dd-bench-arch-hygiene`.
- Base sha: `4dd8ef3a` (origin/main). Verify with `git log --oneline -1` FIRST.
  Any other base = STOP AND REPORT.
- FIRST action after the worktree exists: `git merge --ff-only 4dd8ef3a`.
  Failure = STOP AND REPORT. Do not work around it.

Two chores, independent. Commit after each. If one blocks, finish the other and
report the block with its exact error text.

## Worktree setup you need before any gate command
`node_modules` is absent in a fresh worktree: run `pnpm install` in `v6/tsv2`
and in `v6/sprefa-store/js`.

---

## CHORE 1: the three bench holes

All three are recorded in `plans/2026-08-11-dd-line-recon.md`, section Q3.
Read that section first; it has the citations.

### Hole 1: dead engine rows
`v6/sprefa-store/bench/run.sh:19-32` still lists `sqlite_reach`, `dd_reach` and
`dbsp_reach`. Those engines were DELETED at commit `a7d5ad36`. The rig SKIPs
4 of 11 engine rows as a result.

Fix: remove the dead entries. Confirm by running the rig and showing the SKIP
count go to zero, or reporting what still skips and why.

Before you delete: `git show a7d5ad36 --stat` and confirm those engines are
genuinely gone rather than renamed. A rename means the row gets updated, not
deleted.

### Hole 2: `sqlite_baseline` in no battery
The engine now exists on disk, but no battery runs it, and
`v6/labs/BENCHMARKS.md:272` still calls it absent. Two edits: put it in the
rig's engine list, and correct that line of the doc to match reality.

### Hole 3: `perf_report` has no recipe
The `perf_report` matrix produces the DD comparison table and takes 958
seconds. It has no `just` recipe, so it is invoked by hand and undiscoverable.

Add one recipe to `v6/justfile`. Follow the shape of the recipes already there
exactly, including the comment convention that states the expected receipt line
and the measured runtime. **Do NOT put it in `green` or `green-all`**: 958s
violates the 10-second law by three orders of magnitude and it is a reporting
matrix, not a gate. The comment must say so.

While you are in the justfile, report the current `dl6-budget` ceilings. The
recon measured `grid_10000` at 2500 ms / 900 MB against a banked 2110 ms /
740 MB, about 17% of headroom. Do NOT change the ceilings; they ratchet DOWN
only and that is a measurement decision, not a chore.

---

## CHORE 2: the missing ARCH rows and three stale citations

`v6/prolog/ARCH.pl` is the priced record of landed arcs. Eight commits of
differential-dataflow work landed with NO task row at all, so the record does
not describe the tree.

### The commits, from the recon
`f5f2eaf4` signed-delta, `8676a752` bench row, `00ad3f68` recursive CTE probe,
`43bdbc4e` v2 promoted, `471d0be9` PERF-REPORT refresh, `bff004ab` RECON docs.
Run `git log --oneline` over `v6/sprefa-store/` since 2026-08-09 to find the
rest; the recon names eight and lists six.

### What to write
Add task rows following the EXACT shape of the rows already in the file: a
`task(name, status, [deps]).` term with a trailing comment carrying the landed
detail, the merge sha, and the measured numbers. Read twenty neighbouring rows
before writing one. The numbers to carry, from `plans/2026-08-11-dd-line-recon.md`:

| engine, DAG 960k | ms | statements | ratio to dd |
|---|---:|---:|---:|
| dd | 174.6 | 0 | 1.00x |
| sqlite-signed-delta-v2 | 1135.6 | 3 | 6.50x |
| sqlite-dred-loop | 1774.6 | 53 | 10.16x |
| sqlite-dred-cte | 2582.7 | 6 | 14.79x |

Do NOT invent a status. A row is `done` only if the recon says the work landed
on main. Fork 2 is partial, fork 3 is plan-term only, fork 4 has no code: those
are `unbuilt` or `labbed`, and the comment says which part exists.

### The three stale citations
| site | says | truth |
|---|---|---|
| `v6/prolog/ARCH.pl:202` | cites `emit_rust.pl` | deleted at `688a7252` |
| `plans/2026-08-06-rust-emitter-modes.md:31-33` | repeats the same citation | same |
| third | find it; the recon names three | |

Correct each to what the tree actually contains. Do not delete the surrounding
sentence, correct the citation inside it.

### The gate
`v6/prolog/ARCH.pl` has its own checker:
```bash
cd v6/prolog && swipl -g go -t halt ARCH.pl
```
It must pass. A duplicate-state task row is a real finding it catches; if it
fires, report it rather than deleting a row to silence it.

---

## Files you own
| path | chore |
|---|---|
| `v6/sprefa-store/bench/run.sh` | 1 |
| `v6/labs/BENCHMARKS.md` | 1 |
| `v6/justfile` | 1, one recipe added |
| `v6/prolog/ARCH.pl` | 2 |
| `plans/2026-08-06-rust-emitter-modes.md` | 2, citation only |

Forbidden: `v6/dd-runner/**`, `v6/boop/**`, `v6/tools/**`, `.github/**`,
`v6/prolog/**` EXCEPT `ARCH.pl`, `chat_log/**`. Four other lanes are live.

## Gates
```bash
cd v6/prolog && swipl -g go -t halt ARCH.pl
cd v6 && just green-all     # report the delta against your own stashed diff
```

**KNOWN RED ON BASE, do not chase:** `plunit`
(`catalog_plane_rail:level_plane_family_corpus_counts`, 1 of 598),
`rtkq-golden` (missing release extractor binary), `compile-speed` (baseline
2026-08-07), `tsv2-test` (`hostDecode.test.ts:144`). Measure green-all on your
base FIRST so you have something to diff against. Zero legs may turn red.

## Deliverable
A final report with: one section per chore, the file:line of every edit, the
rig's SKIP count before and after, the text of every ARCH row you added, the
three corrected citations, verbatim `ARCH.pl` gate output, and the green-all
delta.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" is banned in prose; unbuilt work is "TODO" or "not built yet".
- Comments state only constraints the code cannot show. ARCH row comments are
  the ONE exception: that file's whole job is the landed record, so match its
  neighbours.
- Tables and lists over prose. Numbers come from tool output only.
- Never announce location in text ("here is", "below is", "the following").
