# DL7 kernel contract result

Date: 2026-08-28

Status: blocked on declared-node module identity.

## Result

The minimal vertical-slice plan now pins the executable contracts for reader
terms, public colon edges, callable tuple outputs, shared value and type
application lowering, structural interning, ground construction requests, the
compiler request loop, and one phase-independent evaluator.

The stop condition fired on declared-node identity. The parent directed this
card to preserve both exact options in the plan, mark the card blocked, commit
documentation only, and stop before implementation or tests.

## Pinned contracts

| Surface | Contract |
|---|---|
| Reader tree | `node(NodeId, atom(Name) | variable(VariableId, Name) | literal(Value) | form(Nodes))` with `source/8` rows. |
| Reader identity | `reader_node(Path, PreorderIndex)`; named variables share `variable(TopNodeId, Name)` within one top-level form; every `?_` is fresh. |
| Public edge | `':'(Owner, Name, Target, Index)` with keys `(Owner, Name)` and `(Owner, Index)`; indices are zero-based and contiguous. |
| Edge reference | The complete ground `':'/4` row. No synthetic public edge ID. |
| Callable | Input colon edges at indices `0..N-1`, one `return` edge at `N`, and `callable(Callable, Name/Arity, N)` where `Arity = N + 1`. |
| Application | A nested call with `N` arguments lowers to one saturated relation atom with the fresh result appended as tuple column `N`. Value and type calls use the same lowering. |
| Rule IR | Ordered `relation(Name/Arity, KeySets)` and `rule(HeadAtom, Goals)` terms; facts are ground seeds. |
| Interning | `intern(Constructor, Arguments, application(Constructor, Arguments))` over a ground constructor and proper ground ordered argument list. |
| Request | `construction_request(application(Constructor, Arguments), Constructor, Arguments)`, keyed by `(Constructor, Arguments)`. |
| Materialization rows | `specialization(Result, Constructor, Arity)` keyed by `Result`; `argument(Result, Index, Value)` keyed by `(Result, Index)`. |
| Evaluator | One `evaluate(Rules, Seeds, Closure, Diagnostics)` body for compiler and runtime calls, with positive tabled closure, lower-stratum `not/1`, functional-key validation, ground `intern/3`, and complete EvalId cleanup. |
| Request loop | Evaluate, drain unseen ground requests, verify identity through `intern/3`, add specialization and argument seeds, and repeat until semantic rows and request keys stabilize. The donor cap remains 16 calls. |
| Partial | Authored in `v7/4_PRELUDE/0_types.dl7`; kernel modules contain no `Partial` clause or name dispatch. |

The plan contains pseudocode comments directly below all four requested
signatures:

```prolog
read_dl7(+Path, +Text, -Forms, -SourceMap, -Diagnostics).
lower_dl7(+ModulePath, +Forms, -Rules, -Seeds, -Requests, -Diagnostics).
evaluate(+Rules, +Seeds, -Closure, -Diagnostics).
compile_dl7(+Path, -CompilerRows, -RuntimeProgram, -Diagnostics).
```

## Shared evaluator timelines

Compiler execution calls `evaluate/4` with compiler-owned declarations, rules,
and semantic seeds. Each request round allocates an EvalId, installs copied
rules and seeds, closes the strata, copies closure rows out, abolishes the
EvalId tables, and retracts all temporary facts. The driver alone drains
`construction_request/3` rows and starts the next round.

Reference runtime execution destructures
`runtime_program(RuntimeRules, RuntimeSeeds)` and calls the same `evaluate/4`.
It allocates and cleans a separate EvalId through the same clauses. The
evaluator receives no phase option and reads no compiler or runtime tag.

The plan's storage tables list every first-slice row family, key, lifetime, and
cleanup boundary, including reader nodes, spans, variables, graph rows,
normalized rules, seeds, closure rows, construction rows, diagnostics, and
evaluator temporary state.

## Blocking identity ruling

### Option A: donor module hash

```prolog
named(ModuleHash, Kind, Name)
```

