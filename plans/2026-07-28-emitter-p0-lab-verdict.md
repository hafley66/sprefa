# EMITTER P0 LAB VERDICT

Lab: `v6/prolog/labs/emitter_p0/`.

Run:

```bash
swipl -q -l v6/prolog/labs/emitter_p0/0_lab.pl -g go -g halt
```

`0_lab.pl` emits eight TypeScript modules, one inline and one helper spelling
for each statement family. `2_grade.ts` runs every emitted program through
the existing tsv2 `ScratchStore` and `TickFold`, obtains the reference bytes
from `v6/prolog/conformance/ticklog.pl`, compares the complete JSONL text, and
writes `receipts.json`. Each SQLite file is created below `$TMPDIR` and removed
at the end of the run. The shipping recompute emitter, `compile/lower.pl`, and
the runtime files remain unchanged.

## Fixture selection

| fixture | coverage reason |
|---|---|
| `fork_join_is_a_conjunctive_body` | Non-recursive level rule whose two set inputs form a join. |
| `repeat_is_a_self_carry_chain` | Recursive stratum with one frontier hop per drain tick. |
| `departed_fires_next_tick_on_retraction` | Set arrival removal, level-row departure, and the next-tick `finalize/1` edge. |
| `demand_view_fires_its_consumer_once` | Edge trigger fed by a set view over duplicate log arrivals, which grades assertion-side `DISTINCT` placement. |

## Per-family verdict

Linecount is the generated module only. Shared lab runtime lines are excluded.
Statement counts include the boundary-stream read that constructs
`ITickDeltas`. Counts are constant for each tick shown and contain no
arrival-row loop.

| statement family | verdict | inline linecount | helper linecount | byte identity | perf receipt | readability |
|---|---:|---:|---:|---|---|---|
| Semi-naive delta join | mixed | 34 | 7 | Both spellings: fork join 193/193 bytes across 3 ticks; recursive carry 215/215 bytes across 4 ticks. | Both spellings: 8 statements/tick on each fixture. `fork_delta` and `repeat_pulse_delta` plan as indexed `SEARCH`; the fork fixture also scans its unconstrained current-side input. | Inline text exposes each join arm and frontier predicate; the helper call is shorter but reconstructs SQL whose joins and projections vary by rule. |
| Count-IVM support maintenance | helper | 42 | 7 | Both spellings: 292/292 bytes across 4 ticks. | Both spellings: 15 statements/tick. Support deltas use `SEARCH count_source_delta USING INDEX count_source_delta_batch`; the recursive-CTE reachability referee runs once per tick. | The helper call names the delta and support tables; the inline file repeats transition and recursive-CTE text common to this storage shape. |
| `DISTINCT` placement | mixed | 37 | 7 | Both spellings: demand edge 236/236 bytes across 2 ticks; departure edge 292/292 bytes across 4 ticks. | Both spellings: 9 statements/tick for demand, 13 for departure. Assertion and retraction delta reads both plan as indexed `SEARCH`. | The keyword is easiest to audit in the specialized assertion or retraction query, while execution and batching remain shared. |
| Boundary diff from delta stream | helper | 16 | 6 | Both spellings: 292/292 bytes across 4 ticks. | Both spellings: 12 statements/tick. `change_stream` reads use `SEARCH change_stream USING INDEX change_stream_batch`; zero full-table snapshots execute. | The helper call states the stream table once and removes repeated select, ordering, parse, and grouping text from generated files. |

The mixed rows split the generated surface at the SQL boundary:

- Semi-naive rule joins and projection lists stay inline; statement execution,
  batch handling, and frontier iteration use one shared helper.
- `DISTINCT` stays visible in the specialized assertion and retraction SQL;
  stream execution and result shaping use one shared helper.

## Byte-identity receipts

