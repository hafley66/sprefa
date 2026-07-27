# Review: temporal_pipe lab (`|>`)

Reviewer run: `swipl -q -l v6/prolog/labs/temporal_pipe.pl -g go -g halt` observed
25 PASS, exit 0, matching the lab's claim (temporal_pipe.md:3).

## 1. Construct cardinality

| construct | verdict | audit reconciliation |
|---|---|---|
| `\|>` operator | **collapse to sugar**, contingent on the R5 ruling. Desugars to N ordinary rules (temporal_pipe.pl:78-92); registers as `sugar(temporal_pipe, [rule, ground_terms])` if R5 lands the positional rule, or `[rule, ground_terms, trigger_marker]` if R5 lands a marker (temporal_pipe.md:418-437). No kernel entry either way. | AUDIT postdates nothing here; the pipe is not in the keep/kill table. It partially replaces what AUDIT killed at finding 6 (any-atom edge firing, AUDIT.md:734-736 "kill") by supplying single-atom triggers at boundaries only. |
| generated intermediate rels (`pipe_<head>_<k>`) | **instance**, not a construct. They are ordinary edge rels; nothing about them is new machinery. The naming law (linear intermediates never earn names) is honored in the surface, where the author writes none, and leaked in the store: the rows appear in the delta trail (`+pipe_change_log_1(...)`, check pipe_stage_costs_one_tick, temporal_pipe.pl:722) and would appear as sqlite tables and in error text. Requires a reserved namespace or gensym; the lab's own deviations table names the collision (temporal_pipe.md:463). | Consistent with AUDIT's silence: no new row needed in the keep/kill table. The retention question (AUDIT finding 10) applies to them like any edge rel; a pipe intermediate is append-only and unbounded until retention lands. |
| `trigger_marker` (`only/1`) | **one missing construct, circled by three labs.** The eventing lab's per-atom trigger need, the killed `delta()` (AUDIT finding 6, AUDIT.md:240-278), and this lab's `only/1` are the same thing: a per-atom arrival qualifier in an edge body. R5 (lab-consolidation.md:47-49) already names the ruling. The pipe does not add a fourth variant; it is a client that GENERATES whichever form R5 picks. Recommendation below. | AUDIT finding 6 resolution options 1 (explicit marker) and 2 (first-atom positional) are exactly the two exits the lab's sugar/2 section lists (temporal_pipe.md:425-437). The registry's refusal to accept `trigger_marker` unregistered (temporal_pipe.md:420-424) is the registry working as intended for once, against the drift AUDIT finding 1 documents. |
| rel storage marker (declarable edge-headed-ness) | **new construct, and the pipe is its third independent demander, not its origin.** The boundary law's `edge_append` evidence needs to know a rel is append-only; today that is a property of OTHER rules' arrows, making the check non-local (temporal_pipe.md:252-258, ambiguity 7). AUDIT finding 5 already proposed typing the rel `Set` vs `Log` (AUDIT.md:230-232), finding 10 wants retention bounds hung on exactly the `Log` rels, and R2 (lab-consolidation.md:37-40) rules "the rel's key owns the STORAGE effect", which is the same declaration site. Adopt a per-rel storage kind in the declaration; four consumers, one construct. | Directly extends AUDIT's finding 5 option 2 and the keep/kill row "`<+` kill as specified, respecify". |

