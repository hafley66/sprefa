# Ordered aggregate lab — header (planner-seeded contract)

User direction (2026-07-30): "we are going to need aggregation so i guess we
do need to figure this out bc v5 got it figured out, dont trust arch go back
into a lab." So: do NOT execute the priced ARCH row as-is; re-derive the
ordered string/array aggregate design from evidence, v5-first, in a lab.

## v5 receipts (coordinator-scouted 2026-07-30 — verify, don't assume; line
## numbers may drift, re-find by symbol)

- `src/ast.rs:439` `AggFn` = Count / Sum / Min / Max / JsonGroupArray /
  JsonGroupObject. There is NO group_concat anywhere in v5 src.
- `src/lower.rs:1016` lowers `json_group_array(arg)` to
  `json_group_array({arg} ORDER BY {arg})` — element order is **sorted by the
  value itself**, not insertion order. The comment states why: arbitrary
  aggregate order would flap the rel's content digest every tick and force
  spurious daemon rebuilds. Determinism-by-value-sort is v5's whole answer to
  "where does order come from".
- `src/lower.rs:1036` two-arg `json_group_object(key, value ORDER BY key)`.
- Interned head columns wrap the result: `sprf_sym_intern(json)`.
- `src/lower.rs:1005` the SUM-NULL hazard: SUM over zero rows = SQL NULL,
  NULL never dedups under INSERT OR IGNORE, fixpoint diverges; pinned via
  `COALESCE(SUM(x), 0)`. The standing hazard class for aggregate empties.
- Wildcard args are named refusals (`json_group_array(_) has no value...`).
- Real v5 usage: `examples/json-out.dl` (per-group arrays, nested
  array-of-objects via `json(payload)` re-parse at :32),
  `examples/chaos-soak.dl:140`.
- Driver receipt: @libsql/client 0.17.4 embeds SQLite **3.45.1**; aggregate
  ORDER BY works (`select json_group_array(x order by x)` over {3,1,2}
  returned `[1,2,3]`, run in-session). The SQL mechanism is free.

## The 4 sightings (why this construct exists)

1. **self-map mermaid assembly**: the python printer exists only because dl6
   cannot join lines in order into one text cell.
2. **v5 collect parity**: json-out.dl / chaos-soak.dl are unportable.
3. **json agg heads refused in tsv2**: the encoding half was CLOSED by ruling
   `json_ticklog_encoding = canonical_json_text`; the emission refusal was
   explicitly deferred to "a later arc" — this lab is that arc's design step.
4. **extract-t2 round-trip**: rebuilding an openapi fragment needed ordered
   assembly; the lab worked around it.

## Questions

- Q1 **order axis**: v5 sorts BY VALUE. The sightings mostly want ORDINAL /
  insertion order (mermaid lines, log rebuild). Prolog referents: `setof` =
  sorted-by-value, `findall` = generation order — both are real prior art.
  Grade BOTH axes: value-sort (v5 parity, zero extra column, digest-stable)
  vs explicit order-column two-arg spelling (the stream-lab ordinal machinery
  supplies the column naturally). State whether one construct covers both or
  two spellings are honest. Vocabulary law: rxjs/prolog/SQL words only —
  `json_group_array`/`group_concat` (SQL), `toArray`/`reduce` (rx),
  `setof`/`findall` (prolog) are the legal name pool.
- Q2 **string vs array**: is string join its own aggregate
  (`group_concat(X, Sep)`) or a use-site join over the json array? Verify
  whether SQLite 3.45.1 group_concat accepts ORDER BY inside the aggregate
  (3.44+ should); price both routes against the mermaid sighting.
- Q3 **incremental emitter shape**: min/max precedent = group-scoped delete
  recompute (1-of-N-groups touched, EXPLAIN SEARCH receipts in the
  expression-lift landing). An ordered array is almost certainly the same
  tier (any delta in a group recomputes that group's cell). The previously
  priced blocker — where the inner ORDER BY lives in the flat aggregate
  SELECT — must be resolved with REAL hand-drafted emitted SQL run against
  @libsql, statements flat across group counts, group-scoped receipts.
- Q4 **tick-log grading**: the canonical-JSON contract means the array cell
  renders as canonical JSON text. Draft the oracle-side ordering (`msort` for
  value axis, `keysort` on ordinal for the other) and prove byte equality
  oracle-draft vs SQL-draft on a hand pair, BOTH axes.
- Q5 **nesting**: json-out.dl builds array-of-objects by re-parsing text
  through `json(payload)`. Does the tsv2 json plane compose (a json column
  fed into an array aggregate) or is there a text/json coercion pothole?
  Grade at current HEAD; the json-potholes lane is IN FLIGHT and owns typed
  captures — name overlaps, do not touch its files.
- Q6 **retraction**: aggregate head over a log rel under keep(count(N)) —
  does group recompute stay correct on minus deltas (P3 support machinery)?
  Hand-graded scenario, not compiler work.

## Method

- Hand-write the emitted-SQL drafts and toy programs; grade against @libsql
  directly (node scripts inside the lab dir) and against `dl6_oracle.pl`
  where expressible TODAY. json agg heads currently refuse in the compiler —
  those legs are draft-only and the verdict says so per leg.
- Sighting census: each of the 4 sightings rewritten as the dl6 program it
  WANTS to be, with the aggregate in place, tagged with which Q1 axis it
  needs. This table is the demand evidence the wiring arc will consume.

## Fences (hard)

- Writes ONLY under `v6/prolog/labs/ordered_aggregate/**` and
  `plans/2026-07-30-ordered-aggregate-verdict.md`.
- NO edits to registry.pl, lower.pl, parse_dl.pl, print_dl.pl, the emitter
  TS, or the grammar. The json-potholes lane owns overlapping files right
  now, and WIRING is the follow-up arc, never this lab.
- No-commit flow: leave the tree dirty; the coordinator reviews and commits.

## Named slots

- slot_order_axis (value vs ordinal vs both-spellings)
- slot_string_join_spelling (own aggregate vs join-over-array)
- slot_empty_group (NULL vs absent row vs `[]` — the SUM-NULL class)
- slot_nested_array (json-column-into-aggregate composition)
- slot_incremental_tier (group-scoped recompute vs naive-fallback refusal)

## Receipts required to land

- v5 parity receipt: json-out.dl's `group_rels` shape reproduced by a
  hand-drafted tsv2-style SQL emitting byte-identical canonical JSON.
- Q1 both-axis grading table; Q3 EXPLAIN + flat-statement receipts; Q4
  byte-equality pair both axes; the sighting census table.
- Verdict `plans/2026-07-30-ordered-aggregate-verdict.md`; lab files die on
  landing (last-copy hash recorded).
