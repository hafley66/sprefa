# PokeAPI structural shape report

Input: `/Users/chrishafley/projects/sprefa-lanes/pokeapi.openapi.yml`.

The fixture is [pokeapi_shape.dl6](/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/pokeapi-shape/v6/dl/fixtures/pokeapi_shape.dl6). It contains one `rel` declaration per PokeAPI component schema. The text-door spelling for arrays is `json_list(T)`, which lowers to `list(T)` in the compiler type plane.

The fixture emits all 212 source component names. The generated JSON Schema contains 224 definitions because nullable columns expand into compiler-owned option helper relations. The generated OpenAPI contains the same component definitions and the emitter's fixed served-engine routes.

| PokeAPI construct | count in spec | expressible today |
| --- | ---: | --- |
| component schemas | 212 | yes: one `rel name(...)` per schema |
| component object properties | 786 | yes: relation columns |
| required properties | 627 | yes: bare column types |
| nullable properties | 129 | yes: `option(T)`; emitted as option helper relations |
| direct component `$ref` occurrences | 255 | partial: direct non-array refs use named relation types; refs inside arrays or compound shapes are `json` |
| scalar arrays | 1 | yes: `json_list(int)` |
| arrays of component refs | 139 | no: `list(T)` relation-ref storage is not built yet; fixture uses `json` |
| other array/object shapes | 85 | no: inline and compound element shapes are represented as `json` |
| enums | 0 | no enum surface in this source file |
| `oneOf` schemas | 1 | no: represented as `json` |
| recursive component refs | 0 | no recursive case present to test |
| pagination envelope schemas | 49 | partial: component rows are modeled; generic list response typing is absent from the emitter route surface |
| paths | 100 | no: OpenAPI emitter has its fixed six served-engine paths |
| GET operations | 100 | no: PokeAPI operations are not generated from the fixture |
| path parameters | 50 | no: PokeAPI per-path parameters are not generated |
| response schemas | 100 | no: response bodies are not attached to generated operations |
| descriptions, summaries, examples, formats, constraints, titles, defaults | 433 / 100 / 403 / 295 / 110 / 1 / 2 | skipped: metadata by design |

## Generated artifacts

- [pokeapi_shape.schema.json](/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/pokeapi-shape/v6/prolog/compile/out/pokeapi_shape.schema.json)
- [pokeapi_shape.openapi.json](/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/pokeapi-shape/v6/prolog/compile/out/pokeapi_shape.openapi.json)
- [pokeapi_shape.ts](/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/pokeapi-shape/v6/prolog/compile/out/pokeapi_shape.ts)

## Structural comparison notes

The component-name comparison maps PascalCase OpenAPI names to the fixture's snake_case relation names. All 212 names are present in the JSON Schema output.

The JSON Schema emitter currently renders `option(T)` through generated `__opt_*` relations. Its output therefore has zero nullable-property `anyOf` entries even though the fixture carries 129 `option(T)` columns. This is an emitter-shape finding at `v6/prolog/compile/4_emit_jsonschema.pl:column_schema/3`, where `option` is expected to render an `anyOf` schema but the catalog contains the expanded helper relations first.

The OpenAPI emitter's route and response objects are fixed in `v6/prolog/compile/5_emit_openapi.pl:api_route/5` and `operation_responses/2`. The PokeAPI path, parameter, and response rows are consequently outside the current fixture-to-emitter structural surface.

## Counting receipt

Counts above were obtained from the vendored file with this command:

```sh
node <<'NODE'
const fs = require('fs');
const YAML = require('/Users/chrishafley/projects/instant/node_modules/.pnpm/yaml@1.10.3/node_modules/yaml');
const doc = YAML.parse(fs.readFileSync('/Users/chrishafley/projects/sprefa-lanes/pokeapi.openapi.yml', 'utf8'));
const schemas = doc.components.schemas;
const paths = doc.paths;
let counts = {schemas: Object.keys(schemas).length, paths: Object.keys(paths).length, gets: 0, pathParams: 0, responses: 0, responseSchemas: 0, properties: 0, required: 0, nullable: 0, refs: 0, arrays: 0, arrayRefs: 0, scalarArrays: 0, oneOf: 0, description: 0, summary: 0, examples: 0, format: 0, minimum: 0, maximum: 0, title: 0, defaults: 0};
function walk(x) {
  if (!x || typeof x !== 'object') return;
  for (const k of ['description', 'summary', 'example', 'examples', 'format', 'minimum', 'maximum', 'title', 'default']) if (x[k] !== undefined) counts[k === 'example' || k === 'examples' ? 'examples' : k === 'default' ? 'defaults' : k]++;
  if (x.nullable === true) counts.nullable++;
  if (x.$ref) counts.refs++;
  if (x.oneOf) counts.oneOf++;
  if (x.type === 'array') { counts.arrays++; if (x.items && x.items.$ref) counts.arrayRefs++; if (x.items && ['string', 'integer', 'number', 'boolean'].includes(x.items.type)) counts.scalarArrays++; }
  for (const v of Object.values(x)) walk(v);
}
for (const s of Object.values(schemas)) { counts.properties += Object.keys(s.properties || {}).length; counts.required += (s.required || []).length; }
for (const p of Object.values(paths)) if (p.get) { counts.gets++; for (const q of p.get.parameters || []) if (q.in === 'path') counts.pathParams++; for (const r of Object.values(p.get.responses || {})) { counts.responses++; if (r.content?.['application/json']?.schema) counts.responseSchemas++; } }
walk(doc);
console.log(counts);
NODE
```
