# DL7 kernel boundary review

Date: 2026-08-29

## Production inventory

| Layer | File | Physical lines | Nonblank, noncomment lines |
|---|---|---:|---:|
| reader | `v7/src/0_reader/0_parser.pl` | 316 | 285 |
| reader | `v7/src/0_reader/1_expander.pl` | 201 | 179 |
| reader | `v7/src/0_reader/2_embedder.pl` | 82 | 74 |
| reader | `v7/src/0_reader/3_file_loader.pl` | 28 | 24 |
| reader | `v7/src/0_reader/4_cli_mainer.pl` | 53 | 45 |
| libtime | `v7/src/1_libtime/0_evaluator.pl` | 115 | 86 |
| comptime | `v7/src/2_comptime/0_compiler.pl` | 667 | 574 |
| comptime | `v7/src/2_comptime/1_type_compiler.pl` | 113 | 90 |
| total | 8 production modules | 1,575 | 1,357 |

## Dependency order

```text
0_reader/0_parser.pl
0_reader/1_expander.pl
    -> 0_reader/2_embedder.pl
        -> 0_reader/3_file_loader.pl
            -> 0_reader/4_cli_mainer.pl

1_libtime/0_evaluator.pl

2_comptime/0_compiler.pl
    -> 2_comptime/1_type_compiler.pl
       reads 0_reader/2_embedder.pl
       calls 1_libtime/0_evaluator.pl
```

## Evaluator boundary

- Exported evaluator entry points: 1, `evaluate/4`.
- Production call sites: 1,
  `v7/src/2_comptime/1_type_compiler.pl:73`.
- Compiler/runtime phase arguments: 0.
- Compiler/runtime phase branches inside `evaluate/4`: 0.
- Runtime runner modules in this slice: 0.
- Runtime output is retained as the same `checked_datalog/4` data passed around
  comptime. A future runner can call the exported `evaluate/4`; that call site
  does not exist in this slice.
- Evaluation-local dynamic row families: 2, `evaluation_rule/2` and
  `evaluation_seed/2`.
- Tabled closure predicates: 1, `proves/2`.
- Cleanup mechanism: `setup_call_cleanup/3`, scoped by `EvaluationId`.

## Kernel relations

`v7/src/2_comptime/0_compiler.pl` declares 7 relation-shaped kernel nodes:

```text
node/1
module/1
product/1
sum/1
:/4
cons/3
intern/3
```

The evaluator has 2 constructive relation clauses:

```text
cons/3
intern/3
```

The remaining 5 kernel relations are ordinary seed and derived-row families.
Primitive identities `int`, `text`, `any`, and `type` each receive `node/1`.

## Userland Partial boundary

- `Partial` occurrences in reader, compiler, and evaluator production modules:
  0.
- `Partial` implementation location: `v7/prelude/0_types.dl7`.
- Fixture request location: `v7/test/fixtures/2_partial.dl7`.
- Exact semantic oracle location: `v7/test/1_entrypoints.test.pl`.
- The prelude owns constructor application, node/product classification, edge
  copying, ordinal preservation, and Option target mapping.

## Test inventory

- Test modules: 2.
- Test cases: 7.
- Partial vertical tests: 1.
- Partial fixture compilations in that test: 2.
- Latest recorded focused result: 7 passed, 0 failed, 0 choicepoint warnings.
- The review ran no additional suite.

## Boundary findings

1. Reader modules contain syntax and source-location work only.
2. Libtime contains positive closure, reified-variable instantiation, cons,
   intern, and call-local cleanup. It imports no reader or comptime module.
3. Comptime owns graph lowering, checks, kernel graph declaration, graph-to-seed
   conversion, prelude loading, and artifact assembly.
4. Runtime execution is represented by retained checked data. A runtime runner
   remains outside the completed slice.
5. Effect dispatch, ticks, negation, aggregates, PAP, nested expression
   application, Rust emission, and TypeScript emission have 0 implementation
   files in this slice.
