# Lane: Mercury recon lab (modes, determinism, typeclasses) with receipts for dl6

## Base
`git merge --ff-only d2c0e7c9a50c387e683876d3afb7df822c0dea32` is your FIRST
action. Failure = STOP AND REPORT.
Repo: `~/projects/sprefa-v6`. Worktree: `.boop-worktrees/feature/mercury-recon-lab`.

## Why this exists
dl6 (sprefa v6, a datalog compiled by prolog) is designing generics +
interfaces and weighing embedded prolog: compile-time `prolog { }` blocks vs
prolog in rule bodies. Mercury is the existence proof of fully typed, moded,
determinism-checked logic programming, so it is the price list for the
rule-body reading. We need the compiler's real behavior as verbatim receipts,
not textbook summaries. Design calls stay with Chris; you deliver cited
findings.

## Deliverables
Everything under `docs/labs/mercurypl/` in YOUR worktree.

1. `LAB.md` — copy verbatim from the contract block at the end of this brief.
2. `probes/NN_<name>/` per probe:
   - the final `.m` source (compiling, or failing for the INTENDED reason),
   - `cmd.txt` (exact command line),
   - `output.txt` (full compiler + run output),
   - `rc.txt` (exit code).
3. `FINDINGS.md` — a table, one row per probe:
   `probe | Mercury construct | key compiler line (verbatim) | what dl6 has or lacks for the same check`.
   Cite a sprefa path only if you are sure of it; a blank cell beats a guess.
4. `SYNTHESIS.md` — max 2 pages, tables over prose:
   - the determinism lattice (det/semidet/multi/nondet/failure/erroneous) as a
     table: category, meaning, what the compiler infers it from;
   - mode vocabulary (in/out/di/uo, insts) as a table;
   - which subset a datalog whose rels are derived/arrival (no unbound logic
     variables at runtime) actually needs;
   - what typeclass coherence adds beyond dl6's current interface validators
     (duplicate instance, unknown interface, arity — see
     `v6/prolog/0_generic_expand.pl:259-308` in the sprefa repo, read-only);
   - open forks stated as forks. No recommendations phrased as decisions.

## Probes (minimum set; add probes if one surfaces something interesting)
| NN | name | intent | expected receipt |
|---|---|---|---|
| 01 | hello | baseline: det main, io di/uo threading | compiles, runs |
| 02 | detfail | pred declared `det` with two facts | determinism error naming the inferred category |
| 03 | switchgap | 3-constructor du type, det pred switching over 2 | non-exhaustive switch error, inferred semidet |
| 04 | appmodes | ONE pred, TWO modes: `(in,in,out) is det` and `(out,out,in) is multi`; run forward, then `solutions/2` backward | both directions run from one definition |
| 05 | modeerr | call an `in` position with an unbound variable | instantiation mode error |
| 06 | uniqreuse | use a dead `di` io state twice | uniqueness/clobbered error |
| 07 | typeclass | typeclass + instance, method call runs | compiles, runs |
| 08 | dupinstance | second instance for the same type | coherence error |
| 09 | hoinst | higher-order argument with inst `pred(in, out) is det` | compiles; modes live inside insts |
| 10 | inference | exported pred without mode/det decls, plain vs `--infer-all` | error demanding decls, then inference output |

Toolchain: `/opt/homebrew/bin/mmc`, version 22.01.8. Record `mmc --version`
in LAB.md. Prefer `mmc --make <module>` for programs, `mmc -c <module>.m` for
modules without main. Iterate syntax until each probe expresses its INTENT: a
probe failing on a syntax typo is not a receipt. Full compiler output goes in
`output.txt` untrimmed.

## Ownership
You own ONLY `docs/labs/mercurypl/**` inside your sprefa-v6 worktree.
Forbidden: every other path in sprefa-v6, everything in `~/projects/sprefa`
(read-only for the one cited file), everything in `~/projects/hafley-rs`.

## Validation before you finish
```bash
ls docs/labs/mercurypl/probes/*/output.txt | wc -l   # >= 10
ls docs/labs/mercurypl/probes/*/rc.txt | wc -l       # same count
grep -c '^|' docs/labs/mercurypl/FINDINGS.md          # >= probe count + 2
```
Commit everything on your branch. A lane that exits without committing has
delivered nothing.

## Style laws (non-negotiable)
- No em dashes. Banned words in prose AND identifiers: provenance, substrate,
  load-bearing, regime. The word "refusal" is banned; write "not built yet".
- Tables over prose. Every claim about Mercury behavior carries its probe path.
- Comments in probe sources state only what the code cannot show.

## LAB.md contract (copy verbatim as your first file write)
```markdown
# Mercury recon lab

## Goal
Capture the Mercury compiler's actual behavior for modes, determinism,
uniqueness, and typeclass coherence as verbatim receipts, priced against what
dl6 (sprefa v6) would need to check embedded prolog or rel-as-expression.

## Non-goals
- No adoption proposal. No dl6/sprefa code edits.
- No design decisions. Findings are cited forks; the call is Chris's.

## Toolchain
Mercury 22.01.8 (homebrew, `/opt/homebrew/bin/mmc`), aarch64-apple-darwin23.6.0.

## Deliverables (this directory)
| file | content |
|---|---|
| `probes/NN_<name>/` | final `.m` source, `cmd.txt`, `output.txt`, `rc.txt` per probe |
| `FINDINGS.md` | one row per probe: construct, verbatim key compiler line, dl6 gap note |
| `SYNTHESIS.md` | mode+determinism vocabulary as tables; the subset a datalog with derived/arrival rels needs; typeclass coherence vs `v6/prolog/0_generic_expand.pl:259-308` |

## Landing rule
Labs die on landing. Durable content condenses into sprefa `plans/`; this
directory keeps the receipts; the landing commit hash goes here when it lands.
```
