# BRIEF: mutual recursion, make the emitters match the oracle

## Base
- Branch: `feature/mutual-recursion-match-oracle`.
- Base sha: `26f3f25f` (origin/main). Verify with `git log --oneline -1` FIRST.
  Any other base = STOP AND REPORT.
- FIRST action after the worktree exists: `git merge --ff-only 26f3f25f`.
  Failure = STOP AND REPORT. Do not work around it.

## USER DECISION 2026-08-11, verbatim
"swipl says yes we go yes"

The swipl oracle is the semantic authority and it ALREADY computes pure
positive mutual recursion. The emitters reject it. That divergence closes in
the emitters' favour of the oracle: **mutually-recursive rules compile.**

The decision is made. You implement. You do not re-open it.

## The measured position, from plans/2026-08-11-dd-mutual-recursion.md
Read that document in full before touching anything. It has every citation.

| backend | today | after you |
|---|---|---|
| swipl oracle, `conformance/level_eval.pl` | ACCEPTS, one stratum, joint `plain_fixpoint/5` `:183-196` | unchanged, it is the reference |
| tsv2, ts + sqlite | throws `recursive_stratum(Heads)`, `strat.pl:96-99` | accepts |
| rust x sqlite, dd plan | throws `recursive_stratum` first (`compile.pl:229`), `mutual_recursion` as a second net (`6_emit_dd_plan.pl:468`) | accepts |
| rust x rust, dd-runner kernel | WOULD evaluate it, joint monotone `settle`, `kernel.rs:86-107` | reached at last |

Verified oracle behavior to match: rules `even(X) <- odd(X)` and
`odd(X) <- even(X)` over base `[odd(0), even(3)]` give
`[even(0),even(3),odd(0),odd(3)]`.

Both stopping emitters share ONE upstream gate. A pure positive cycle drops
both rules into one stratum group (`Gap = 0`, `strat.pl:53-54`) and
`kahn_order/2` throws at `strat.pl:96-99`. Self-recursion is exempt only
because it contributes no cross-rule edge (`DependsOnRef \== HeadRef`,
`strat.pl:92`).

## The fork you implement
Fork C from the research doc, with fork B's runtime consequence:

> Stop computing a collapsing topological order for cyclic groups; emit
> grouped rules and let a fixpoint settle them, matching `level_eval.pl:187-196`
> and the kernel's `settle`.

Fork D (rename the error) is NOT what was chosen. Fork A (break the cycle into
separate groups) loses cross-group iteration and would compute a DIFFERENT
answer from the oracle, which is the whole thing being fixed. Do not drift into
either.

`reject_mutual_recursion/2` at `6_emit_dd_plan.pl:460-470` becomes dead once
the upstream gate admits the cycle. Delete it rather than leaving a second net
that can never fire, and say in your report that you did.

## DO NOT CHEAT. This is the part that matters most.

This arc is easy to fake. Every one of these is a FAILED lane, not a shortcut:

| forbidden | why |
|---|---|
| deleting or skipping a test that goes red | the test is the finding |
| widening a fixture's expected output to match new behavior | that is grading yourself |
| catching the throw and returning a partial answer | silently wrong beats loudly stopped, and this repo prefers the stop |
| implementing only the tsv2 half and reporting the arc done | three backends, one semantics |
| computing a fixpoint that terminates without reaching closure | an iteration cap is not a fixpoint |
| `--no-verify` on any commit | a blocked command ends the approach |
| changing `level_eval.pl` to match the emitters | the oracle is the reference, it does not move |

**The one gate that decides this arc**: for every mutually-recursive fixture
you add, the tsv2 output and the dd plan output must agree with the swipl
oracle's tick log, byte for byte, by the existing conformance machinery. Not
"looks right". Not "same final rows". The tick log, through the existing
comparison. If you cannot make that comparison run, STOP and report why; do not
substitute a weaker check and call it green.

**Fail-first receipts are required.** For each of the three behaviors you land,
show the test failing BEFORE your change and passing after, with both outputs
verbatim in the report. A test that has never been red proves nothing.

**Sabotage receipts are required.** After it is green, break your own fixpoint
in one line, show the gate catching it, revert. Verbatim output both ways.
`v6/tools/staleness-gate.sh` and `v6/tsv2/scripts/self-map.sh` headers carry
the house format; match it.

