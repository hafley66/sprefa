# BRIEF: finish the mutual-recursion arc, your own gate is RED

## Base
- Branch: `feature/mutual-recursion-finish`.
- Base sha: `1e3500af` (origin/main). Verify with `git log --oneline -1` FIRST.
  Any other base = STOP AND REPORT.
- FIRST action after the worktree exists: `git merge --ff-only 1e3500af`.

## What happened on the previous attempt
A lane on this arc exited rc=0 with **zero commits** and a **red gate**. Its
work sat uncommitted in a worktree. The coordinator ran the gates and found:

```
$ cd v6 && just conformance
fail  mutual_recursion_matches_oracle
    MISMATCH deltas even/1
      got [[]]
FAILURES  1
```

Exiting clean on a red gate is a failed lane. Do not repeat it.

The previous work is NOT on main and NOT on a branch. You redo it. The design
below is unchanged and already decided; what follows is what that attempt got
right, so you can move fast, plus the one thing it did not finish.

## USER DECISION, unchanged
"swipl says yes we go yes". The swipl oracle already computes pure positive
mutual recursion; the emitters reject it; the emitters change. The oracle is
the reference and does not move.

Read `plans/2026-08-11-dd-mutual-recursion.md` in full. Fork C is the chosen
shape: stop computing a collapsing topological order for cyclic groups, emit
grouped rules, let a fixpoint settle them.

## What the previous attempt got right, reproduce it
| file | change |
|---|---|
| `v6/prolog/strat.pl` | `topo_order_group/2` returns `Ordered = Group` when `kahn_order/2` cannot cover every head, instead of throwing `recursive_stratum`; new `recursive_stratum_groups/2` exported to identify cyclic groups |
| `v6/prolog/compile/6_emit_dd_plan.pl` | `reject_mutual_recursion/2` deleted; `iterate/1` widened to hold several heads, emitting `iterate([even/1,odd/1])` |
| `v6/prolog/emit_ts.pl` | recompute path admits the cyclic group |
| `v6/prolog/compile/test/6_emit_dd_plan.test.pl` | the test asserting the stop is replaced by one asserting the joint iterate against a real fixture. That replacement is CORRECT: the stop no longer exists, so the old test cannot pass. Keep it a real assertion over a real fixture, never a deletion. |

## THE ONE THING TO FINISH
The conformance fixture `mutual_recursion_matches_oracle` in
`v6/prolog/conformance/fixtures/engine_core.pl`:

```prolog
fixture(mutual_recursion_matches_oracle,
  prog([ kind(clock/1, set) ],
       [ (even(Value) <- odd(Value)),
         (odd(Value) <- even(Value)) ]),
  [ odd(0), even(3) ],
  [ [ +clock(go) ] ],
  [ deltas(even/1, [ [ +even(0) ] ]),
    deltas(odd/1, [ [ +odd(3) ] ]),
    final(even/1, [ even(0), even(3) ]),
    final(odd/1, [ odd(0), odd(3) ]) ]).
```

`deltas even/1` came back `[[]]`.

**Diagnose it before you touch it.** Two possibilities and they need opposite
fixes:

| possibility | how you prove it | what to do |
|---|---|---|
| the joint fixpoint is not propagating | the `final(...)` rows also mismatch | fix the engine |
| the closure already ran at boot, so tick 1 carries no delta | the `final(...)` rows PASS and only `deltas` mismatch | the fixture's delta expectation was wrong, and you fix the fixture |

**If and only if the second is true**, you may change the fixture's expected
deltas, and only with the code cited: name the file and line where boot-time
closure happens and where a tick's deltas are computed, and show why an
already-closed rel emits nothing at tick 1. A fixture edit with no citation is
grading yourself and fails this lane.

Better than either: make the fixture actually exercise a tick. Feed the
mutually-recursive pair a NEW row at the tick rather than only at boot, so a
real delta must flow through the cycle. A fixture where the interesting value
arrives before the first tick tests boot, not recursion.

## Do not cheat
| forbidden | why |
|---|---|
| deleting a red test | the test is the finding |
| widening a fixture's expected output without the citation above | grading yourself |
| making `level_eval.pl` match the emitters | the oracle is the reference |
| a capped iteration called a fixpoint | not closure |
| reporting done with any gate red | this is exactly what failed last time |
| `--no-verify` | a blocked command ends the approach |

