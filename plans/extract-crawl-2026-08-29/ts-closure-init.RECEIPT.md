# ts closure-caller mirror + unannotated-const receivers (lane `fix-extract-ts-closure-init`)

Two `ts.GAPS.md` classes, both measured against the tsc oracle
(`plans/extract-bench-2026-08-29/ts5.oracle.call.tsv`, 59,356 rows).

## Contents

- [Numbers](#numbers)
- [What each leg does](#what-each-leg-does)
- [What the init leg still misses](#what-the-init-leg-still-misses)
- [How to reproduce](#how-to-reproduce)

## Numbers

ONE process, `~/projects/TypeScript-5.9`, `find src -name '*.ts' ! -name '*.d.ts'`
(600 files), `--resolve --project-root`, rc=0 every run.

| stage | ours | ours ∩ oracle | recall (∩/ours) | precision (∩/oracle) | drops | wall | peak RSS |
|---|---:|---:|---:|---:|---:|---:|---:|
| baseline (#566, `ts5.parse.call.tsv`) | 59,311 | 41,547 | 70.05% | 70.00% | 7,638 | n/a | n/a |
| after A (closure mirror) | 65,974 | 46,380 | 70.30% | 78.14% | 7,638 | 3.14s | 384 MB |
| after B (init receivers) | 66,714 | 46,958 | 70.39% | 79.11% | 6,865 | 3.31s | 383 MB |

Oracle-only rows 17,809 -> 12,398. Ours-only 17,764 -> 19,756.

Task A adds 6,663 rows of which 4,833 (73%) are rows the oracle also has.
Task B adds 740 rows of which 578 (78%) are rows the oracle also has.

A rejected variant of task B keyed the initializer binding by the innermost
NAMED enclosing def instead of walking the lexical chain: 105 extra rows, 12 of
them oracle-confirmed (11%), recall 70.19%. The lexical chain replaced it.

## What each leg does

| leg | site | rule |
|---|---|---|
| closure mirror | `ts.rs` resolve arm, edge emission | a call whose caller def is a Lambda also emits onto the innermost NAMED def covering the site; the closure row stays. Same shape and kind as the rust and go arms, and the same walk `oracle_ts.mjs` `enclosingName` does, `<module>` included as its fallback |
| init receiver, cross-file | `ts.rs` `bound_types` + `ts_receivers.rs` `visit_function` | `const printer = createPrinter()` binds `printer` to the callee's DECLARED return type. Two kinks fixed: the type name is anchored in the CALLEE's file (the caller never imports it), and `ret_of` is keyed by the fn's name identifier as well as the `function` keyword, since the module plane's export entry seats an exported fn at its name |
| init receiver, closures | `ts.rs` `covering_chain` | a nested arrow reads the outer `const` off the chain of covering defs, which is what a closure does lexically. Keyed by the def the `const` sits in, so a sibling closure never leaks |

## What the init leg still misses

300 random remaining `inferred` drops, bucketed by the receiver's initializer:

| bucket | rows |
|---|---:|
| no `const`/`let`/`var` initializer at the receiver name (param, destructured, field) | 100 |
| qualified or member call init (`ts.createX()`, `this.f()`) | 83 |
| bare call init, callee's return type unwritten / union / generic | 53 |
| literal, `new`, arrow, `||` inits | 22 |

The bare-call remainder needs the return type INFERRED from the callee body
(tsc does this; we read written annotations only). The qualified bucket is the
namespace-merged class `ts.GAPS.md` assigns to `scan_module_specifiers` +
`modules.member`.

## How to reproduce

```bash
cd v6/sprefa-extract && cargo build --release --features cli
cd ~/projects/TypeScript-5.9
/usr/bin/time -l timeout 120 <repo>/v6/sprefa-extract/target/release/extract \
  --resolve --project-root ~/projects/TypeScript-5.9 \
  $(find src -name '*.ts' ! -name '*.d.ts') > /tmp/ts.jsonl
python3 plans/extract-bench-2026-08-29/normalize.py resolved /tmp/ts.jsonl \
  ~/projects/TypeScript-5.9 /tmp/ts.call.tsv /tmp/ts.type.tsv
python3 plans/extract-bench-2026-08-29/bench.py /tmp/ts.call.tsv \
  plans/extract-bench-2026-08-29/ts5.oracle.call.tsv
```

The normalized tsvs are 7 MB each and are not committed (the 1 MB rule).
