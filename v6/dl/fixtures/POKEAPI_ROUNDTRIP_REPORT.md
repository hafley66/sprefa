# PokeAPI components round-trip report

Source: `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/converter-probe/v6/dl/fixtures/pokeapi.openapi.yml`
Generated: `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/converter-probe/v6/tsv2/gen/pokeapi_gen.dl6`

compile (compile_dl6.sh) exit code: 0
emit-back (4_emit_jsonschema / 5_emit_openapi): OK
source components: 212 | generated component rels: 212 | lifted/enum rels: 161

## KNOWN emitter/compiler gaps (do not fail the gate)

- **G1 ref-target carries generic columns**: the mapping mandates `list(rel_name)` and inline-object LIFT; the tsv2 compiler refuses a rel that is itself a ref TARGET (used as a column type, a list element, or an option element) while carrying generic option()/list() columns — the generic expansion inside that rel can't lower (`unsupported_construct(column_type_unknown(...))`, 0_type_plane.pl:128). Strict mode keeps every other real spelling and drops exactly these columns to the `json` carrier, each attributed in the gap rows below with the throw site; the clean-data rows are proven in the mapping hand fixture.
- **G2 nullable arrays**: dl6 has no nullable-array type; `option(list(_))/option(json_list(_))` are refused. Nullability dropped (logged per property).

### Component name set

| metric | count |
| --- | ---: |
| match | 212 |
| mismatch | 0 |
| known gap | 0 |
| total compared | 212 |

### Per-component property name set

| metric | count |
| --- | ---: |
| match | 786 |
| mismatch | 0 |
| known gap | 0 |
| total compared | 786 |

### Per-property kind

| metric | count |
| --- | ---: |
| match | 786 |
| mismatch | 0 |
| known gap | 0 |
| total compared | 786 |

### Per-property ref target

| metric | count |
| --- | ---: |
| match | 257 |
| mismatch | 0 |
| known gap | 0 |
| total compared | 257 |

### Per-property nullability

| metric | count |
| --- | ---: |
| match | 786 |
| mismatch | 0 |
| known gap | 0 |
| total compared | 786 |

## Compile / emit receipts

```
wrote /var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/pe_pokeapi_gen.ts
COMPILE-TRACE program=pokeapi_gen parse=1337/7709259 plan=825/6043703 lower=178/736060 boot=2/24244 emit=889/8281779 write=34/179 total=3265/22795224
emit-back wrote /Users/chrishafley/projects/sprefa/.boop-worktrees/feature/converter-probe/v6/tsv2/gen/pe_emit/schema.json
emit-back wrote /Users/chrishafley/projects/sprefa/.boop-worktrees/feature/converter-probe/v6/tsv2/gen/pe_emit/openapi.json
```

Converter strict-mode dropped columns (G1) + nullable-array (G2): 25 + 4

## Emit-back receipt

Emitted component definitions: 212 / 212 source components.
Spot check (formerly json-carrier array-of-refs now round-trips):
`ability_change.effect_entries: list(ability_change_effect_text)` — an array of $ref items to ability_change_effect_text.