`ModuleHash` is the first 8 SHA-256 digest bytes rendered as 16 lowercase hex
characters. The digest input is the extensionless path relative to the entry
file's directory, using `/` between directory and stem. This preserves the
current donor shape. A 64-bit collision aliases modules unless V7 carries the
normalized stem beside the hash and rejects a collision. Checkout relocation
preserves identity; relative module rename changes it. Cross-implementation
portability requires identical path, separator, Unicode, and extension
normalization.

### Option B: structural module path

```prolog
named(module(ModulePath), Kind, Name)
```

`ModulePath` is the nonempty ordered atom-segment list derived from the
extensionless path relative to the entry directory. Structural equality avoids
hash aliasing. Checkout relocation preserves identity; relative module rename
changes it. Cross-implementation portability requires the same path,
separator, Unicode, dot-segment, and extension normalization. Semantic rows
and serialized compiler artifacts retain the complete path term.

The choice changes `compile_dl7/4` module derivation, the meaning or shape of
the `lower_dl7/7` first argument, every declared `named/3` ID, module-owner
colon edge, callable constructor, `application/2` specialization,
construction request, `CompilerRows`, compile-twice snapshot, and later
ProgramJson projection. `read_dl7/5` and `evaluate/4` remain unchanged.

## Donor receipts

| Input | Contract used |
|---|---|
| Boop favorites 26 through 37 | Prefix forms, `?x` variable identity, `'x` symbol data, output tuple columns, uniform value and type application, structural specialization, interning, and one relational fixpoint. |
| `1_READER.md` | Literals, escapes, comments, positions, call-local variable identity, and the boundary excluding DL6 statement dispatch. |
| `2_EXPANSION.md` | Ordered body walking, variable sharing, normalized flat rule inputs, and exclusion of DL6 phase spellings. |
| `3_LOWER.md` | Graph and plan laws plus the boundary excluding `plan/9`, `lowered/8`, and SQL lowering from the kernel. |
| `4_SCOPE.md` | Reserve-before-resolve, collision checks, local-first scope lookup, authored order, and absence of implicit parent columns. |
| `5_TYPES.md` | `named/3`, `primitive/1`, `application/2`, semantic encoding separation, and the unresolved module-identity input. |
| `6_GENERICS.md` | Ground application requests, first-occurrence request deduplication, refreeze stability, 16-round exhaustion, and compiler-transport cleanup. |
| `7_COMPILER_FIXPOINT.md` | Authored-order safety, strata, table namespace per evaluation, cleanup order, functional keys, and recursive construction refusal. |
| `8_CHECKS.md` | Shared key validation, dependency strata, lower-stratum negation, and deterministic diagnostic ordering. |
| `9_RUNTIME.md` | Runtime relation closure as a rule-and-seed call, call-local state, and the later engine ownership boundary. |
| `10_EMITTERS.md` | ProgramJson remains after the kernel proof; target emission and `plan/9` carriers stay outside the four modules. |
| `11_ORACLES.md` | One complete expected term, compile-twice cleanup determinism, and semantic fixture coverage without a generated corpus. |
| `12_ENGINE_CONTRACT.md` | Existing Rust engine and ProgramJson fields remain unchanged; the first slice returns normalized runtime rules for a later adapter. |

## Production and test ceiling

The plan names exactly four production modules:

```text
v7/0_READER/0_reader.pl
v7/1_KERNEL/0_kernel.pl
v7/2_EVALUATOR/0_evaluator.pl
v7/3_COMPILE/0_compile.pl
```

It names one prelude, one fixture, and one test file. This card adds no
production or test file.

## Acceptance criteria

- [x] Public edge order is Owner, Name, Target, Index.
- [x] Relation output remains a tuple column.
- [x] Application lowering serves values and types.
- [x] One evaluator body runs compiler and runtime rules.
- [x] `intern/3` is the only domain-construction primitive.
- [x] Partial remains a `.dl7` proof goal.
- [x] Plan stays within four production modules and one test.
- [x] Report written to `v7/tasks/results/0_KERNEL_CONTRACT.md`.
- [ ] Declared-node module identity selected. Parent directed this ruling to
      remain open under the task stop condition.

## Tests run

Read-only document and diff checks only. No suite, SWI command, V6 test, Rust
test, generated corpus, formatter, or linter ran.

CI coverage added, changed, or removed: 0.
