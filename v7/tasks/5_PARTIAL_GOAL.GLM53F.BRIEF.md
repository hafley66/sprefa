## Description

Add the compile driver and one `.dl7` standard-library definition of Partial.
The reader, graph kernel, and evaluator must remain unaware of Partial.

## Signature

```prolog
compile_dl7(+Path, -CompilerRows, -RuntimeProgram, -Diagnostics).
```

## Timeline and storage

Read prelude and program, lower both, run compiler rules through `evaluate/4`,
drain ground intern requests, and return runtime rules in the same IR.

## Acceptance Criteria

- [ ] Partial behavior exists only in `.dl7` prelude rules.
- [ ] Partial copies names and indices and maps targets through Option.
- [ ] Compiler closure and runtime program use the same normalized rule shape.
- [ ] Compile twice in one process produces identical terms.
- [ ] Driver stays under `v7/3_COMPILE/`; prelude under `v7/4_PRELUDE/`.
- [ ] Adds no standalone test file.

## Test Run

Use one direct end-to-end SWI receipt until the oracle lands.

## Stop condition

Hail the parent if Partial requires a macro, recursive construction, or
operator-specific kernel clause.
