# PokeAPI components round-trip report

Source: `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/import-openapi-hover/v6/dl/fixtures/pokeapi.openapi.yml`
Generated: `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/import-openapi-hover/v6/tsv2/gen/pokeapi_gen.dl6`

compile (compile_dl6.sh) exit code: 0
emit-back (4_emit_jsonschema / 5_emit_openapi): OK
source components: 212 | generated component rels: 212 | lifted/enum rels: 161

## KNOWN emitter/compiler gaps (do not fail the gate)

- **G1 a reference target whose every column is a reference option**: `move_detail__contest_combos__normal` and `__super` carry only `option(list(<rel>))` columns, so the desugar moves both to companion split rels and the parent keeps zero stored columns. Target identity is `key(...)` or the full row (0_type_plane.pl header) and a zero-column row has neither, so it stops as `unsupported_construct(reference_target_has_no_columns(<rel>/0))`. All 4 remaining drops are this shape, measured. Reaching 0 needs a ruling on what identity such a target has; the forks are in plans/2026-08-11-option-list-rel-generic.md.
- **CLOSED, `option(<rel>)` and `option(list(<rel>))` on a reference target**: both compile and round-trip absent and present (conformance fixtures 14_option_wrapper_walk.pl). The 12 drops this file used to report were a rel-name collision: a lifted inline-object rel named `<parent>__<prop>` is the name the reference-option desugar mints for that property's companion split rel. A nullable lifted object now takes the `_object` suffix, which took the count 12 -> 4.
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
COMPILE-TRACE program=pokeapi_gen parse=1540/7711179 plan=947/9063849 lower=190/840598 boot=2/24516 emit=914/8329951 write=36/271 total=3629/25970364
emit-back wrote /Users/chrishafley/projects/sprefa/.boop-worktrees/feature/import-openapi-hover/v6/tsv2/gen/pe_emit/schema.json
emit-back wrote /Users/chrishafley/projects/sprefa/.boop-worktrees/feature/import-openapi-hover/v6/tsv2/gen/pe_emit/openapi.json
```

Converter strict-mode dropped columns (G1): 4; nullable-array drops (G2): 0 (option(list(..)) spelling emitted)

## Emit-back receipt

Emitted component definitions: 212 / 212 source components.
Spot check (formerly json-carrier array-of-refs now round-trips):
`ability_change.effect_entries: list(ability_change_effect_text)` — an array of $ref items to ability_change_effect_text.