# PokeAPI components round-trip report

Source: `/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a1237a7d94bba8cfe/v6/dl/fixtures/pokeapi.openapi.yml`
Generated: `/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a1237a7d94bba8cfe/v6/tsv2/gen/pokeapi_gen.dl6`

compile (compile_dl6.sh) exit code: 0
emit-back (4_emit_jsonschema / 5_emit_openapi): OK
source components: 212 | generated component rels: 212 | lifted/enum rels: 161

## KNOWN emitter/compiler gaps (do not fail the gate)

- **G1 lifted rel name == option companion rel name**: this converter names a lifted inline-object rel `<parent>__<prop>`, and the compiler's reference-option desugar names its companion split rel `<parent>__<column>` (0_option_expand.pl companion_rel_decls/4), so every nullable lifted-object property declares one name at two arities and the program stops with `unsupported_construct(rel_arity_collision(<parent>__<prop>, 1, 2))`. All 12 dropped columns are this shape, measured. `option(<rel>)` and `option(list(<rel>))` themselves compile: conformance fixtures 14_option_wrapper_walk.pl round-trip both, absent and present.
- **G2 a reference target whose every column is a reference option**: `move_detail__contest_combos__normal` and `__super` carry only `option(list(<rel>))` columns, so the desugar moves both to companion rels and the parent shrinks to zero columns. A zero-column ref target has no full-row identity, which is what the type plane falls back to without `key(...)`; it stops as `column_type_unknown(option(list(<rel>)))`. This is a design question, not unfinished bookkeeping.
- Renaming the converter's lifted rels alone takes the drop count 12 -> 4, measured; the remaining 4 are G2.
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
COMPILE-TRACE program=pokeapi_gen parse=1450/7711179 plan=962/9063849 lower=195/840598 boot=1/24516 emit=920/8329230 write=34/271 total=3562/25969643
emit-back wrote /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a1237a7d94bba8cfe/v6/tsv2/gen/pe_emit/schema.json
emit-back wrote /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a1237a7d94bba8cfe/v6/tsv2/gen/pe_emit/openapi.json
```

Converter strict-mode dropped columns (G1): 4; nullable-array drops (G2): 0 (option(list(..)) spelling emitted)

## Emit-back receipt

Emitted component definitions: 212 / 212 source components.
Spot check (formerly json-carrier array-of-refs now round-trips):
`ability_change.effect_entries: list(ability_change_effect_text)` — an array of $ref items to ability_change_effect_text.