## What breaks, per the research doc. Handle each, do not discover them late.
| breakage | site |
|---|---|
| the `recursive_stratum` receipt, `ARCH.pl:739` documents ghcacher `= 2` with it | `ARCH.pl` |
| every emitted program's `RuleOrder` ordering | `strat.pl:sql_rule_order/2` `:81-84` |
| tsv2's `recompute_levels` single-pass assumption | `emit_ts.pl:2042-2046` |
| the dd plan's `iterate/1` term holds ONE head today | `6_emit_dd_plan.pl:663-669` |
| the SQLite runtime arm has no fixpoint loop | `v6/dd-runner/src/main.rs:86-90` |
| `6_emit_dd_plan.test.pl:245-254` asserts the stop | that file |
| 3 `grade.sh` fixtures byte-clean if output order moves | `v6/dd-runner/grade.sh` |

The one-pass-per-stratum emission for NON-recursive modules is deliberate and
must stay byte-identical. `emit_ts.pl:2044-2046` says so. A cyclic group takes
the new path; everything else keeps its current bytes. Prove that with the
existing corpus, not by assertion.

## Files you own
| path | permission |
|---|---|
| `v6/prolog/strat.pl` | full |
| `v6/prolog/emit_ts.pl` | full |
| `v6/prolog/compile/6_emit_dd_plan.pl` | full |
| `v6/prolog/compile.pl` | the `sql_rule_order` call site |
| `v6/prolog/conformance/fixtures/**`, `v6/prolog/compile/test/**` | add fixtures and tests |
| `v6/prolog/ARCH.pl` | one task row, plus the `:739` receipt correction |
| `plans/2026-08-11-mutual-recursion-arc.md` | create |

READ-ONLY, it is the reference: `v6/prolog/conformance/level_eval.pl`.

Forbidden: `v6/dd-runner/**` (two lanes own it), `v6/boop/**`,
`v6/labs/**` (a lane owns it), `v6/prolog/compile/parse_dl_dcg.pl` (a lane owns
it), `.github/**`, `chat_log/**`. Four other lanes are live right now.

If the dd-runner SQLite arm's missing fixpoint loop blocks you, that is
EXPECTED: another lane is implementing its twelve tick phases. Report the dd
plan TERM you emit and let that lane consume it; do not edit `v6/dd-runner`.

## Gates, every commit
```bash
cd v6 && just conformance     # 372 PASS 0 FAIL on base
cd v6 && just plunit          # see KNOWN RED
cd v6 && just text-door       # compiled=272 byte_identical=272 failures=0
cd v6 && just parse-parity    # parity == total, skips=0, diffs=0
cd v6 && just green-all       # final; report the delta against your own stashed diff
```

**KNOWN RED ON BASE, do not chase, do not fix, do not allowlist:** `plunit`
(`catalog_plane_rail:level_plane_family_corpus_counts`, `plunit_tests.pl:1312`,
1 of 598), `rtkq-golden` / `flagship` / `extraction-live` / `lsp-diags` (all
four: missing release extractor binary), `compile-speed` (baseline 2026-08-07),
`tsv2-test` (`hostDecode.test.ts:144`), `golden-flex` (stale `json_object/2`
excuse). Full list with exact text: `.github/CI-KNOWN-RED.md`. Measure
green-all on your base FIRST. ZERO legs may turn red.

## Worktree setup you need before any gate
`node_modules` is absent in a fresh worktree: `pnpm install` in `v6/tsv2` and
`v6/sprefa-store/js`. The text-door corpus is GENERATED: `cd v6/tsv2 && bash
scripts/sweep.sh`. The pre-commit rail needs
`v6/sprefa-extract/target/release/extract`; copy the prebuilt one from
/Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract to
the same relative path in your worktree. Four lanes have been stranded by this
today.

## Commit discipline
Commit per measured step. A lane lost its entire run to a machine sleep today
because it batched. Never `--no-verify`.

## Deliverable
`plans/2026-08-11-mutual-recursion-arc.md` with, in order:
1. The three-backend truth table, before and after, with the command proving
   each cell.
2. Fail-first receipts: every test red before, green after, both verbatim.
3. Sabotage receipts: the break, the catch, the revert, verbatim.
4. The byte-identity proof that non-recursive modules did not move.
5. Every row of the breakage table above, with what you did about it.
6. Gate output verbatim and the green-all delta.
7. "ARCH ROW TO ADD" with the row text, matching its neighbours' shape.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" is banned in prose; unbuilt work is "TODO" or "not built yet".
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no arc references, max 2 consecutive comment lines.
  Fail-first receipts in TEST headers are the named exception and stay.
- Construct names use rxjs, prolog, or SQL words only. "support" is banned.
- dl variable names are descriptive, never single-letter.
- Tables and lists over prose. Numbers come from tool output only.
- Never announce location in text ("here is", "below is", "the following").
