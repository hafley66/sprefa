# Phase F: typegen as a dl6 program

Decided with Chris 2026-08-14 ("both is fine but dl6 first"): the type plane
exports as dl6 arrival rows, and TS interface text renders through a checked-in
dl6 program running on the real tsv2 runtime. The prolog emitter
(`7_emit_ts_types.pl`) stays; this arc builds the dl6 door beside it and
measures parity. File writing is OUT OF SCOPE (fs-effects door is a separate
queued arc); rendered text rests in a derived rel and is asserted by a golden.

## TOC
1. Receipts
2. Contract
3. Slices
4. Validation
5. Ownership and laws

## 1. Receipts

| fact | where |
|---|---|
| the render shape is PROVEN: a 3-rule dl6 program rendered TS interfaces through tsv2 | chat_log/20260813.2 ("The type-render PROBE WORKS"); probe artifacts were bulldozed, the rules are restated below |
| rule shapes | field lines via `concat`, body via `group_concat(line, sep, ordinal)` in aggregate HEAD (`:=` position throws by #246), wrap via `concat` |
| casing | `replace(initcap(name), '_', '')` = PascalCase; snake_case stays canonical |
| semantic type rows exist and are complete | unity A-D landed: declaration/application/argument/option/list-flavor origin rows + bound judgments, `v6/prolog/0_generic_expand.pl` (`merge_option_type_rows`, `merge_flavor_type_rows`, `judge_template_bounds/4`) |
| existing prolog TS emitter to measure against | `v6/prolog/compile/7_emit_ts_types.pl` (its `.types.ts` outputs sit beside each fixture in `compile/out/`) |
| string primitives gating this arc are LANDED | upper/lower/trim/initcap (#245), substr/instr/length (#244), split (8a1698c7), concat (pre-existing) |
| type-name collisions resolve by MODULE PREFIX | `7_emit_ts_types.pl:61-64` (`type_name/2`), user decision |

## 2. Contract

Export (new prolog file, no edits to existing compiler files):
```prolog
% v6/prolog/compile/typegen_export.pl
% dump_type_rows(+CompiledProgram, +JsonlPath)
% one JSONL line per semantic type row, arrival-shaped for the renderer's
% EDB rels. Calls 0_generic_expand's row predicates MODULE-QUALIFIED
% (0_generic_expand:...); adds no export lines to any existing module.
```

Renderer (new dl6 program):
```
v6/dl/typegen/render_ts.dl6
  EDB:  type_row(...)            -- mirrors the JSONL arrival shape
  IR:   field_line(TypeName, Ordinal, LineText)   <- concat per column
        body_text(TypeName, group_concat(LineText, '\n', Ordinal)) <- aggregate head
        rendered_type(TypeName, FileText)         <- concat wrap
```

Driver + golden (new script, standalone; does NOT touch plunit_tests.pl or the
justfile):
```
v6/prolog/compile/test/typegen_golden.sh
  1. compile a pinned fixture set through the normal door
  2. dump_type_rows -> JSONL
  3. run render_ts.dl6 on tsv2 with the JSONL as arrivals
  4. diff rendered_type text against golden files committed under
     v6/prolog/compile/test/typegen_golden/
  exit nonzero on any diff; print the diff
```

## 3. Slices (one commit each)

1. **`typegen_export.pl` + JSONL for one fixture.** Pick a fixture with enum +
   option + list columns already compiling (e.g. the unity nested-generics
   fixture). Commit the dump predicate and one checked-in sample JSONL.
2. **`render_ts.dl6` + driver + goldens for the pinned set.** Start from the
   3-rule shape above; extend only as far as the pinned fixtures need
   (interfaces + option columns + list columns). Golden = the dl6-rendered
   text, committed.
3. **Parity report, not parity enforcement.** For the pinned set, diff
   dl6-rendered text against the existing `compile/out/<fixture>.types.ts`.
   Byte-parity is NOT required this arc; the deliverable is
   `plans/2026-08-14-phase-f-typegen-dl6.REPORT.md` with a table: fixture,
   identical yes/no, first differing line, cause (casing / ordering / feature
   gap). Every gap names the construct, never "unsupported".

## 4. Validation
```
bash v6/prolog/compile/test/typegen_golden.sh               # the new gate, green
cd v6/prolog/conformance && swipl -g go -t halt go.pl        # unchanged: 421 PASS / 0 FAIL
cd v6/tsv2 && bash scripts/sweep.sh                          # unchanged: RUN wrong=0
```
This arc adds NO fixture and touches NO existing compiler file, so conformance,
plunit, and grade must come back byte-identical to base; run conformance +
sweep once at the end to prove it. Baselines on 91da6781: conformance 421/0,
sweep RUN total=317 identical=314 wrong=0 rejection=3.

## 5. Ownership and laws
- First action: `git merge --ff-only 91da67816f00255d18d12094ea7cb11a9a896c70`;
  failure = STOP AND REPORT.
- Files owned (ALL NEW): `v6/prolog/compile/typegen_export.pl`,
  `v6/dl/typegen/render_ts.dl6`, `v6/prolog/compile/test/typegen_golden.sh`,
  `v6/prolog/compile/test/typegen_golden/**`,
  `plans/2026-08-14-phase-f-typegen-dl6.REPORT.md`.
- FORBIDDEN: every existing file. Especially `v6/prolog/0_generic_expand.pl`,
  `v6/prolog/lower.pl`, `v6/prolog/compile/registry.pl`,
  `v6/prolog/compile/test/plunit_tests.pl`, `justfile` (other lanes own the
  first four; the coordinator wires the justfile at merge). A needed export
  that does not exist = call it module-qualified; if that fails, REPORT, do
  not edit the module.
- If the tsv2 runtime rejects a renderer spelling, the report carries the
  throw name and site; do not patch the runtime.
- Style: dl variable names descriptive; comments only for constraints code
  cannot show; banned words incl. identifiers: provenance, substrate,
  load-bearing, regime, refusal.
