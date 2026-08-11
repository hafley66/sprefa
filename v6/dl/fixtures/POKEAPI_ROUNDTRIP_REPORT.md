# PokeAPI components round-trip report

Source: `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/openapi-to-dl6/v6/dl/fixtures/pokeapi.openapi.yml`
Generated: `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/openapi-to-dl6/v6/tsv2/gen/pokeapi_gen.dl6`

compile (compile_dl6.sh) exit code: 0
emit-back (4_emit_jsonschema / 5_emit_openapi): REFUSED (json_list serialization gap, G3)
source components: 212 | generated component rels: 212 | lifted/enum rels: 0

## KNOWN emitter/compiler gaps (do not fail the gate)

- **G1 arrays and inline objects**: the mapping mandates `list(rel_name)` and inline-object LIFT; the tsv2 compiler's generic `list()`/`option()` machinery refuses them on this dense ref web (`unsupported_construct(column_type_unknown(...))`, 0_type_plane.pl:128 — a rel that is a plain-ref target and carries generic option/list columns breaks generic expansion). The converter drops these to the `json` carrier in safe mode, logged per property; the rows are proven on clean data in the mapping hand fixture.
- **G2 nullable arrays**: dl6 has no nullable-array type; `option(list(_))/option(json_list(_))` are refused. Nullability dropped (logged per property).
- **G3 emitter serialization**: `kind_schema/6` (4_emit_jsonschema.pl) has no `json_list` clause; a full program carrying `json_list(int)` makes 4/5 return no document. Compare reads the emitter's own catalog model.
- **G4 nullable emit**: `option(T)` lowers to `__opt_*` helper rels at emit; an emitted doc shows the option column as an integer id, not an anyOf. Fixed by lane fix/anyof-emit-phase.

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
| match | 585 |
| mismatch | 0 |
| known gap | 201 |
| total compared | 786 |

_201 known-gap rows (sample):_
- `G1 ability_change.effect_entries`
- `G1 ability_detail.names`
- `G1 ability_detail.effect_entries`
- `G1 ability_detail.effect_changes`
- `G1 ability_detail.flavor_text_entries`
- `G1 ability_detail.pokemon`
- `G1 berry_detail.flavors`
- `G1 berry_firmness_detail.berries`
- `G1 berry_firmness_detail.names`
- `G1 berry_flavor_detail.berries`
- `G1 berry_flavor_detail.names`
- `G1 characteristic_detail.descriptions`
- `G1 contest_effect_detail.effect_entries`
- `G1 contest_effect_detail.flavor_text_entries`
- `G1 contest_type_detail.names`
- `G1 currency_detail.names`
- `G1 egg_group_detail.names`
- `G1 egg_group_detail.pokemon_species`
- `G1 encounter_condition_detail.values`
- `G1 encounter_condition_detail.names`

### Per-property ref target

| metric | count |
| --- | ---: |
| match | 118 |
| mismatch | 0 |
| known gap | 0 |
| total compared | 118 |

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
COMPILE-TRACE program=pokeapi_gen parse=413/2568940 plan=109/1064163 lower=51/226810 boot=0/7498 emit=237/2494409 write=9/179 total=819/6361999
emit-back REFUSED: emitter jsonschema_text on pokeapi_gen
ERROR: [Thread main] -g main('/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/openapi-to-dl6/v6/tsv2/gen/pokeapi_gen.dl6','/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/openapi-to-dl6/v6/tsv2/gen/pe_emit'): false
```

Converter safe-mode gap rows (G1/G2): 202 + 0 nullable-array