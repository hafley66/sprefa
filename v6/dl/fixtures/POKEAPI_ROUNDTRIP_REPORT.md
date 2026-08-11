# PokeAPI components round-trip report

Source: `/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-ab605cc630a481ac3/v6/dl/fixtures/pokeapi.openapi.yml`
Generated: `/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-ab605cc630a481ac3/v6/tsv2/gen/pokeapi_gen.dl6`

compile (compile_dl6.sh) exit code: 0
emit-back (4_emit_jsonschema / 5_emit_openapi): OK
source components: 212 | generated component rels: 212 | lifted/enum rels: 161

## KNOWN emitter/compiler gaps (do not fail the gate)

- **G1 `option(<rel>)` on a ref target**: a rel used as a reference target keeps a `type_decl/2` schema mirror minted at parse; the option desugar then removes that column from the rel and moves it to a companion split rel, and the mirror is never retargeted (0_generic_expand.pl:264-268), so a later check reads a column type no rel declares (`unsupported_construct(column_type_unknown(option(...)))`, 0_program_check.pl:342-347). Measured on this base: a ref target carrying `option(int)`, `option(text)`, `option(bool)`, `option(json)`, `list(int)`, `list(<rel>)` or `json_list(int)` compiles green; only `option(<rel>)` stops. Strict mode probes each generic column on its own and drops only the ones that stop the compiler.
- **G2 `option(list(<rel>))`**: the list element's schema mirror is never minted, because the walk that finds element names peels the list flavors and not `option` (parse_dl_dcg.pl:637-646). `option(list(int))` and `option(list(text))` compile; the rel-element case stops as `column_type_unknown(<rel>)`. Nullable arrays otherwise round-trip their nullability.
- Full trace, forks and receipts: `plans/2026-08-11-pokeapi-generic-nesting.md`.

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
COMPILE-TRACE program=pokeapi_gen parse=1364/7700086 plan=844/6525088 lower=187/834964 boot=3/24292 emit=911/8242972 write=33/271 total=3342/23327673
emit-back wrote /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-ab605cc630a481ac3/v6/tsv2/gen/pe_emit/schema.json
emit-back wrote /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-ab605cc630a481ac3/v6/tsv2/gen/pe_emit/openapi.json
```

Converter strict-mode dropped columns (G1): 12; nullable-array drops (G2): 0 (option(list(..)) spelling emitted)

## Emit-back receipt

Emitted component definitions: 212 / 212 source components.
Spot check (formerly json-carrier array-of-refs now round-trips):
`ability_change.effect_entries: list(ability_change_effect_text)` — an array of $ref items to ability_change_effect_text.