| family | spelling | fixture | receipt |
|---|---|---|---|
| Semi-naive delta join | inline | `fork_join_is_a_conjunctive_body` | 3 ticks, oracle 193 bytes, actual 193 bytes, identical |
| Semi-naive delta join | inline | `repeat_is_a_self_carry_chain` | 4 ticks, oracle 215 bytes, actual 215 bytes, identical |
| Semi-naive delta join | helper | `fork_join_is_a_conjunctive_body` | 3 ticks, oracle 193 bytes, actual 193 bytes, identical |
| Semi-naive delta join | helper | `repeat_is_a_self_carry_chain` | 4 ticks, oracle 215 bytes, actual 215 bytes, identical |
| Count-IVM support maintenance | inline | `departed_fires_next_tick_on_retraction` | 4 ticks, oracle 292 bytes, actual 292 bytes, identical |
| Count-IVM support maintenance | helper | `departed_fires_next_tick_on_retraction` | 4 ticks, oracle 292 bytes, actual 292 bytes, identical |
| `DISTINCT` placement | inline | `demand_view_fires_its_consumer_once` | 2 ticks, oracle 236 bytes, actual 236 bytes, identical |
| `DISTINCT` placement | inline | `departed_fires_next_tick_on_retraction` | 4 ticks, oracle 292 bytes, actual 292 bytes, identical |
| `DISTINCT` placement | helper | `demand_view_fires_its_consumer_once` | 2 ticks, oracle 236 bytes, actual 236 bytes, identical |
| `DISTINCT` placement | helper | `departed_fires_next_tick_on_retraction` | 4 ticks, oracle 292 bytes, actual 292 bytes, identical |
| Boundary diff from delta stream | inline | `departed_fires_next_tick_on_retraction` | 4 ticks, oracle 292 bytes, actual 292 bytes, identical |
| Boundary diff from delta stream | helper | `departed_fires_next_tick_on_retraction` | 4 ticks, oracle 292 bytes, actual 292 bytes, identical |

## Query-plan receipts

Exact plan details are in `receipts.json`. The asserted delta-side steps are:

```text
SEARCH delta_result_a USING COVERING INDEX fork_delta_batch_rel (batch_id=? AND rel=?)
SEARCH previous_frontier USING COVERING INDEX repeat_pulse_delta_batch (batch_id=? AND value<?)
SEARCH count_source_delta USING INDEX count_source_delta_batch (batch_id=?)
SEARCH distinct_stale_delta USING COVERING INDEX distinct_stale_delta_batch (batch_id=?)
SEARCH distinct_source_delta USING COVERING INDEX distinct_source_delta_batch (batch_id=?)
SEARCH change_stream USING INDEX change_stream_batch (batch_id=?)
```

Both spellings produce the same plans and statement counts for every family.

## Named cracks

### `CURRENT_COMPILER_GATE_CRACK`

`compile_fixture/4` accepts the fork-join fixture. The current supported-subset
gate refuses `repeat_is_a_self_carry_chain` at its comparison and bind, refuses
`departed_fires_next_tick_on_retraction` at `finalize/1` plus `now/1`, and
refuses the demand fixture because its edge trigger is derived. The P0
generator therefore emits fixture-specific modules directly. The grader
reuses the same oracle, SQLite seam, tick fold, and byte comparator as the
sweep. General lowering from arbitrary registry-shaped plans remains work for
P1 through P3.

### `COUNT_CYCLE_RESEED_CRACK`

The selected departure fixture has a direct acyclic support path. The support
transition reaches byte identity, and every tick runs a recursive-CTE
reachability referee patterned after the retraction verdict. A cyclic rule
graph is absent from the fixture corpus used here, so this lab does not claim
support counts alone handle cycles. P3 needs the recursive-CTE reseed mutation
for cyclic components.

### `CARTESIAN_CURRENT_SCAN_CRACK`

`fork_join_is_a_conjunctive_body` is an unconstrained Cartesian join. SQLite
uses the required indexed `SEARCH` on `fork_delta` and scans
`fork_result_b` on the current side because the rule has no equality
predicate. The lab assertion is scoped to the delta side, matching the P0
grade.

## Files

- `0_lab.pl`: Prolog emitter and one-command entry point.
- `1_runtime.ts`: isolated program builders, shared helpers, batched SQL, and
  receipt counters.
- `2_grade.ts`: oracle comparison, generated-file linecount, statement
  counts, query-plan assertions, and temporary database cleanup.
- `generated/*.ts`: the eight emitted modules graded above.
- `receipts.json`: machine-readable output for every number in this verdict.
