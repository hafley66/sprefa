# Base type IR TypeScript and Rust renderers

| file | lines |
|---|---:|
| `4_emit_jsonschema.pl` (existing, the yardstick) | 176 |
| `5_emit_openapi.pl` (existing, the yardstick) | 103 |
| `7_emit_ts_types.pl` (new) | 69 |
| `8_emit_rust_types.pl` (new) | 69 |
| `sweep.pl` diff | +28 / -1 |
| `emit_type_renderers.test.pl` (new) | 40 |
| `2026-08-12-type-ir-ts-rs-renderers.md` (new) | 63 |
| **total added** | **269** |

## Context

`catalog_decl_rows/6` supplies the `row/11` catalog shared with
`jsonschema_text/3`. The two renderers read those rows directly, with
`option_rows/3` restoring scalar `option(T)` rows before rendering.

The JSON Schema emitter drops option fields from `required` at
`v6/prolog/compile/4_emit_jsonschema.pl:121`. This admits both missing and
null wire states. This lane leaves that emitter unchanged. TypeScript renders
the present field as `T | null`; Rust renders it as `Option<T>`.

## Decisions

| Catalog constructor | TypeScript receipt | Rust receipt |
|---|---|---|
| `int` | `number` | `i64` |
| `float` | `number` | `f64` |
| `text` | `string` | `String` |
| `bool` | `boolean` | `bool` |
| `json` | `unknown` | `serde_json::Value` |
| named relation record | `export interface PascalCaseName` | `pub struct PascalCaseName` |
| `option(T)` | `T | null` | `Option<T>` |
| `json_list(T)` | `Array<T>` | `Vec<T>` |

Relation references use the same PascalCase type name as their declaration.
Rust structs use `Debug`, `Clone`, `PartialEq`, `serde::Serialize`, and
`serde::Deserialize`; fields are public.

`list(T)`, `list_interned_set(T)`, `list_entity_dense_sequence(T)`, and
`list_entity_linked_sequence(T)` have no generated declaration. The corpus
catalog count command returned `[]`: zero rows for each constructor, hence zero
skipped relations for each constructor across 286 compiled fixtures.

## Verification

| command | result |
|---|---|
| `swipl -q -l v6/prolog/compile/test/emit_type_renderers.test.pl -g run_tests -g halt` | 2 passed |
| `pnpm exec tsc --noEmit /tmp/sprefa-types.ts` | zero errors |
| scratch `cargo check --quiet` including `/tmp/sprefa-types.rs` | zero errors |
| `bash v6/tsv2/scripts/sweep.sh` | `RUN total=286 identical=283 wrong=0` |
| type output count command | `286/286` TypeScript and `286/286` Rust |

## Staffing

| item | value |
|---|---|
| base SHA | `259e0289` |
| worktree | `feature/type-ir-ts-rs-renderers` |
| implementation | single lane |
