# Ordered aggregate wiring arc — brief (codex luna)

Executes the lab verdict `plans/2026-07-30-ordered-aggregate-verdict.md`
(lab last copy at commit `c45e3a46` — `git show c45e3a46:v6/prolog/labs/
ordered_aggregate/README.md` recovers the runnable receipts). Read the
verdict AND the lab header `plans/2026-07-30-ordered-aggregate-lab-header.md`
first. This arc deletes four standing sightings: self-map mermaid assembly,
v5-collect expressibility, the json aggregate head refusals, extract-t2
round-trip rebuild.

## Surface (coordinator-fixed, do not relitigate)

Four aggregate head forms, all ordinary aggregate heads beside
count/sum/min/max:

| dl6 head form | SQL lowering |
| --- | --- |
| `json_group_array(Value)` | `json_group_array(v ORDER BY v)` (value axis, v5 parity) |
| `json_group_array(Value, Ordinal)` | `json_group_array(v ORDER BY ordinal)` |
| `group_concat(Value, Sep)` | `group_concat(v, sep ORDER BY v)` |
| `group_concat(Value, Sep, Ordinal)` | `group_concat(v, sep ORDER BY ordinal)` |

`Sep` must be a constant text literal (refuse a variable separator by name).
`Ordinal` and `Value` are body-bound variables. The multi-arg aggregate
precedent is v5's `json_group_object(key, value)` two-arg arm (src/lower.rs,
`agg_args2`) — read it, then implement in the v6 prolog compiler's own style.
The existing `group_concat` refusal (`refuse(not_implemented)`, landed by the
oracle-gate lane) is REPLACED by the real implementation; keep the refusal
for any argument shape outside the table above, named.

## Semantics contract (grading is byte-exact, treat these as law)

- Head column type: `json_group_array` head column is `json`; `group_concat`
  head column is `text`.
- Tick log: the array cell renders as canonical JSON text (ruling
  `json_ticklog_encoding = canonical_json_text`: sorted keys, no whitespace,
  arrays keep element order). The oracle's agg_compute must produce output
  BYTE-IDENTICAL to SQLite's `json_group_array` text for the same rows —
  verify number rendering (SQLite emits `[1,2]` not `[1.0,2.0]` for
  integers) and string escaping on at least one non-ascii + quote-bearing
  fixture row.
- Empty group = ABSENT head row (lab slot_empty_group). When the last row of
  a group is retracted, the head row's deletion must appear as a minus delta
  in the tick log.
- Incremental emitter: group-scoped recompute — scope seed from the delta
  stream (`INSERT OR IGNORE INTO __agg_scope SELECT DISTINCT group FROM
  delta WHERE _sign IN (-1,1)`), scoped DELETE of old head rows, scoped
  grouped INSERT with the inner ORDER BY. Beside the existing min/max
  aggregate plan, same file, same style. The naive referee recomputes
  whole-table; both modes must grade identical.
- Retraction ticks (P3 support machinery): a minus delta in a group EXECUTES
  the group recompute. The lab graded this as prose only; this arc owes the
  EXECUTED fixture (see fixtures below).

## Full wiring checklist

1. registry.pl surface rows for the new forms (generated SYNTAX.md table
   picks them up; regen the vscode tmLanguage if the registry emitter feeds
   it — check `emit_dl6_grammar/0`).
2. Oracle: agg_compute arms for both aggregates, both axes; the canonical
   JSON encoder is shared with ticklog.pl — one implementation, never two.
3. parse_dl.pl / print_dl.pl: aggregate heads already parse as calls —
   verify roundtrip (G1) covers the 2- and 3-arg forms, add if not.
4. Compiler lowering: emitted SQL per the table; scoped statements per the
   contract; `sprf_sym_intern`-style interning does NOT exist in tsv2 —
   ignore that v5 detail.
5. Fixtures (conformance + sweep, both modes, canonical tick logs):
   value order, ordinal order, string join both axes, empty-group absence,
   EXECUTED minus-delta (retract one row of three → rebuilt array, retract
   last row → head minus delta, re-add → fresh head), nested
   `json_group_array(Payload)` over a json column, plus the four sighting
   programs from the verdict census as named fixtures (mermaid-line
   assembly, fragment assembly, v5 group_rels x2).
6. COUNT tests per the formerly-quadratic law: statement count flat across
   group counts, EXPLAIN QUERY PLAN SEARCH-not-SCAN on the scoped insert,
   sabotage receipt in the test header.
7. plunit for refusal paths: variable separator, wrong arity, ordinal at a
   non-int column.

## Receipts required before you report done

- conformance (expect 245 + your new fixtures, 0 fail), sweep BOTH modes
  (0 wrong; state the new compiled/identical counts), TEXT_DOOR 0 failures,
  roundtrip ALL PASS, plunit all pass, `just green` exit 0, compile-speed
  gate (report any regression, never bypass; a justified accepted cost is
  the coordinator's call, not yours), staleness gate OK.
- The byte-equality receipt: one fixture's array cell diffed byte-for-byte
  oracle vs emitted-module tick log, shown in your report.

## Process

- No-commit flow: coordinator-cut worktree, git metadata writes will fail in
  your sandbox. FIRST ACTION: `git rev-parse --short HEAD` must print the
  base sha stated in the launch prompt; if not, STOP AND REPORT. Never work
  around a blocked command through another mechanism.
- Style laws: descriptive dl6 variable names; no em dashes; banned words
  provenance/substrate/load-bearing/regime; every dl6 snippet in any doc you
  write carries its pure-rxjs lowering.
- Do not touch: v6/tsv2/serve/, v6/dl/ (except reading), labs/, plans/ other
  than nothing — your report is the doc; the coordinator writes the ledger.
