# DL6 donor audit index

DL6 is a donor corpus for V7. Source compatibility, declaration syntax, and
phase layout are outside the reuse boundary.

## Reports

| Slice | Report | Smallest useful donor boundary |
|---|---|---|
| Reader and CST | [1_READER.md](1_READER.md) | literals, comments, source positions, parse diagnostics |
| Expansion | [2_EXPANSION.md](2_EXPANSION.md) | body walking, variable-sharing laws, phase-runner skeleton |
| Lowering | [3_LOWER.md](3_LOWER.md) | graph algorithms and plan-to-engine semantic laws |
| Scope | [4_SCOPE.md](4_SCOPE.md) | scope-tree construction, path collision checks, path resolution |
| Types | [5_TYPES.md](5_TYPES.md) | semantic type IDs and deterministic encodings |
| Generics | [6_GENERICS.md](6_GENERICS.md) | interning, canonical rows, demand and refreeze laws |
| Compiler fixpoint | [7_COMPILER_FIXPOINT.md](7_COMPILER_FIXPOINT.md) | strata, tabled closure, aggregates, functional-key checks |
| Checks | [8_CHECKS.md](8_CHECKS.md) | dependency checks, stratification, clock graph algorithms |
| Runtime | [9_RUNTIME.md](9_RUNTIME.md) | fixpoint, tick, occurrence, retention, and boundary-delta laws |
| Emitters | [10_EMITTERS.md](10_EMITTERS.md) | registries, type renderers, schemas, plan serialization laws |
| Oracles | [11_ORACLES.md](11_ORACLES.md) | semantic fixture clusters independent of DL6 spelling |
| Rust engine | [12_ENGINE_CONTRACT.md](12_ENGINE_CONTRACT.md) | ProgramJson, arrival DTOs, tick phase order, `ir_version` |

## Reuse classes

### Extract as isolated SWI-Prolog modules

- Literal reading, escapes, comments, line and column calculation, and parse
  error locations.
- Pure graph algorithms from lowering and checks.
- Semantic ID encoding from `0_type_ids.pl`.
- Canonical type-name encoding and content-derived generated names.
- Tabled positive closure, strata calculation, aggregate separation, and
  functional-key validation.
- Pure runtime laws whose inputs and outputs do not contain DL6 syntax terms.

### Adapt to V7 term contracts

- Scope and path resolution. Current code stores `__`-joined names and reads a
  flat declaration list.
- Type-plane and generic logic. Current code reads `col_type/3`,
  `type_decl/2`, `rel_template/3`, and one `semantic_type_rows/1` carrier.
- Lowering. Current code consumes `plan/9`, DL6 expression terms, and
  positional catalog IDs.
- Emitters. Current code reaches back into analysis and lowering modules for
  facts that need to arrive through one plan value.
- Compiler relation binding. Current code recognizes DL6 `:=` forms and
  guesses compiler-plane ownership from `type` columns.

### Preserve as semantic oracles

- Tick boundaries, occurrence identity, keyed writes, retention, and stream
  behavior.
- Type identity, structural type expansion, recursion refusal, module
  resolution, and generic specialization.
- Existing ProgramJson decoding and Rust engine phase order.
- Cross-door tests where Prolog and the Rust engine must produce the same
  rows.

### Leave in DL6

- `rel` declarations, braces, dotted declaration spelling, infix declaration
  operators, named-argument puns, and anonymous declaration expansion policy.
- `__` name mangling as semantic identity. It may remain inside a target
  emitter as an artifact-name encoding.
- The TS V2 execution path.
- DL6 declaration-list carriers used only to communicate between old phases.

## Reuse rule

A reused predicate must have a V7-owned input and output signature. Copying a
predicate together with a DL6 declaration carrier extends the old compiler
boundary and therefore counts as adaptation.
