# SEED: polyglot typegen vs TypeSpec — gap census and arc sequence

Planner contract, seeded by coordinator 2026-08-16. The plan lane reads this
first and fills it; no lab starts blank.

## The question

sprefa dl6 declarations already drive storage DDL, a reactive runtime on two
doors, and type emission. TypeSpec (typespec.io, Microsoft) is the strongest
spec-first comparison point: models compile to OpenAPI3, JSON Schema,
Protobuf, and generated clients/servers in ~5 languages. How far is dl6 from
TypeSpec-class typegen, what does dl6 have that TypeSpec cannot, and what is
the cheapest arc sequence that closes the gaps worth closing?

## Inventory (verify every row, cite path:line, correct anything stale)

| asset | where |
|---|---|
| prolog emitters (parity judges, NOT the product per user decision 2026-08-14) | `v6/prolog/compile/{4_emit_jsonschema,5_emit_openapi,7_emit_ts_types,8_emit_rust_types,9_emit_type_artifact,3_emit_trace_schema}.pl` |
| dl6-first render doors (the product) | `v6/dl/typegen/render_ts.dl6`, `render_rust.dl6`; IR = `type_row/7` JSONL from `v6/prolog/compile/typegen_export.pl` |
| render_ts declared scope | its header: interfaces + option + list columns, single module, no collisions; module-prefix and generic-rel emission named as future arcs |
| type plane wrapper inventory | `v6/prolog/compile/0_type_plane.pl:145-151`; `docs/generics-wrapper-inspection.md` (+ .visual.human.unga.md) |
| compile coverage | `v6/prolog/compile/out/manifest.json` — 342/452 compiled, 110 unsupported at seed time; RE-RUN, never quote |
| ~780 untracked `out/*.types.{ts,rs}` | awaiting user word; the plan may propose their fate |

## Decisions that BIND this plan (rulings.pl / CLAUDE.md; do not relitigate)

- dl6 carries codegen alone; prolog emitters stay as parity judges
  (epic `issues/dl6-first-typegen`).
- Cross-module type-name collisions resolve by MODULE PREFIX. Same-module
  collision (`type-name-non-injective`) is an OPEN fork for Chris.
- Generics: written inspection exists (`docs/generics-wrapper-inspection.md`);
  implementation still needs Chris. Plan may sequence it, not start it.
- No coercions (`lower.pl:1826`, `lower.pl:335`).
- An enum rel IS a reusable named type (probed, compiles); `option(<enum>)`
  stopped at `0_option_expand.pl:43`. Absence-vs-null: VALUE plane spells it,
  COLUMN plane cannot (`4_emit_jsonschema.pl:121-146`).
- Lang design lands with Chris in the room: the plan presents forks with
  throw sites, it does not settle them.

## Comparison axes the plan MUST fill (a table per axis, sprefa vs TypeSpec)

1. Type expressivity: primitives, option/list, enums, named reusable types,
   unions, templates/generics, absence-vs-null, recursive types.
2. Emitter targets: TS, Rust, JSON Schema, OpenAPI vs TypeSpec's OpenAPI3,
   JSON Schema, Protobuf, client/server codegen (C#, Java, JS, Python, Go).
3. Constraint/validation vocabulary (TypeSpec decorators @minLength etc. —
   sprefa's answer today, if any, with citation).
4. Versioning (TypeSpec @added/@removed — sprefa: nothing? cite absence).
5. Extensibility: TypeSpec emitter framework vs a new render_*.dl6 door.
6. What TypeSpec CANNOT do: the running engine. A dl6 rel is storage +
   reactive runtime + wire type at once. Weigh this honestly as scope
   difference, not free superiority.
7. Build-vs-buy per repo law: candidate-by-candidate — could a TypeSpec (or
   protobuf/smithy) emitter CONSUME the type_row IR instead of us writing
   more renderers? No one-line dismissals.

## Deliverables (lab protocol)

- `plans/2026-08-16-typespec-parity-typegen.PLAN.md` — receipts, citations,
  the axis tables, sequenced arcs each sized small/med/large with the exact
  gate that proves it landed.
- `plans/2026-08-16-typespec-parity-typegen.PLAN.visual.human.unga.md` —
  plain words, diagrams (mermaid), zero citations, for Chris. A plan without
  this doc is undelivered.
- A "forks for Chris" section: every open design call, one line each, with
  the throw site or absent-code citation that proves it is real.

## Commands that print the truth

```bash
python3 -c "import json;from collections import Counter;m=json.load(open('v6/prolog/compile/out/manifest.json'));print(Counter(f['bucket'] for f in m))"
ls v6/dl/typegen/
cd v6 && just plunit
grep -rn "type_row" v6/prolog/compile/typegen_export.pl | head
```
