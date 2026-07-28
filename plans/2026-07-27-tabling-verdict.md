# Lab: can SWI tabling replace the hand-rolled fixpoint? VERDICT: SHIFTS SEMANTICS

Companion to `v6/prolog/labs/tabling_fixpoint.pl`, DELETED per lab protocol;
last copy at commit cf13ed77, recover via
`git show cf13ed77:v6/prolog/labs/tabling_fixpoint.pl`. Contract:
`plans/2026-07-27-tabling-fixpoint-lab-header.md`. Prior art consulted:
`plans/research-swi/01-core.md` (verified against local swipl 10.0.2).

## Verdict

**SHIFTS SEMANTICS**, on exactly the tripwire the header named in advance:
stratified-negation rejection. Everything else measured is a clean match.

- 100/100 fixtures (97 conformance + 3 stress fixtures this lab wrote) are
  byte-identical between the hand-rolled evaluator (`engine:run_program/5`,
  unchanged) and the tabled evaluator (`run_fixture_tabled/5`, this lab) on
  final rowset, per-tick delta lists, and tick count. Zero divergence.
- One deliberate adversarial probe outside the corpus, the textbook
  non-stratifiable pair `p <- not(q), q <- not(p)`, diverges. The
  hand-rolled evaluator throws `not_stratified` (via
  `level_eval:relax_strata/4`'s `Cap` check). The tabled evaluator has no
  code path that performs that check at all (`tabled_level_closure/6` never
  runs `stratify_level_rules/1`); it just resolves `\+` under SWI's ordinary
  tabling and returns `ok` with both `p` and `q` derived true.

Per the header's explicit instruction ("if tabling forces well-founded
semantics anywhere, that is a SHIFTS finding, report it, do not work
around it"), this single divergence decides the verdict, even though the
other 100 comparisons all pass. Reporting is required, not workaround: I did
not try to bolt a stratification check back onto the tabled evaluator to
force it to match, because doing so would just be re-implementing the thing
under test.

## Top 3 receipts

1. **Stratification tripwire, the deciding one** (`probe_stratification_tripwire/0`,
   tabling_fixpoint.pl:513-540): run against `prog([], [(p <- not(q)),
   (q <- not(p))])`.
   ```
   reference (hand-rolled): threw(not_stratified)
   tabled (incremental):    ok
   ```
   The hand-rolled result comes from `relax_strata/4`'s cap: with 2 derived
   refs, `Cap = 3`; a stratum number keeps climbing past the cap on this
   cycle and `throw(not_stratified)` fires before any row is ever derived.
   The tabled result comes from real Prolog resolution: `p`'s clause body
   `\+ q(_)` and `q`'s clause body `\+ p(_)` get resolved via SWI's ordinary
   incremental tabling with no equivalent cap, and it settles on `[p, q]`
   both true, not classical stratified semantics (which has no answer for
   this program) and not textbook well-founded semantics either (which
   would leave both `p` and `q` undefined, not true). Whichever internal
   evaluation order SWI's tabling engine picked here decided the answer;
   that non-canonical, order-dependent settling is itself part of the
   finding, not just the fact that it answered at all.

2. **97/97 conformance fixtures + 3/3 stress fixtures, zero divergence**
   (full run: `swipl -q -l v6/prolog/labs/tabling_fixpoint.pl -g go -g halt`,
   exit 0). Every fixture in `conformance/fixtures/*.pl`, including all 7
   `throws(...)` fixtures (`missing_retention`, `keyed_log_rel`,
   `edge_into_unkeyed_set`, `retract_from_log`, `keyed_conflict`,
   `arith_on_non_int`, `json_object_dup_key`), produces an identical
   outcome from both evaluators. This corpus already exercises count/sum/
   min/max/json_array/json_object aggregation, keyed replace, Log
   multiplicity (q7 bag semantics), decode/json_each, and `not/1` over
   derived rels (`timeless_rail.pl`, the fixture that originally forced the
   hand-rolled engine onto stratified evaluation), all byte-identical.

3. **The 3 stress fixtures this lab added, all byte-identical too**
   (tabling_fixpoint.pl:410-470):
   - `stress_keyed_replace_churn`: 5 same-key writes in a row, alternating
     values, reading back through a level rule (`loud_author`) that flips
     on and off each tick. Matched exactly, including the two ticks where
     the level view has to retract-then-nothing (drain ticks with `[]`).
   - `stress_pure_retraction_tick`: a tick whose ONLY arrival is a bare
     `-candidate(orion)` on a Set rel, no `+Row` anywhere that tick, the
     exact shape of the engine's own historical pure-retraction-tick defect
     (chat_log 20260727.3, fixed via reverse_preserving deletion). Matched.
   - `stress_departure_chain`: a keyed replace departs the old row, a
     `departed/1`-triggered edge rule turns that into a Log write, and a
     level rule counts it, carried across the T+1 boundary per ruling r4.
     Matched, including the one-tick lag between the retraction and the
     count updating.

   This directly answers the "retraction propagation is incremental
   tabling's known hard case" concern from `01-core.md`: on every shape of
   retraction this lab could construct (keyed churn, bare Set retraction,
   departure-triggered chains), the incremental dependency graph invalidated
   and recomputed exactly the rows the hand-rolled from-scratch recompute
   did, every tick.

## Microbench (3 heaviest fixtures by rule-count + initial-row-count +
schedule-row-count, `time/1` around both evaluators, inference counts not
wall-clock)