On R5 itself, since the mandate asks: the positional rule ("first body atom is the
trigger") is cheaper and the pipe generates conformant rules for free
(temporal_pipe.md:434-437). Its cost is that comma-body atom order, currently
meaning-preserving, becomes semantic, and a reorder is a silent behavior change,
the same failure shape as the dot-space truncation this lab grades. The explicit
marker costs a surface form and buys an error instead of a silent change. This
review leans marker, stated once, with the pipe as its main generator; the
positional rule is defensible if the checker forbids reordering-equivalent
duplicate rules. Either way R5 should be ruled before the pipe registers.

## 2. Tier placement

**The lab's T4 claim is right; its T5 gate is overstated; the one-tick cost does
not belong to the pipe.**

- Desugaring is a compile-tier rewrite gated on the declaration table (Key decls,
  effect signatures, storage kinds), proven by
  `cut_law_depends_on_declarations` (temporal_pipe.pl:786-794): same chain text,
  verdict flips on one `append/1` fact. Checked rewrite, not runtime machinery.
- The one-tick-per-boundary cost belongs to edge-chaining semantics, not to the
  pipe: `desugared_trace_equals_hand_written` (temporal_pipe.pl:709-715) shows the
  hand-written 3 rules land `change_log` at tick 5, same as the pipe. Under any
  R9 ruling the pipe and its expansion cost identically. The pipe adds zero
  runtime semantics; its tier is wherever the arrows are, which the topology doc
  places at T4 (tier-topology.md:56-60). Topology doc confirmed.
- The blanket "not shippable before T5" (temporal_pipe.md:441) is wrong for
  non-effect chains: program `pipe_declared_cut` (temporal_pipe.pl:546-550)
  desugars legally with an `edge_append` cut and no effect anywhere. Correct
  statement: pipes with edge/key cuts ship at T4; `yield` cuts additionally need
  the T5 effect-signature DECLARATION in scope (not T5 runtime).

## 3. Lab-specific assessments

### 3a. One tick per boundary: graded, and the law survives without a fusion variant

The grading is real: `pipe_stage_costs_one_tick` (temporal_pipe.pl:719-727) pins
the response intermediate to tick 3, the fold to tick 4, the append to tick 5,
and asserts `change_log` is empty at ticks 3-4.

The fusion question, head on. The rx analogy (sync operators fuse in one
microtask) does not transfer cleanly: an rx pipe is per-value and linear; a `|>`
stage joins its trigger against standing sets and its intermediate is a durable
occurrence rel. The tick is the price of the trail. Whether the price is
acceptable is a corpus question, and the corpus answers it:

- The LSP/diag hot path is level-only: AUDIT finding 6, "55 of 55 diag-heading
  files are level-only" (AUDIT.md:269). No pipe appears on the latency-sensitive
  path.
- Edge chains live in the effect minority: 12/173 files `@async` (AUDIT census,
  AUDIT.md:30), driven by 300s polls where a tick of latency is noise.
- v5 already pays per-boundary ticks on the flagship chain: gh-cache.dl's two
  `@next` carries (AUDIT finding 7, citing examples/gh-cache.dl:103-104) each
  cost a tick, and v5 lived with it.

So there is no chain in the corpus today where per-boundary latency bites, and a
5-stage pipe does not exist in the corpus at all. Recommendation: **keep the law
as stated, every `|>` is one tick; do not add a same-tick pipe variant now.**
Record fusion as a rejected-for-now alternative with a reopen condition: a
measured receipt of a real chain whose drain latency matters (an SSE consumer is
the likeliest candidate, since `sse <+ subscriber, change_log` adds one more hop
after the append).

If fusion is ever needed, the shape is known and narrow: distinguish `yield` cuts
(world-imposed, irreducibly multi-tick) from commit cuts (engine-imposed), and
run commit cuts as within-tick strata in dependency order. That is decidable for
pipe-generated chains precisely because the generated dependency graph is a
straight line; it is not available as a general edge-feeds-edge rule, because a
self-referential edge rule under cascade-to-fixpoint does not terminate within a
tick. Which is also the strongest argument for 3b's ruling.

One unstated obligation the lab's schedule hides: `feed_ticks` hand-supplies two
empty arrival ticks (temporal_pipe.pl:485-486) to drain the carry. Under next-tick
semantics the ENGINE must self-schedule drain ticks while the carry set is
nonempty, or a chain freezes mid-flight when outside arrivals stop. That
scheduler is new machinery no document names. It belongs in R9's text. The carry
set is reconstructable from the delta trail restricted to edge-headed rels, so it
survives a crash without new persistent state.

### 3b. Ambiguity 6: the stall is real; recommend next-tick propagation as R9

Confirmed by inspection of the copied interpreter. merge_family.pl:175 computes
`Arrived` as `ord_subtract(MidAll, StartAll)`; `MidAll` holds outside arrivals
plus level closure only, edge writes land afterwards (merge_family.pl:179) and by
the next tick sit inside `StartAll`, so a row written by an edge rule is never in
any tick's `Arrived` set. A downstream edge rule triggered on it never fires;
under the any-atom rule it fires only if some unrelated body atom happens to
arrive later, which is worse than a clean stall (nondeterministic-looking
delivery). temporal_pipe.pl:347/:356 is the one-change fix: `written_rows` become
`CarryOut`, appended into the next tick's `Arrived`.

The two candidate semantics against R7 (one tick, one delta set, diffed at the
boundary; lab-consolidation.md:53-55):

- **Same-tick cascade to fixpoint.** R7's letter survives (the fixpoint's net is
  still one delta set), but: `|>` becomes free, deleting the boundary law's own
  justification (the lab's ambiguity 6 says this itself, temporal_pipe.md:366-371);
  within-tick write ordering questions return (R1's territory); and a
  self-referential edge rule makes the cascade non-terminating inside a
  transaction, an unbounded tick.
- **Next-tick propagation** (the lab's carry). Each tick is one round of edge
  firing, trivially terminating; every intermediate delta lands in its own
  tick's delta set, so the trail names each hop (the self-diagnosis law likes
  this); R7 holds without qualification. Cost: N-1 drain ticks plus the drain
  scheduler from 3a.

**Recommend R9 = next-tick propagation**: rows written by edge rules at tick T
commit at T's boundary and are arrivals for tick T+1; the engine runs drain
ticks (empty outside-arrival set) while the carry is nonempty. This must be
ruled with or without the pipe, as the lab says (temporal_pipe.md:371): any
hand-written edge-feeds-edge program hits it.

### 3c. Dot-space truncation: severe for the lab methodology, moderate for the language

The break is graded (`dot_access_truncates_on_space`, temporal_pipe.pl:637-642):
one space after `.` yields a legal shorter chain plus a stray clause, no
diagnostic anywhere. Two severities to keep apart:

- For prolog-hosted prototyping: severe. Any lab exercising dot access cannot
  even quote the shape it grades (the lab builds it with `=..`,
  temporal_pipe.pl:424-425). This confirms prolog-as-reader is dead for this
  construct family, cumulative with `|>`, `!rel`, `{ }` patterns, `match`.
- For the shipped language: moderate, because the actual surface terminates
  clauses with `;` (LANG.md:17 examples), so `.` is not end-of-statement there
  and the specific truncation cannot reproduce. The lab's framing of this as the
  decider for `surface_dcg` (temporal_pipe.md:106-109) overstates it slightly;
  `surface_dcg` was already owed (lab-consolidation.md:90-91).

What it adds to the `surface_dcg` requirement list, concretely: (1) the lexer
owns `.` entirely, whitespace around it is either insignificant or an error,
never meaning-changing; (2) an adversarial test family: no single-character
perturbation of a legal program may yield a different legal program silently.
Requirement (2) is the general law this incident instantiates and is cheap to
state now.

## 4. Wrong or overclaimed

- temporal_pipe.md:441 "not shippable before T5": overstated, see section 2.
  `pipe_declared_cut` (temporal_pipe.pl:546-550) is a legal effect-free chain.
- temporal_pipe.md:176 "`|>` is latency, made syntactic": stated as a finding,
  but it is contingent on the lab's own deviation (the carry change to the tick,
  deviations table temporal_pipe.md:462). Under the cascade alternative the
  sentence is false. The .md is honest about the dependency in ambiguity 6 but
  the section 2a prose asserts it unconditionally.
- The drain-tick obligation is smuggled in via the arrival schedule
  (temporal_pipe.pl:485-486) and never named as engine machinery. See 3a.
- Minor, ungraded: `cut_evidence/6` tries `yield` before `edge_append`
  (temporal_pipe.pl:130-135), so a stage mentioning both an effect rel and an
  append rel is classified `yield` by clause order. Probably the right priority;
  currently an accident of clause ordering, not a stated rule.
- Minor: the last-boundary head-evidence clauses (temporal_pipe.pl:136-143) let
  the FINAL `|>` be justified by the destination's storage kind while every
  earlier boundary needs source-stage evidence. The asymmetry is correct under
  R9 (the commit boundary is a real cut) but the .md never states why the last
  boundary is special.

Nothing found that invalidates a graded check; all 25 re-verified.

## Disposition

**Accept with notes.** The lab is honest about its deviations, the grading is
real, the trace-equality methodology (pipe vs hand-written vs unmarked) is the
right instrument, and its two structural findings (the boundary law is
declaration-dependent; edge-write arrival semantics is undecided) are both
genuine and both land on pre-existing open rulings rather than minting new ones.
The notes: fix the T5 gate claim, name the drain-scheduler obligation, and mark
the latency finding as contingent on R9.

Reviewer's own stance on `|>`, independent of the lab's conditional yes:
**conditional adopt, as sugar.** Conditions: (1) R5 ruled first, marker
preferred, pipe generates it; (2) the per-rel storage kind lands in the
declaration (AUDIT finding 5 / R2, which the boundary check needs to stay
local); (3) R9 ruled as next-tick propagation with the drain scheduler named;
(4) reserved namespace for generated intermediates. Under those four the pipe
adds no primitive, costs nothing at runtime beyond what its expansion already
costs, and makes the temporal minority of the corpus visibly temporal, which is
the one styling claim of the proposal that survives scrutiny intact.
