# Design: lower + check, step A1 (prelude lowered and checked once per process)

Gap A of `2026-10-06-phase-atlas.md`. A1 precedes A2 (signed-row deltas, per-rule re-resolve).

## Calls per hello process today

`dl8 compile --trace plans/v8/probes/2026-10-05-one-dd-engine/0_hello.dl7`, plus a code read.

| # | unit | call site | lower | check |
|---|---|---|---|---|
| 1 | macrotime library | `src/_8_driver/_7_one.rs:115` `checked_alone` | 1 | 1 |
| 2 | macrotime library | `src/_8_driver/_3_units.rs:32` / `:86` via `_7_one.rs:137` | 1 | 1 |
| 3 | macrotime library | refreeze (trace `macrotime` `Refreeze outer 1`) | 1 | 1 |
| 4 | prelude | `_7_one.rs:85` `stdlib_program` -> `checked_alone` | 1 | 1 |
| 5 | prelude + user | `src/lib.rs:298` / `:313` `finish_compile` | 1 | 1 |
| 6 | prelude + user | `src/_4_comptime/_7_sources.rs:56` / `:74` refreeze | 1 | 1 |

Inputs differ: rows 1/4 alone without an environment; 2/5 with imports or the attached pool;
3/6 under `CallPolicy::Strict` with a compiler-facts environment.

## Type sigs

```rust
// held by OneEngine after the first compile
pub struct UnitLowered { basement: TermId, origins: Vec<TermId>, exports: TermId }
pub struct UnitChecked { graph: CheckerGraph, arities: Arities, rules: Vec<TermId>, seeds: Vec<TermId>, edges: Vec<TermId> }

fn lower_importers(u: &mut Universe, exporter: &UnitLowered, importers: &[TermId], env: TermId) -> Units
  // lower_units_with_exporter minus the exporter's own lowering
fn check_with(u: &mut Universe, base: &UnitChecked, delta: &Lowered) -> (Option<Checked>, Vec<TermId>)
  // graph = base.graph + delta edges; resolve delta rules; stratify over the union
```

## Open questions (user)

1. Answered 2026-10-06: holding `UnitLowered` / `UnitChecked` on `OneEngine` is allowed; the
   no-caching rule does not cover in-process maps of kept state.
2. Prelude rule resolution reads `graph.forward(owner, name)` up the owner's parent chain
   (`src/_3_check/_2_resolve.rs:94-135`), `graph.by_owner[callee]` for instance columns
   (`_2a_instance.rs:79`), and `relations[target]` arities (`_2_resolve.rs:215`). A user delta
   changes it only through an edge whose owner is a prelude owner: `check_with` keeps resolved
   prelude rules when no delta edge has a prelude owner, else re-resolves them.
   `install_module_aliases` writes alias edges with owner = importer (`_12_units.rs:478`, `:492`),
   never under the prelude. The exporter lowers first (`:99`), importer reruns replace their own
   slot (`:171-176`), and the merge concatenates in that order (`:364-376`): prelude rules hold
   indexes `0..n_prelude`. Runtime guard: a delta edge owned by a prelude owner takes the full path.
3. Prelude lowering is `lower_one(prelude, policy, generated env or empty, imports)`
   (`_12_units.rs:94-99`) and never reads importers. Key the kept map by the interned tuple
   `(unit, policy, env, imports)`: rows 4 and 5 share a key when imports do not touch the
   prelude; row 6 (Strict, facts env) is its own key.
