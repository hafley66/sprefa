# Type-matrix lab: verdict

Header: `plans/2026-07-30-type-matrix-lab-header.md`. Base sha `5ce31a24`.
Lab: `v6/prolog/labs/type_matrix/**` (dies on landing; the coordinator records
the last-copy hash). Regenerate everything with one command:

```
bash v6/prolog/labs/type_matrix/matrix.sh
```

That runs `gen_cells.mjs` (enumerate the axes, write one `.dl6` and one
schedule per cell), `drive.mjs` (four child processes per cell: the text door
`compile:compile_dl6/2`, both `.dl6` oracle doors, and the emitted module under
BOTH emitter modes via the unmodified `v6/tsv2/scripts/golden-run.ts`), and
`classify.mjs` (the four verdicts, byte comparisons only). Generated table:
`v6/prolog/labs/type_matrix/MATRIX.md`; machine form `matrix.json`.

No cell was hand-graded. No compiler, oracle, registry, golden or conformance
file was edited.

## Cell counts

422 cells = 420 (position × declared type × fed value) + 2 supplementary
seeds. **Constructible 422 / n-a 0 / not run 0.**

| verdict | cells |
|---|---|
| DIVERGENT | 168 |
| NAMED REFUSAL | 116 |
| IDENTICAL | 73 |
| SILENT COERCION | 65 |

