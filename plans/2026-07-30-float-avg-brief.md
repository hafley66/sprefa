# float + avg + bool — brief (codex luna)

Beta gate item 2 (plans/2026-07-30-v6-beta-plan.md). All value semantics are
ALREADY RULED (rulings + ARCH clock_check row, 2026-07-30): do not re-open
them, implement exactly:

- **float**: finite SQLite REAL / IEEE binary64. Exact comparison and join
  (no epsilon anywhere). Output/tick-log rendering = shortest round-trip
  (SQLite REAL text is not shortest round-trip — canonicalize at the seam
  like json_ticklog: oracle and TS runtime must render identically;
  JS Number.prototype.toString IS shortest round-trip, swipl float printing
  must be made to match it, state how). NaN/Infinity = named refusal at the
  boundary, never stored.
- **avg()**: aggregate over int/float groups, result float. Incremental
  form: sum + count accumulators, avg derived at read (the count-IVM
  precedent in the aggregate emitter). Empty group = absent row (existing
  aggregate law).
- **bool**: NOT a column type. Ruled: row presence / two-variant enum. The
  only storage-shaped ruling is for hosts/world columns that must carry
  one: INTEGER NOT NULL CHECK(value IN (0,1)). If nothing currently needs
  that column shape, state so and skip it — do not invent surface.
- **wide int**: the 19 matrix cells that RangeError today (int outside
  Number.MAX_SAFE_INTEGER through the bigint bind path). Becomes a named
  refusal at the value boundary (int_out_of_range) in BOTH oracle and
  compiler. Receipt: matrix cells move RangeError -> NAMED_REFUSAL.

## Where

- registry.pl: float column type row + avg aggregate row (mirror sum).
- Type inference/checking: the colon-typed decl path (decl type authority),
  arith over mixed int/float per SQL affinity — int op float = float,
  int/int division stays int unless ruled otherwise: SQLite `/` on
  integers truncates; keep sqlite semantics (vocabulary_tiebreak ruling =
  sqlite first).
- lower.pl/emit_ts.pl: REAL storage, expression typing, avg accumulators.
- engine.pl oracle: same semantics, byte-identical tick logs.
- parse_dl/print_dl/SYNTAX generated table/grammar regen: float literals
  (decimal point form only; state what you refuse — hex/exponent — as
  named refusals or supported, your call from sqlite's literal grammar).
- Fixtures: float decl/join/arith, avg (incl empty group + retraction
  tick), int/float mixed arith, wide-int refusal, float round-trip
  rendering. Fail-first where a bug is claimed fixed.

## Receipts required

- Conformance new count stated; sweep BOTH modes 0 wrong; TEXT_DOOR;
  roundtrip; plunit; type-matrix rerun: RangeError cells -> NAMED_REFUSAL,
  state full before/after bucket counts.
- EXPLAIN receipts for avg delta path (SEARCH not SCAN, the min/max
  precedent).
- Oracle-vs-emitted byte identity on every new fixture, both doors.

## Fences

- Touch: registry.pl, lower.pl, emit_ts.pl, engine.pl, parse_dl/print_dl,
  analyze typing, fixtures, SYNTAX/grammar regen, matrix harness.
- Do NOT touch: 0_messages.pl / compile.pl error printing / bop error
  paths (concurrent refusal-messages lane owns those), self-map/devlog
  files, labs/**.
- No-commit flow. STOP AND REPORT on blocked commands. Report EPERM legs.
  Set LC_ALL=en_US.UTF-8 before unicode-sensitive runs (known sandbox
  locale fake-failures).
