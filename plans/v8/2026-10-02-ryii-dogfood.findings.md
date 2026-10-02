# ryii dogfood on dl8 Rust (2026-10-02, branch chore/ryii-dogfood off a787b4662)

| op | tier | result | defect |
|---|---|---|---|
| `cleave _7_execute.rs#tuple_position_rules -> _7a_tuple_positions.rs --drag --verify "cargo check"` | fast | applied, verify ok (13.6 s); dragged `handed_to_generic`, exported `term_has_var`/`term_mentions`, carried `Cx`/`TermId` imports | new module line is `pub mod _7a_tuple_positions;` appended at the end of `mod.rs`; repo convention is `#[path = "_7a_tuple_positions.rs"] pub mod tuple_positions;` in numeric order; module made `pub` |
| `rename _5a_generic.rs#kernel_goal -> kernel_relation_goal` | fast | applied; correctly left the unrelated private `kernel_goal` in `_8_express.rs` alone | `rename` has no `--verify` (cleave and move do) |
| `move _7_execute.rs -> _7_goals.rs --verify "cargo check"` | fast | applied, verify ok; rewrote `#[path]` only, module name `execute` kept; listed 8 plain-text mentions in book/plans | none |
| `rename _7_goals.rs#term_mentions -> goal_mentions` | fast | plan edits `_7_goals.rs` only, 0 abstains, rc 0 | **missed 3 sites in `_7a_tuple_positions.rs`** (`use super::execute::{term_has_var, term_mentions}` + 2 calls): the import goes through a `#[path]`-mapped module name. Applying it breaks the build, silently |
| same | slow (`--verify-scip index.scip`, rust-analyzer scip 108 s) | `scip-merge 3 sites`, `disagreements=3`, merged; applied, `cargo check` ok, 0 leftovers | none |

Other:
- Corpus root is the whole git repo: every dry run extracts `v5/`, `v6/`, `labs/` (about 6 s). No flag scopes it to `src/`.
- Default log level INFO prints one line per extracted file on stderr; needs `RUST_LOG=warn`.
- `index.scip` is written at the repo root and is not gitignored.

Fixes to file against ryii:
1. fast rename: resolve `use super::<mod>::{name}` where `<mod>` is declared with `#[path]`; when unsure, abstain (exit 7) instead of rc 0.
2. `rename --verify <cmd>` with rollback, like cleave/move.
3. cleave: emit `#[path = "<file>"] pub(crate) mod <name>;` per the repo's mod.rs pattern, inserted in numeric order.
4. `--scope <dir>` (or default to the nearest Cargo package) for extraction.
5. default log level warn.