| verdict / label | cells | what it means |
|---|---|---|
| NAMED_REFUSAL / compiler_only | 86 | the compiler refuses by name, the reference engine computes (the sweep's standing `unsupported` class, not a divergence) |
| IDENTICAL / lossless | 73 | all three runs byte-identical AND the value survived |
| DIVERGENT / doors_disagree | 63 | oracle vs emitter byte difference |
| DIVERGENT / emitter_modes_disagree | 50 | the two EMITTER MODES disagree with each other |
| SILENT_COERCION / value_changed | 44 | every door agrees; the value that came out is not the value that went in |
| DIVERGENT / oracle_only_refusal | 36 | the reference engine refuses the arrival; the compiler accepts and the emitter stores it |
| NAMED_REFUSAL / both | 30 | both doors refuse, same name |
| SILENT_COERCION / row_absent | 21 | every door agrees the row simply is not there |
| DIVERGENT / emitter_run_error | 19 | the emitted module throws |

**Zero cells are n/a.** That is itself the first finding: the surface accepts
every corner of this cross product as TEXT. Nothing is stopped by the grammar,
so every guard has to come from a door, and 168 of them do not.

### The shape of the damage, per axis

| position | IDENTICAL | SILENT_COERCION | DIVERGENT | NAMED_REFUSAL |
|---|---|---|---|---|
| arrival | 10 | 9 | **51** | **0** |
| join_column | 10 | 9 | **51** | **0** |
| level_head | 22 | 10 | 38 | 0 |
| aggregate_head | 10 | 10 | 15 | 35 |
| json_capture | 6 | 23 | 11 | 30 |
| edge_head | 15 | 4 | **2** | **49** |

The edge head is the only position in the language that is LOUD about a
declared-type conflict, and it is the only position with essentially no
divergence. The arrival and join positions carry no type guard at all and
carry 51 divergences each. That single row pair is the whole verdict of this
lab compressed.

| fed value | IDENTICAL | SILENT_COERCION | DIVERGENT | NAMED_REFUSAL |
|---|---|---|---|---|
| wide_int | **0** | 0 | 34 | 8 |
| float_integral | **0** | 17 | 17 | 8 |
| neg_zero | **0** | 17 | 17 | 8 |
| bool | 4 | 3 | 19 | 16 |

Three of the ten values have **zero** identical cells anywhere in the matrix.

## Ranked DIVERGENT list

Ranked by blast radius: how many cells, and how reachable the shape is in a
program someone would actually write.

### DIV-1: the incremental emitter drops an arrival delta whenever SQLite affinity rewrites the value (50 cells, DEFAULT path, silent)

The trophy. Program, in full:

```
rel probe_in(value).
rel probe_out(value).

probe_out(value) <- probe_in(value).
```

Arrival: `probe_in(4)`. Receipts, same program and schedule:

| door | tick log | final state |
|---|---|---|
| oracle | `{"tick":1,"deltas":{"probe_in":{"add":[[4]],...},"probe_out":{"add":[[4]],...}}}` | `probe_in [[4]], probe_out [[4]]` |
| emitter, incremental (DEFAULT) | `{"tick":1,"deltas":{}}` | `probe_in [["4"]]`, and **`probe_out` never exists** |
| emitter, naive | `{"tick":1,"deltas":{"probe_in":{"add":[["4"]],...},"probe_out":{"add":[["4"]],...}}}` | both rels |

No declared types, no refusal, no error. The whole rule graph goes silent on
the default path.

Rx lowering of the program, so the intent is on the record:
`const probeOut$ = probeIn$;`, an identity projection over the delta stream,
`probeIn$.pipe(distinctUntilChanged(sameMultiset))` under the boundary-diff
contract. Nothing in that lowering can drop a row.

**Mechanism, located.** `v6/tsv2/runtime/1_incremental.ts:728-734`:

```ts
const changedRows = new Set(
  resultRows(result, relation.columns).map((row) => JSON.stringify(row)),
);
const stagedRows = new Set<string>();
for (const entry of entries) {
  const row = JSON.stringify(entry.row);
  if (!changedRows.has(row) || stagedRows.has(row)) continue;   // <-- drop
```

`changedRows` comes from the arrival statement's `RETURNING`, i.e. the value
AFTER SQLite applied column affinity: `"4"`. `entry.row` is the value as it
came off the wire: `4`. `JSON.stringify([4])` never equals
`JSON.stringify(["4"])`, so the arrival is filtered out of the delta stream
while the row sits in the table. Verified in `sqlite3` directly: inserting
`json_extract('[[4]]','$[0]')` into a `TEXT` column and reading the
`RETURNING` clause gives `text|4`.

Reach: every declared type × every value whose JS type differs from the
column's storage type, at the two positions where the world writes
(`arrival`, `join_column`). Includes the zero-declaration case above.

### DIV-2: a head column's declared type is a CAST on the emitter and a no-op on the oracle (63 cells)

The header's proof incident, generalized, and it runs both directions.

```
rel probe_in(value: int).
rel probe_out(value: text).

probe_out(value) <- probe_in(value).
```

Rx lowering: `probeIn$.pipe(map((row) => row))`. The declared head type is
not a conversion in any rx reading, which is the side the oracle takes.

Receipts (fed value → oracle graded value vs emitter graded value):

| declared head | fed | oracle | emitter |
|---|---|---|---|
| `text` | int `4` | `4` | `"4"` |
| `int` | text `"4"` | `"4"` | `4` |
| `int` | json `{"key":1}` | `{"key":1}` | `"{\"key\":1}"` |
| `text` | json `[1,2]` | `[1,2]` | `"[1,2]"` |
| `bool` | int `4` | `4` | row ABSENT |
| `bool` | float `1.0` | `1` | `true` |
| `bool` | float `-0.0` | `0` | `false` |
| `float` | json `{"key":1}` | `{"key":1}` | row ABSENT |

46 of the 63 are a recast, 17 are the row vanishing on the emitter.
Distribution: `level_head` 31, `json_capture` 10, `aggregate_head` 8,
`arrival` 7, `join_column` 7.

**The asymmetry that makes this fixable**: the same conflict at an EDGE head is
already `unsupported_construct(edge_head_column_type_mismatch(Ref, Pos, Body,
Head))`, 49 cells, loud, both doors. The level head, the aggregate head and
the arrival have no equivalent.

Sub-case worth its own name (8 cells): an UNTYPED json capture.

```
rel probe_in(payload: json).
rel probe_out(value).

probe_out(captured) <- probe_in(payload), decode(payload, {field: captured}).
```

Rx lowering: `probeIn$.pipe(map(({payload}) => payload.field))`. The oracle
keeps the json value's own type (`4`, `1.5`, `{"key":1}`, `true`); the emitter
types the hole `text` and yields `"4"`, `"1.5"`, `"{\"key\":1}"`, `"1"`.
`SYNTAX.md` already states the emitter half ("lower.pl types a bare hole
`text`"); nothing states that the oracle does not.

### DIV-3: `dl6_oracle.pl` cannot carry a float or a boolean schedule value at all (92 cells)

```
rel probe_in(value: float).
rel probe_out(value: float).

probe_out(value) <- probe_in(value).
```

Schedule `[[{"rel":"probe_in","sign":"add","row":[1.5]}]]`.

| door | answer |
|---|---|
| `dl6_oracle.pl` | `type_arrival_shape_mismatch(probe_in/1,value,float,field_not_finite_float('1.5'))` |
| `golden_oracle.pl` | `{"tick":1,"deltas":{"probe_in":{"add":[[1.5]],"del":[]}, ...}}` |

`compile/scripts/dl6_oracle.pl:136-139` is the whole of it:

```prolog
schedule_value(Rel, json, Value, Term) :- !, json_column_term(Rel, Value, Term).
schedule_value(_, _, Value, Value) :- integer(Value), !.
schedule_value(_, _, Value, Atom) :- string(Value), !, atom_string(Atom, Value).
schedule_value(_, _, Value, Atom) :- term_to_atom(Value, Atom).
```

No `float/1` clause and no boolean clause, so every float and every bool falls
to `term_to_atom/2` and reaches the engine as the ATOM `'1.5'` / `true`, which
`engine.pl`'s own shape check correctly rejects. `golden_oracle.pl` has both
clauses (`schedule_value(V,V) :- float(V)`, `schedule_value(true,
bool_lit(true))`) and lacks dl6_oracle's type-directed json mapping. **Each
door is missing what the other has.**

Consequence, stated plainly: **no served or text-door program with a float or
bool world column has ever been gradeable.** The lab worked around it by
running both doors and grading a cell against whichever accepted its arrival
(92 cells graded via `golden_oracle.pl`), and the fallback is narrow on purpose
because a wide fallback was measured wrong, because `golden_oracle.pl` also "passes"
every `json_capture_type_unknown` cell VACUOUSLY (its json column stays an
atom, so the capture is never reached) and would have deleted 20 real refusals.

### DIV-4: wide integers fail three different ways on three different paths (34 divergent cells, 0 identical anywhere)

1. **`RangeError` kills the tick pipeline** (19 cells). Any integer at or above
   2^53 stored in an `int` column throws on read-back:
   `RangeError: Received integer which cannot be safely represented as a
   JavaScript number` from `@libsql/client .../sqlite3.js:364` (`valueFromSql`),
   through `sprefa-store/js/src/engine/sqlRunner.ts:20`. Not a refusal, not a
   named answer: an exception out of the driver.
2. **The schedule seam rounds before SQL sees it.** `9007199254740993` in a
   schedule row is `9007199254740992` after `JSON.parse` on the TS side and
   exact in SWI.
3. **Inside a json document the rounding INVERTS.** Same document text
   `{"field":9007199254740993}`: oracle logs `{"field":9007199254740993}`,
   emitter logs `{"field":9007199254740992}`.

Confirms json-flex card C2 at HEAD and extends it: the cliff is not only a json
one, it is any `int` column.

### DIV-5: the reference engine refuses arrivals the compiler accepts and the emitter stores (36 cells)

```
rel probe_in(value: float).
rel probe_out(value: float).

probe_out(value) <- probe_in(value).
```

Fed the integer `4`:

| door | answer |
|---|---|
| oracle (both doors) | `type_arrival_shape_mismatch(probe_in/1,value,float,field_not_finite_float(4))` |
| compiler | compiles clean |
| emitter | stores and logs `4` |

Same shape for every non-bool value at a `bool` column. **Open language
question, not a defect**: should an integer widen to a declared `float`? The
engine says no, the emitter says yes, and nothing in `rulings.pl` says either.
Named as `slot_int_widens_to_float` below.

### DIV-6: the two `.dl6` oracle doors disagree on 120 cells where BOTH accepted the arrival (reported on its own axis)

Not counted in the 168: this is door-vs-door, not door-vs-emitter. Same
program, same schedule, `rel probe_in(value: json)` fed `"{\"key\":1}"`:

| door | tick log |
|---|---|
| `dl6_oracle.pl` | `"probe_in":{"add":[[{"key":1}]]` |
| `golden_oracle.pl` | `"probe_in":{"add":[["{\"key\":1}"]]` |

120 cells, concentrated on the json values (`json_object`/`json_array` at every
declared type) and on the float/bool values that dl6_oracle atom-ises. The
golden lane's in-flight seam fix (GAP 1) closes the json half; the float/bool
half is dl6_oracle's and is DIV-3.

### DIV-7: `dl6_oracle.pl` calls `halt(1)` instead of refusing by name (4 cells)

`rel probe_in(value: json)` fed the text `"north"`:
`dl6_oracle: json column value is not valid json: "north"`, process exit 1, no
term, no `prolog:message//1`. The emitter, on the same cell, drops the row
silently (DIV-8). Three doors, three answers, none of them a refusal a program
can catch. This is also what forced the lab to run one swipl process per cell.

### DIV-8: a CHECK-constraint violation is swallowed by `INSERT OR IGNORE` (4+ cells, silent)

`rel probe_in(value: list(text))` fed the text `"north"`. Emitted DDL:

```sql
CREATE TABLE "probe_in" ("value" TEXT NOT NULL CHECK (json_valid("value")),
                         PRIMARY KEY ("value")) WITHOUT ROWID
```

Emitter output: `{"tick":1,"deltas":{}}` and `{"final":{}}`. The row never
lands, no delta, no error, no refusal. `INSERT OR IGNORE` treats the CHECK
failure as a duplicate. Oracle stores `"north"` and logs it. Any emitted CHECK
is currently a silent row filter.

## Ranked SILENT COERCION list

Every door agrees; the value did not survive.

| # | class | cells | loss |
|---|---|---|---|
| SC-1 | `1.0` renders `1`, `-0.0` renders `0` | 30 | the float/int distinction and the IEEE sign of negative zero, at EVERY declared type including `float` itself (18 of the 30 are at `float` or at an undeclared column) |
| SC-2 | a typed json capture whose json type does not match silently contributes no row | 21 | nothing (this is DEFINED, per `SYNTAX.md`: "A value of the wrong json type contributes no row, exactly as an absent key does"); listed because it is a silent filter a cold author will trip |
| SC-3 | json document text at a non-json column stays a string | 12 | the json-ness; both doors agree, and it is the stated arrival contract |
| SC-4 | a `json` column fed the JSON string `"4"` becomes the NUMBER `4` | 2 | string-ness: the "a json document arrives as its text" contract makes a top-level json STRING document inexpressible |

The five classes sum to 65 (SC-2 is the `row_absent` label, the other four are
`value_changed`).

SC-1 is the one worth a ruling: `float` is a declared type, and a `float`
column cannot represent `1.0` distinguishably from `1`, nor `-0.0` from `0`, on
either door. The canonical-json log contract (ruling
`json_ticklog_encoding = canonical_json_text`) is where that collapse happens
and it does not say so.

## Fail-first fixture candidates, one per DIVERGENT class

Naming only. Fixture promotion is the follow-up arc's job.

One candidate per divergent CLASS, not per divergent cell: a fixture per cell
would be 168 fixtures, and each of these eight goes red on every cell in its
class. The per-cell list is addressable in `matrix.json` (filter on
`label`), so a promoter that wants a wider fixture has the exact set. DIV-6
gets no candidate here: it is door-versus-door, the golden lane already owns
the json half of it, and DIV-3's fixture covers the float/bool half.

| class | candidate fixture | shape | red before, green after |
|---|---|---|---|
| DIV-1 | `arrival_value_survives_affinity_rewrite` | zero-decl `probe_in`/`probe_out` identity rule, one `probe_in(4)` arrival | red on the incremental path (empty tick log), green once the RETURNING comparison is storage-normalized. Also flips the two emitter modes into agreement |
| DIV-2 | `level_head_column_type_is_not_a_cast` | `rel src(value: int)`, `rel dst(value: text)`, `dst(value) <- src(value)`, one int arrival | red today (`4` vs `"4"`), green once a `head_column_type_mismatch` refusal fires on level heads, or once both doors cast identically |
| DIV-2 (capture) | `untyped_json_capture_keeps_its_json_type` | `decode(payload, {field: captured})` into an undeclared head | red on 8 of 10 value types, green once the bare hole carries the json value's type on both doors |
| DIV-3 | `float_and_bool_arrive_through_the_text_door` | `rel reading(celsius: float, ok: bool)` + a schedule feeding `1.5` and `true` | red today (`field_not_finite_float('1.5')`), green once `dl6_oracle.pl:136-139` grows the two clauses `golden_oracle.pl` already has |
| DIV-4 | `wide_integer_is_a_named_answer_not_a_RangeError` | `rel counter(total: int)` fed `9007199254740993` | red today (driver `RangeError` kills the tick), green once the arrival door answers by name |
| DIV-5 | `integer_arrival_at_a_float_column` | `rel reading(celsius: float)` fed `4` | red today (oracle refuses, emitter stores); green in whichever direction `slot_int_widens_to_float` is ruled. The fixture pins the RULING, so it must be written after the call |
| DIV-7 | `malformed_json_arrival_is_a_refusal_not_a_halt` | `rel doc(payload: json)` fed `"north"` | red today (process exit 1 on one door, silent drop on the other), green once both doors answer with one named term |
| DIV-8 | `check_violating_arrival_is_not_silently_ignored` | `rel doc(payload: json)` fed `"north"`, graded on the FINAL state | red today (emitter final state empty, no error), green once the arrival statement stops swallowing constraint failures |

## Recommended refusal set

Split into what should REFUSE, what should be DEFINED with a written contract,
and what is a DEFECT with no design content.

### Refuse at load (extend an existing name, do not mint a new vocabulary)

1. **Widen `edge_head_column_type_mismatch` to every head and to arrivals**,
   under one name (`column_type_mismatch(Ref, Position, Source, Declared)`).
   Evidence for the shape rather than an argument for it: the edge head is
   the ONE position that already refuses, and it is the ONE position with
   essentially no divergence (2 of 70 cells vs 51 of 70 at the arrival). The
   `decl_type_conflicts_witness` precedent is the same rule already ruled once
   (decl is authority) applied to literals instead of to values.
2. **Extend `type_arrival_shape_mismatch` past `float`/`bool` to every declared
   type, on BOTH doors.** The engine already refuses a bad float and a bad bool
   at the world boundary and says nothing about a text at an `int` column.
   Receipt for why that hole matters: `rel probe_in(value: int)` fed `"north"`
   is byte-IDENTICAL on both doors and both store `"north"`, a declared `int`
   column holding arbitrary text with every gate green. The same column fed
   `"4"` DIVERGES (oracle `"4"`, emitter `4`), because SQLite's INTEGER
   affinity converts numeric-looking text and leaves the rest alone. So the
   type hole is not merely permissive, it is permissive in a way that changes
   behaviour on exactly the values that look like they should work.
3. **A wide integer is a named answer at the arrival door.** Which answer is a
   user call (`slot_wide_int_fate`); a driver `RangeError` is not a candidate.

### Define, with the contract written down

4. **`1.0` == `1` and `-0.0` == `0`** in storage and in the tick log, on every
   declared type. Both doors already agree; the log contract does not say so,
   and a `float` column silently cannot hold the distinction.
5. **The untyped json hole carries the json value's own type**, matching the
   oracle, or the emitter's `text` becomes the definition and the oracle is
   changed. Either way one door moves; today they differ on 8 of 10 value types.
6. **A json column's arrival cannot express a top-level json string** (SC-5).
   Worth one line in `SYNTAX.md` beside the existing "a json document arrives
   as its text" sentence.

### Defects, no design content

7. `runtime/1_incremental.ts:728-734` compares pre-affinity JS values against
   post-affinity `RETURNING` values (DIV-1). Even with refusal 2 in place this
   stays reachable through host responses and binds, which are not schedule
   arrivals.
8. `dl6_oracle.pl:136-139` needs the float and boolean clauses
   `golden_oracle.pl` already has (DIV-3), and its two `halt(1)` sites need to
   throw (DIV-7). Both are fenced away from this lab and from the concurrent
   golden lane; they are `dl6_oracle.pl`'s owner's to take.
9. The arrival `INSERT OR IGNORE` swallows CHECK failures (DIV-8).
10. `aggregate_operand_not_number` refuses `min`/`max` over text on the
    compiler while the reference engine computes it (35 compiler-only cells).
    Either widen the compiler (SQLite `MIN` orders text fine) or add the
    refusal to the oracle; today it is a one-sided wall.

## Per-slot answers

### `slot_decl_source_conflict_fate`: REFUSE AT LOAD

Measured, not preferred. The four positions that coerce a decl-vs-source
conflict (`arrival` 51, `join_column` 51, `level_head` 38, `aggregate_head` 15)
produce 155 of the matrix's 168 divergences. The one position that refuses
(`edge_head`) produces 2. There is no third option visible in the data: every
cell where a value crosses a type boundary silently is a cell where at least
two of the three runs disagree.

The refusal should be the EXISTING `edge_head_column_type_mismatch` widened,
not a new name, and it should fire at the arrival too, because the arrival is where
`type_arrival_shape_mismatch` already lives for two of the seven types.

Caveat this lab cannot settle: widening the refusal will break real programs
that today feed a JSON number into an undeclared column and get away with it
on the naive path. The size of that blast radius is a sweep/conformance
question, not a matrix question.

### `slot_float_int_boundary`: ALREADY COLLAPSED, and undocumented

`1` and `1.0` are the same value on both doors, at every declared type
including `float`: 17 cells fed `1.0` graded `1`, agreed by every run. `-0.0`
graded `0` on 17 more. Zero cells anywhere in the matrix keep the distinction.

So there is nothing to rule about equality; it is already decided by the
canonical-json log contract. What is owed is the WRITTEN contract (recommended
definition 4) and the acknowledgement that `float` is therefore a storage type
with no distinguishable `.0`, which makes `avg()` the only thing `float`
currently buys.

Second half, and it is the reason nobody noticed: through `dl6_oracle.pl` a
float cannot be fed at all (DIV-3), so the served/text door has never exercised
this boundary once.

### `slot_undeclared_column_default`: the default is `text`, and it IS divergent

Measured from the emitted DDL. `rel probe_in(value).` with no witness anywhere
emits:

```sql
CREATE TABLE "probe_in" ("value" TEXT NOT NULL, PRIMARY KEY ("value")) WITHOUT ROWID
```

Of the 60 undeclared cells: 20 IDENTICAL, 10 SILENT_COERCION, **24 DIVERGENT**,
6 NAMED_REFUSAL. The worst is the two-line program in DIV-1, where an ordinary
integer arriving at an undeclared column makes the default emitter path derive
nothing at all.

So: yes, the zero-witness default is `text`, and no, it is not safe as it
stands. It becomes safe under either fix 7 (the delta-drop defect) or refusal 2
(refuse the mismatched arrival). It does NOT need a different default; `text`
is the right conservative choice; it needs the two paths below it to agree
about what `text` does to an integer.

## Known seeds, re-confirmed at HEAD, not re-derived

| seed | status at `5ce31a24` |
|---|---|
| `join_column_type_mismatch` | HOLDS, compiler-only. `unsupported_construct(join_column_type_mismatch('b1."value"',text,'b0."value"',int))`; the oracle runs the same program and correctly derives zero joined rows (`4` ≠ `'4'` in prolog), so the refusal is conservative and right |
| `decl_type_conflicts_witness` | HOLDS, compiler-only. `unsupported_construct(decl_type_conflicts_witness(probe_in/1,1,int,text))` for a `'north'` literal at an `int`-declared column; oracle runs |
| `edge_head_column_type_mismatch` | HOLDS, 49 cells. Fires on 49 of the 60 edge-head cells where declared ≠ source type, and on NONE of the 10 where they match. The 11 mismatched cells it lets through are the ones SQLite affinity happens to round-trip |
| `@libsql` int→bigint REAL corruption | NOT reproduced as corruption; the surviving wide-int failure at HEAD is the read-back `RangeError` (DIV-4), which is a different and louder shape |
| wide-int cliff (json-flex C2) | HOLDS and is WIDER than json: 34 divergent cells, 0 identical, and the rounding inverts between the schedule seam and the json document seam |
| bool degradation (C4) | HOLDS. `true` reaches storage as `1`; at a `bool` head a float `1.0` becomes `true` on the emitter and `1` on the oracle, `-0.0` becomes `false` vs `0` |
| the `lane: text` over int source incident | HOLDS and is a special case of DIV-2; the incident's own cell also turns out to be DIV-1 on the default path, where the emitter derives nothing at all rather than logging `"4"` |

## Ambiguities, named and not guessed

- **`slot_int_widens_to_float`**: the engine refuses an integer at a declared
  `float` column, the emitter stores it. Nothing rules which is right. 36 cells
  hang on it. USER CALL.
- **`slot_wide_int_fate`**: refuse integers outside the JS safe range at the
  arrival door, or store them as TEXT, or change the driver. Each is a
  different contract for `int`. USER CALL.
- **`slot_bool_storage`**: `bool` is a live declared type whose values are `1`
  and `0` in storage, `true`/`false` in json, and `bool_lit(_)` in the oracle's
  terms. 32 of its 60 cells are DIVERGENT and 24 are refusals. Whether `bool`
  should be a column type at all is a live question. The golden plan's own
  recommendation was "bool = row presence / two-variant enum, never a column
  type", and this matrix is evidence for that recommendation rather than
  against it. NOT decided here.
- **Blast radius of widening the type refusals** (slot answer 1 and 2): needs a
  sweep + conformance run against the widened check, which this lab is fenced
  out of.
- **`list(text)` behaves as `json` in storage and as `text` in the oracle's
  rendering** in several cells, and `{field: captured: list(text)}` is a
  `dl_parse_error` (a raw code-list dump, 10 cells) rather than a named
  refusal. Whether `list(T)` should be a capture type at all is unstated.
- **The lab's own door choice**: 92 cells are graded through
  `golden_oracle.pl` rather than `dl6_oracle.pl`. If the golden lane changes
  `golden_oracle.pl`'s `schedule_value/2` while closing GAP 1, those 92
  verdicts must be re-run before they are trusted. `matrix.sh` re-runs in about
  four minutes.

## Process notes

- Base `git merge --ff-only 5ce31a24`, confirmed `git rev-parse --short HEAD` =
  `5ce31a24`.
- Writes stayed inside `v6/prolog/labs/type_matrix/**` and this file. The
  pre-commit hook additionally regenerated `v6/INDEX.md` (21 lines, indexing
  this lab's own new files), disclosed, not intended, droppable.
- The lab holds two relative symlinks (`runtime`, `node_modules`) into
  `v6/tsv2` so an emitted cell module resolves its own imports from the lab
  directory. That is what let the matrix reuse `golden-run.ts` unmodified
  instead of growing a third copy of the final-state encoder, and what kept
  1,700 generated files out of `v6/tsv2/gen_emitted/`.
- `out/` is gitignored (about 1,700 files); `MATRIX.md` and `matrix.json` at
  the lab root are the committed artifacts.

## Coordinator addendum: merged-main re-run (post golden-json-seam merge)

The golden seam lane landed while this lab ran and changed exactly the door
this verdict warned about (golden_oracle.pl schedule_value now rides the
shared 0_json_arrival.pl). Coordinator re-ran `matrix.sh` on merged main
(f91e1d5d-era tree): 422 cells = 79 IDENTICAL / 156 DIVERGENT / 71
SILENT_COERCION / 116 NAMED_REFUSAL (was 73/168/65/116 at base 5ce31a24) —
the shared arrival module healed 12 divergent cells, nothing worsened. The
headline defect REPRODUCES at merged main:
`cell_arrival__int__numeric_text  incremental "ABSENT" vs naive "4" vs
oracle "\"4\""` — the DEFAULT emitter path drops affinity-rewritten arrival
deltas (v6/tsv2/runtime/1_incremental.ts, post-affinity RETURNING text vs
wire value under JSON.stringify comparison). Fix lane dispatched.

Two lab symlinks (`runtime`, `node_modules` -> ../../../tsv2/...) did not
survive the merge and 266 cells read emitter_run_error until the coordinator
recreated them; they are now committed so `matrix.sh` regenerates anywhere.
Lab dir stays alive pending the fix wave (the matrix is the fix lanes'
verification tool); protocol debt accepted, same as csp/extract-t2.

---

## Addendum: regrade at base `ed639842` (2026-07-31)

Re-run of all 422 cells on `ed639842`, plus a classifier that had gone stale
against three landings. `matrix.sh` regenerates everything; the run itself is
35s wall.

Nothing outside `v6/prolog/labs/type_matrix/**` and this section was touched. No
compiler, runtime, emitter, fixture, rail or gate file was edited. `just
conformance` re-run from `v6/` on this tree after the regrade: **267 PASS / 0
FAIL, exit 0**.

### What moved, and which half is the engine

Two different things changed since the last committed matrix, and folding them
into one number would misattribute both:

* **landings** — the float/avg + `int_out_of_range` arc (`b6154706`), the
  `intMode` bigint revert (`0b7c2d37`). The incremental-affinity fix
  (`32916613`) was already in the last committed matrix.
* **the reading** — the classifier could not hear a run-time refusal, and could
  not parse a refusal name through the `at/3` location wrapper.

| verdict | committed matrix (`32916613`, pre float/avg) | this base, OLD classifier | this base, NEW classifier |
|---|---|---|---|
| IDENTICAL | 87 | 81 | **81** |
| SILENT_COERCION | 79 | 75 | **75** |
| DIVERGENT | 140 | 150 | **104** |
| NAMED_REFUSAL | 116 | 116 | **162** |

| verdict / label | committed | this base, OLD | this base, NEW |
|---|---|---|---|
| IDENTICAL / lossless | 87 | 81 | 81 |
| NAMED_REFUSAL / compiler_only | 86 | 81 | 81 |
| DIVERGENT / doors_disagree | 81 | 69 | 69 |
| SILENT_COERCION / value_changed | 58 | 54 | 54 |
| NAMED_REFUSAL / both | 30 | 30 | **48** |
| NAMED_REFUSAL / name_mismatch | 0 | 5 | **33** |
| SILENT_COERCION / row_absent | 21 | 21 | 21 |
| DIVERGENT / emitter_only_refusal | — | — | **14** (new label) |
| DIVERGENT / oracle_only_refusal | 40 | 58 | **12** |
| DIVERGENT / emitter_modes_disagree | 0 | 8 | 8 |
| DIVERGENT / emitter_run_error | 19 | 15 | **1** |

Only the last column is current truth. Columns 1 and 2 are the same stale
reading applied either side of the landings; column 2 to column 3 is the reading
being corrected against one banked `run-results.json`, so those 60 cells are the
classifier moving and not the engine.

**The stale reading was hiding the float/avg landing's whole benefit.** Read with
the old classifier the arc looks like a regression (DIVERGENT 140 -> 150,
IDENTICAL 87 -> 81). Read correctly it converted 46 divergences into agreed
refusals: the emitted module now carries an arrival type gate
(`emit_ts.pl:504-518`), so 46 cells that used to be "the reference engine refuses
and the emitter stores it anyway" are now "both sides refuse".

Cell movement, 60 cells, all one direction:

| movement | cells | landing that caused the behaviour |
|---|---|---|
| `oracle_only_refusal` -> `NAMED_REFUSAL/name_mismatch` | 28 | float/avg arrival gate (18 bool, 10 float) |
| `oracle_only_refusal` -> `NAMED_REFUSAL/both` | 18 | float/avg `int_out_of_range`, both sides same functor |
| `emitter_run_error` -> `emitter_only_refusal` | 14 | float/avg gate refusing where the engine accepts |

Three classifier defects fixed, each named in place in `classify.mjs`:

1. **Run-time refusals are refusals.** `RUNTIME_REFUSALS` is a closed set copied
   out of `emit_ts.pl` and `runtime/rows.ts`, not a pattern guess. An
   unrecognised throw still grades `emitter_run_error`, which is what keeps the
   driver `RangeError` visible as the crash it is.
2. **Name-match leg.** The name is the refusal FUNCTOR when the message carries
   one that also exists prolog-side (`int_out_of_range`), and `""` when the guard
   is prose. Two of the emitted guards answer in a vocabulary the oracle's
   `type_arrival_shape_mismatch` does not share, so those 28 pairs grade
   `name_mismatch` rather than quietly asserting the sides agree.
3. **`at/3` unwrap.** `unsupported_construct(at(File,Line,Reason))` was reading as
   the refusal named `at` in 50 cells. Verdicts unaffected, 50 receipts repaired.

Sabotage receipt: swapping the `int_out_of_range` probe for a never-matching
pattern puts exactly those cells back (`both` 48 -> 30, `emitter_only_refusal`
14 -> 0, `oracle_only_refusal` 12 -> 30, `emitter_run_error` 1 -> 15).

### The RangeError count, honestly

**ONE cell**, not 19: `cell_json_capture__int__wide_int`, both emitter modes,
`RangeError: Received integer which cannot be safely represented as a JavaScript
number`. It is the only unnamed emitter failure left in the matrix.

The `intMode` revert did **not** push read-side overflow cells back to
`RangeError` at the scale the priced ARCH row anticipated. Traced cell by cell
against the committed matrix: all 19 of its `emitter_run_error` cells were
`RangeError`, and **18 of those exact 19 are now `NAMED_REFUSAL/both`** on
`int_out_of_range` while 1 survives. The count did not collapse because of the
revert; it collapsed because the arrival gate now refuses the unsafe integer by
name before it ever reaches the driver.

The one survivor reaches SQLite through a **json capture**, which has no arrival
gate to pass, so the unsafe integer is only discovered on the way back out. That
single cell is the whole remaining surface of the `bigint_seam_normalize` class
and it is exactly the shape that row predicts (SQL-computed overflow read back
raw). The row's own "matrix residual" note is therefore accurate and its size is
one cell.

### The two `.dl6` oracle doors: DIV-3 and DIV-6 are structurally closed

Both doors now delegate to the shared `compile/scripts/0_json_arrival.pl`, whose
only per-door argument is a Context atom used in `halt/1` text. Measured this
run: **0 cells where the doors disagree, 0 cells where one door carried an
arrival the other refused.** DIV-3 (92 cells) and DIV-6 (120 cells) as written no
longer reproduce. The lab's golden leg had been answering
`existence_error(read_schedule/2)` on every cell since the door repair; it is
called at the shared arity again so this stays a measurement.

### What the current numbers say about each open ruling

One sentence each, evidence only.

**Refusal-widening blast radius.** The widening has already been run as a live
experiment on three of the seven declared types — the float/avg arrival gate
covers `int`/`float`/`bool` — and the measured result at the two world-boundary
positions is that `bool` went to **0 divergent cells of 20**, `float` to 2 and
`int` to 6, while the four ungated types (`text`/`json`/`list(text)`/`undeclared`)
sit at 6/3/3/6 each, and no cell anywhere got worse; the edge head, still the
only position with a full type wall, holds **0 divergent of 70**.

**`int_widens_to_float`.** The language currently answers this question three
different ways depending on where the integer crosses: at an arrival or join
column both oracle doors refuse `type_arrival_shape_mismatch` while the emitter
stores `4` (2 divergent cells), at a level head or aggregate head every door
accepts and grades `4` **IDENTICAL**, and at an edge head the compiler refuses
`edge_head_column_type_mismatch` — so the widening is already silently permitted
inside a program and refused only at its boundary.

**`wide_int_fate`.** Of 42 wide-integer cells there are still **zero IDENTICAL
anywhere**, but the shape has changed: 28 are now agreed named refusals at
`int`-declared columns, and the 14 survivors are all at columns the gate does not
cover, where the value is not refused but silently altered — 8 of them are
`emitter_modes_disagree` giving **three different answers to the same input**
(`"9007199254740992"` incremental vs `"9.00719925474099e+15"` naive vs
`9007199254740993` oracle at a `text` column), which is a stronger receipt than
the original DIV-4 because it shows the two emitter modes disagreeing with each
other rather than merely with the engine.

**`bool_storage`.** A `bool` column is well-behaved on exactly one input: of its
60 cells only **4 are IDENTICAL and all four are fed an actual boolean**, 44 are
refusals and the remaining 12 are divergences concentrated at the two head
positions that no arrival gate protects, where the oracle keeps the source value
and the emitter's `CHECK ("value" IN (0,1))` either drops the row entirely
(`4`, `1.5`, `"north"`, `{"key":1}` -> ABSENT) or coerces it (`1.0` -> `true`,
`-0.0` -> `false`) — evidence for, not against, the golden plan's
"bool = row presence / two-variant enum, never a column type".

### Process notes

* Base `git merge --ff-only ed639842` from `01ac896e` (382 behind, 0 ahead),
  confirmed `git rev-parse HEAD` = `ed639842`.
* `pnpm install` was needed in **both** `v6/tsv2` and `v6/sprefa-store/js`; the
  store package is a workspace source dependency and its own `rxjs` must resolve
  or every emitted cell dies at import. No outer `node_modules` was symlinked.
* The lab's two symlinks were absent again (they are gitignored, so the
  "committed" note in the previous addendum does not hold). `matrix.sh` now
  recreates them and hard-fails on an empty `node_modules` rather than reporting
  422 silent errors at exit 0.
