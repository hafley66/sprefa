# emit_rust grade climb

## Table of contents

- [Measurements](#measurements)
- [Cause order](#cause-order)
- [Unsupported construct trace](#unsupported-construct-trace)
- [Validation](#validation)

## Measurements

| receipt | result |
|---|---:|
| inherited grade | `RUST-GRADE graded=392 byte-clean=103` |
| after runtime causes | `RUST-GRADE graded=392 byte-clean=109` |
| grade wall time | 13.044 s cold, 9.6 s warm |

`grade.sh` creates and compiles a temporary Rust check crate for the text-door
receipt. Measure that crate separately before changing the grade contract.

## Cause order

| order | verdict | cause | fixtures | current action |
|---:|---|---|---:|---|
| 1 | diff | first differing tick line | 171 | group by output relation and tick before changing runtime behavior |
| 2 | unsupported | `type_arrival_shape_mismatch` | 11 | compiler-front validation, outside this lane |
| 3 | unsupported | `edge_body_needs_json_destructure` | 9 | compiler-front lowering, outside this lane |
| 4 | unsupported | `trigger_arg_not_var` | 4 | compiler-front validation, outside this lane |
| 5 | unsupported | `lifecycle_arm` | 4 | compiler-front validation, outside this lane |
| 6 | unsupported | `level_body_goal` | 4 | compiler-front validation, outside this lane |

## Unsupported construct trace

| reason | fixtures | throw site | emitter site | disposition |
|---|---:|---|---|---|
| `type_arrival_shape_mismatch` | 11 | `v6/prolog/compile.pl:317-318` | no `throw/1` in `v6/prolog/emit_rust.pl` | compiler-front validation |
| `edge_body_needs_json_destructure` | 9 | `v6/prolog/analyze.pl:1065-1067` | no `throw/1` in `v6/prolog/emit_rust.pl` | compiler-front TODO, recorded at `v6/prolog/ARCH.pl:828` |
| `trigger_arg_not_var` | 4 | `v6/prolog/lower.pl:3182` | no `throw/1` in `v6/prolog/emit_rust.pl` | compiler-front validation |

`grade.pl` calls the emitter after `program_plan/3` and `lower_program/2`.
The listed compiler-front paths are forbidden to this lane. The emitter has
zero `throw/1` matches, so no emitter decline path exists for these rows.

## Validation

| command | required result |
|---|---|
| `cd v6/sprefa-engine-rs && bash grade.sh` | `RUST-GRADE graded=392 byte-clean=109` or higher |
| `cd v6/tsv2 && bash scripts/sweep.sh` | `MANIFEST_REASON_DIFF` counts all zero |
| `cd v6/prolog && swipl -g go -t halt ARCH.pl` | all PASS |
