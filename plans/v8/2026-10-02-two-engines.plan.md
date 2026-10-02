# Two engines: ir:sqlite (comptime default) and ir:dd (cross-check)

User (2026-10-02): "i just want those 2. the comptime favorite is sqlite_ivm to prove it out but in
reality dd is always there to keep it comparatively correct ... recursively doing string ops or
thinking recursion is okay in the compiler _at all_" is wrong.

## Laws

- Engines: `ir:sqlite` (default, comptime) and `ir:dd` (cross-check). Both through `ivm_ir` +
  `ivm_engine::Engine`. Nothing else evaluates.
- Both engines share `ivm_ir`, so agreement between them does not prove correctness. The frozen
  goldens (`oracle/refreeze.py` output) stay the independent oracle.
- No recursion in kernel and engine Rust (user: "by compiler i mean kernel in rust"): `_6_eval` kernel/eval path, `ivm_*` engines; no function calls itself, directly or through a cycle.
  Trees are walked with an explicit stack/worklist and a named depth/size limit diagnostic.
- No eager per-character string structure. Text is stored whole; decompose works on demand.

## Lanes (fresh agent each, sequential, one Rust process at a time)

| # | repo | job | done when |
|---|---|---|---|
| 1 | sqlite_ivm | string storage: no recursion, no eager suffix/head minting, decompose on demand (DD + SQLite) | 1 MB text mints in O(n) memory on both backends; `random_string` seeds pass; no self-calling fn in `crates/ivm-ir/src/1_terms.rs`, `ivm-sqlite/src/2_terms.rs` |
| 2 | sprefa (dl8) | `ir:sqlite` is the default; delete `Engine::{Rust,Sqlite,Dd}` arms, `_5_evaluate.rs` evaluate_into, `_7_sqlite_eval.rs`, `_9_dd_eval.rs`; tests run ir:sqlite and ir:dd against goldens | `cargo test` green on both; whole-file emit (`registry.rs`, str_ops) on both; `DL8_ENGINE` accepts only the two |
| 3 | sprefa (dl8) | kernel recursion purge: kernel/eval/ir-lowering entries of the 37 self-calling fns (ryii list, `2026-10-02-two-engines.recursion.txt`) plus cycles, to worklists; a test fails on any new self-call | test lists zero |
| 4 | sprefa (dl8) | `dl8 emit sqlite` (`_5_reify/_7_sqlite.rs`, 2517 lines): who uses it; delete or move onto `ivm_ir` | user decision with callers listed |

Lane 2 decides whether `_5_evaluate.rs` keeps any pure helpers (term compare, stratify) that the IR
lowering imports; the semi-naive loop itself goes.
