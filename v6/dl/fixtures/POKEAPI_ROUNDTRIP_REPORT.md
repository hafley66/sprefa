# PokeAPI components round-trip report

Source: `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/pokeapi-strict/v6/dl/fixtures/pokeapi.openapi.yml`
Generated: `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/pokeapi-strict/v6/tsv2/gen/pokeapi_gen.dl6`

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
| match | 769 |
| mismatch | 0 |
| known gap | 17 |
| total compared | 786 |

_17 known-gap rows (sample):_
- `G1 ability_change.effect_entries`
- `G1 item_price.purchase_price`
- `G1 item_price.sell_price`
- `G1 move_change.accuracy`
- `G1 move_change.power`
- `G1 move_change.pp`
- `G1 move_change.effect_entries`
- `G1 move_meta.min_hits`
- `G1 move_meta.max_hits`
- `G1 move_meta.min_turns`
- `G1 move_meta.max_turns`
- `G1 move_meta.drain`
- `G1 move_meta.healing`
- `G1 move_meta.crit_rate`
- `G1 move_meta.ailment_chance`
- `G1 move_meta.flinch_chance`
- `G1 move_meta.stat_chance`

### Per-property ref target

| metric | count |
| --- | ---: |
| match | 256 |
| mismatch | 0 |
| known gap | 0 |
| total compared | 256 |

### Per-property nullability

| metric | count |
| --- | ---: |
| match | 771 |
| mismatch | 0 |
| known gap | 15 |
| total compared | 786 |

_15 known-gap rows (sample):_
- `G1 item_price.purchase_price`
- `G1 item_price.sell_price`
- `G1 move_change.accuracy`
- `G1 move_change.power`
- `G1 move_change.pp`
- `G1 move_meta.min_hits`
- `G1 move_meta.max_hits`
- `G1 move_meta.min_turns`
- `G1 move_meta.max_turns`
- `G1 move_meta.drain`
- `G1 move_meta.healing`
- `G1 move_meta.crit_rate`
- `G1 move_meta.ailment_chance`
- `G1 move_meta.flinch_chance`
- `G1 move_meta.stat_chance`

## Compile / emit receipts

```
wrote /var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/pe_pokeapi_gen.ts
COMPILE-TRACE program=pokeapi_gen parse=1434/7228204 plan=593/5060477 lower=150/671298 boot=2/22144 emit=743/7493601 write=29/179 total=2951/20475903
emit-back wrote /Users/chrishafley/projects/sprefa/.boop-worktrees/feature/pokeapi-strict/v6/tsv2/gen/pe_emit/schema.json
emit-back wrote /Users/chrishafley/projects/sprefa/.boop-worktrees/feature/pokeapi-strict/v6/tsv2/gen/pe_emit/openapi.json
```

Converter strict-mode dropped columns (G1) + nullable-array (G2): 75 + 4

## Emit-back receipt

Emitted component definitions: 212 / 212 source components.
Spot check (formerly json-carrier array-of-refs now round-trips):
`ability_detail.names: list(ability_name)` — an array of $ref items to ability_name.