`v6/prolog/conformance/level_eval.pl` and `engine.pl` are READ-ONLY to you.

**Fail-first receipts required**, verbatim both directions.
**Sabotage receipt required**: once green, break the joint fixpoint in one line,
show the gate catching it, revert, verbatim both ways.

## Byte-identity obligation
Non-recursive modules must emit the same bytes as before.
`emit_ts.pl:2044-2046` says the one-pass-per-stratum emission is deliberate.
Only a cyclic group takes the new path. Prove it against the corpus with
`just text-door`, not by assertion.

## Files you own
`v6/prolog/strat.pl`, `v6/prolog/emit_ts.pl`,
`v6/prolog/compile/6_emit_dd_plan.pl`, `v6/prolog/compile.pl` (the
`sql_rule_order` call site only), `v6/prolog/conformance/fixtures/**`,
`v6/prolog/compile/test/**`, and `plans/2026-08-11-mutual-recursion-arc.md`.

Forbidden: `v6/prolog/conformance/level_eval.pl` and `engine.pl` (read-only
reference), `v6/prolog/lower.pl`, `0_type_plane.pl`, `0_option_expand.pl`,
`0_generic_expand.pl`, `compile/parse_dl_dcg.pl`, `v6/dd-runner/**`,
`v6/boop/**`, `v6/labs/**`, `.github/**`, `chat_log/**`. Five lanes are live.

## Gates, every commit
```bash
cd v6 && just conformance     # 372 PASS 0 FAIL. YOUR FIXTURE MUST PASS.
cd v6 && just plunit          # see KNOWN RED
cd v6 && just text-door       # compiled=272 byte_identical=272 failures=0
cd v6 && just parse-parity    # parity == total, skips=0, diffs=0
cd v6 && just dd-grade        # graded=200 byte-clean>=131, new since your predecessor ran
cd v6 && just green-all       # final; delta against your own stashed diff
```

`dd-grade` is new: the dd-runner sqlite arm now grades 200 fixtures byte-clean
against the oracle tick log. If your change moves a byte there, that is a
regression you own.

**KNOWN RED ON BASE, do not chase, do not fix, do not allowlist:** `plunit`
(`catalog_plane_rail:level_plane_family_corpus_counts`, `plunit_tests.pl:1312`,
1 of 599), `rtkq-golden` / `flagship` / `extraction-live` / `lsp-diags`
(missing release extractor binary), `compile-speed`, `tsv2-test`,
`golden-flex`, `scale-floor`, `memory-soak`, `getting-started`,
`serve-leak-soak`, `leak-soak`. Full list: `.github/CI-KNOWN-RED.md`. Measure
green-all on your base FIRST. ZERO legs may turn red.

## Worktree setup you need BEFORE any gate
```
pnpm install            in v6/tsv2 and v6/sprefa-store/js
cd v6/tsv2 && bash scripts/sweep.sh
cp /Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract \
   v6/sprefa-extract/target/release/extract
```
That last one is why the pre-commit rail blocks a fresh worktree. Five lanes
have been stranded by it today. Create the directory first.

## COMMIT DISCIPLINE, non-negotiable
Your predecessor produced 8 changed files and committed NONE. Commit after
every measured step. A lane also lost its whole run to a machine sleep today.
If you finish with uncommitted work, the arc is lost.

## Deliverable
`plans/2026-08-11-mutual-recursion-arc.md` with:
1. The three-backend truth table, before and after, with the command per cell.
2. The `deltas even/1` diagnosis: which of the two possibilities, with the
   file:line proving it.
3. Fail-first receipts, verbatim both directions.
4. The sabotage receipt, verbatim.
5. The byte-identity proof for non-recursive modules.
6. Gate output verbatim including `dd-grade`, and the green-all delta.
7. "ARCH ROW TO ADD" with the row text. Valid statuses are `done`, `unbuilt`,
   `labbed`, `active`, `labbing`, `closed`, `parked`, `superseded`. `landed` is
   not one. `ARCH.pl` is not yours to edit.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" is banned in prose; unbuilt work is "TODO" or "not built yet".
- Comments state only constraints the code cannot show. Max 2 consecutive
  comment lines. Fail-first receipts in TEST headers are the named exception.
- Construct names use rxjs, prolog, or SQL words only. "support" is banned.
- dl variable names are descriptive, never single-letter.
- Tables and lists over prose. Numbers come from tool output only.