| Fixture | Reference inferences | Tabled inferences | Delta |
|---|---:|---:|---:|
| `clean_state_gate_and_exit_zero` | 10,443 | 13,755 | +31.7% |
| `fix_by_waiver_returns_to_clean` | 8,224 | 12,321 | +49.8% |
| `clock_rel_join_storms` | 23,365 | 22,555 | -3.5% |

Mixed, not a clean win. The two smaller fixtures cost MORE inferences under
tabling (namespacing/predicate-dispatch overhead per call dominates at this
scale); the one with the most join/clock activity is very slightly cheaper.
None of these fixtures are large enough to exercise the actual payoff case
(incremental tabling should win when MANY ticks reuse MOST of a large
derived table and only a small delta changes it); the conformance corpus is
unit-test-scale, not a real workload, so this microbench says "no regression
at this scale," not "faster."

## LOC comparison

This is NOT a clean line-count shrink, and I want to say that plainly rather
than round it into the requested SHRINKS shape.

| level_eval.pl predicate | Lines | Fate under adoption |
|---|---:|---|
| `aggregate_head/3`, `classify_head_arg/2` | 12 | **survives**, reused verbatim via `level_eval:aggregate_head/3` |
| `split_rules/4` | 6 | **survives**, reused verbatim |
| `level_closure/5`, `eval_strata/5` | 18 | **dies**, replaced by `tabled_level_closure/6` |
| `stratify_level_rules/1` | 20 | **dies**, no replacement (see verdict) |
| `rule_body_constraint/4` | 9 | **dies**, no replacement |
| `goal_rel_refs/3` | 18 | **survives, repurposed**, this lab reuses it to find which base predicates a fixture's rules read, not to compute strata |
| `relax_strata/4` | 22 | **dies**, no replacement; this is exactly the code whose absence produces the SHIFTS finding |
| `plain_fixpoint/5` | 11 | **dies**, replaced by real tabled Prolog clauses |
| `agg_loop/6` | 8 | **dies**, replaced by tabled Prolog clauses + `lab_agg_rows/3` |
| `agg_rule_rows/4` | 14 | **dies**, replaced by `lab_agg_rows/3` (same shape, calls a translated goal instead of `solve/2`) |
| `head_arg_value/2`, `group_key/3`, `aggregate_args/3`, `agg_compute/3` | 27 | **survives**, reused verbatim via qualified calls |

Dies: 102 lines. Survives unchanged: 66 lines. That is the honest scope of
what plain_fixpoint/agg_loop and their stratification apparatus cost today.

What replaces the 102 dead lines, in THIS lab, is NOT smaller:
`tabling_fixpoint.pl` lines 53-278 (namespacing, multiset diff, body
translation, rule installation, level collection, the tabled closure itself)
is 226 lines. That is a LOC increase, not a decrease, and I am not going to
call it a shrink by rounding.

Why the replacement is bigger here, and why a real integration would likely
look different: this lab's translator exists to run ~100 independent
fixtures in ONE swipl process, so most of its bulk (`pred_name/3`,
`ensure_base_declared/2`, the whole namespacing layer, `next_run_id/1`) is
solely there to keep one fixture's predicates from colliding with the next
one's. A real adoption inside the actual v6 engine would compile the ONE
program it is running to real predicates ONCE at load time, no per-run
namespace, no re-diffing a full snapshot against a remembered previous
snapshot every tick (the diffing exists here only because `engine:tick/7`
hands `level_closure` a full store snapshot rather than a delta; a native
integration could feed real assert/retract calls directly instead of
diff-then-assert). That would cut a meaningful fraction of the 226 lines,
but I have not built that version, so I am reporting the lab's actual
measured LOC, not a projected one. Whether the projected, integrated version
would net below 102 lines is genuinely unclear from this lab; see ambiguity 1.

## Diff sketch (NOT applied; level_eval.pl is read-only to this lab)

If this were adopted despite the SHIFTS finding (e.g. by accepting that
non-stratifiable programs become the caller's problem, or by keeping
`stratify_level_rules` as a load-time validation pass that runs once and
then discards its stratum numbers, using them only to decide accept/reject,
never to sequence evaluation):

```
level_eval.pl:
- level_closure/5, eval_strata/5              (18 lines) -- DELETE
- plain_fixpoint/5                            (11 lines) -- DELETE
- agg_loop/6                                  ( 8 lines) -- DELETE
- agg_rule_rows/4                             (14 lines) -- DELETE, folded into
                                                             the tabled agg clause
+ install_rules/3, translate_goal/5           (~90 lines) -- NEW, compiles
                                                             Rules (still DSL
                                                             data at this call
                                                             site) to real
                                                             tabled/dynamic
                                                             predicates ONCE
                                                             per running
                                                             program, not per
                                                             tick
+ (keep) stratify_level_rules/1, relax_strata/4         -- KEEP as a
                                                             load-time-only
                                                             accept/reject
                                                             gate, run once,
                                                             never again used
                                                             to sequence
                                                             evaluation

engine.pl:
  tick/7's two level_closure(...) calls become two calls into the compiled
  program's now-real predicates directly (no Base snapshot diffing needed if
  absorb_arrivals/apply_edge_writes assert/retract the SAME underlying
  dynamic predicates directly, rather than rebuilding a Store list engine.pl
  still separately maintains for boundary_deltas/retention/keyed-conflict
  bookkeeping) -- this is the part that makes it a redesign rather than a
  drop-in edit, exactly as 01-core.md predicted.
```

I have not applied any of this; it is a sketch of the shape a real adoption
would take, offered so the LOC table above has a concrete referent.

## Numbered ambiguities

1. **The LOC criterion inside "SHRINKS" was not met by what I built, and I
   do not know if a production integration would meet it either.** The
   header defines SHRINKS as byte-identical AND fewer lines. This lab is
   byte-identical on every corpus fixture but is not fewer lines as built
   (226 replacement lines vs 102 dead lines), and separately, the
   stratification tripwire diverges. Both facts point away from SHRINKS,
   but for two different reasons (one an artifact of this lab's
   per-fixture-reentrant harness, the other a genuine semantic gap). I did
   not attempt to build the "compile once, no namespacing" version that
   would give an honest LOC answer for a real integration, since the
   tripwire finding already forces SHIFTS regardless of what that number
   would be. If the user wants the LOC question settled independently of
   the stratification question, that is a follow-up lab, not answered here.

2. **The "only PASS lines" constraint and the "print both delta lists on
   any diff" constraint are in direct tension once a real divergence
   exists.** I resolved this by printing full `MISMATCH`/`TRIPWIRE`/
   `DIVERGES` diagnostic blocks (there are none from the corpus, one from
   the tripwire) and letting `go/0` always succeed (never fail the overall
   goal on a comparison mismatch), so `swipl ... -g go -g halt` still exits
   0. But the run's stdout is not literally "only PASS lines", it also
   has `TRIPWIRE`/`DIVERGES` lines (1), `BENCH` lines (the microbench), and
   the closing `=== SUMMARY ===`/`VERDICT` block. I judged printing the
   actual finding to be the higher-priority instruction ("report it, do not
   work around it") over the literal letter of "only PASS lines," since the
   two were never simultaneously satisfiable once a genuine divergence
   existed to report.

3. **The tripwire probe is a hand-built adversarial program, not a promoted
   fixture.** I did not add it to `conformance/fixtures/` (read-only, and it
   would need a `throws(not_stratified)` expectation the corpus does not
   currently have on any file) or reuse the `fixture/5` format for it,
   since I did not want a single deliberately-divergent case silently
   folded into the "100 fixtures compared" count in a way that looked like
   a corpus failure. Whether this probe is representative of real DSL usage
   (does any real program ever accidentally write a negation cycle, or does
   the surrounding tooling make that unreachable before it reaches the
   evaluator) is outside this lab's scope; I only checked that the DSL
   grammar and check_program do not statically reject it before
   stratification runs (they do not, `check_program/1` never calls
   `stratify_level_rules`).

4. **I did not check whether SWI's tabling has an explicit well-founded /
   3-valued mode that WOULD refuse this program (or return `undefined`
   correctly) instead of silently picking `[p, q]`.** `01-core.md` flagged
   monotonic and incremental tabling specifically; it did not survey
   `:- table Goal as (incremental, wfs)`-style options or
   `answer_count_restraint`/completion flags. If such a mode exists and
   defaults off, the tripwire finding might be "wrong default," not "no
   capability", I have not verified either way, and I am not asserting one.

5. **The microbench is unit-test-scale and I do not think it is
   informative about the workload that would actually matter.** All three
   heaviest fixtures are still only tens of thousands of inferences; the
   scenario incremental tabling is supposed to win (many ticks, a large
   derived table, a small delta each tick) is not represented anywhere in
   this corpus. I would not use these three numbers to argue performance
   either way at production scale.

6. **Base-predicate discovery reuses `level_eval:goal_rel_refs/3` for a
   purpose it was not written for** (finding which non-level-headed rels a
   body reads, so they can be declared `dynamic ... as incremental` with
   zero rows up front, instead of computing stratum constraints). This
   worked on every fixture in the corpus, but I have not proven it is
   complete for every goal shape the DSL grammar allows versus only the
   shapes that appear in `conformance/fixtures/*.pl` today.
