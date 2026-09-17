# Namespacing

[One shape](#one-shape) · [Where names live](#where-names-live) · [Collisions](#collisions) · [Forks](#forks) · [Receipts](#receipts)

The full inspection, with every probe's pasted output: `plans/v8/2026-09-15-v8-namespacing.inspection.md`.

## One shape

| relation kind | hosted by | a name meets its id at |
|---|---|---|
| kernel, `intern`, `effect` | the evaluator; minted as `ref(kernel(Name))`, never declared in `prelude/` | `src/_2_lower/_1_slots.rs:36-42`, `src/_6_eval/_4_kernel.rs:24-28` |
| served, `timer`, `fetch_json` | an executor | `program_names` (`src/_6_eval/_6_json.rs:333-343`) and `executors_for` (`src/_9_runtime/_3_executors/mod.rs:61-100`) |
| declared | rules and facts | `program_names` only |
| every kind, past those two places | a relation id: `Program.served` is a `HashSet<TermId>` (`src/_6_eval/_1_program.rs:171`), the effect row keys on it (`src/_6_eval/_5_evaluate.rs:244-249`) | nowhere |

## Where names live

| space | example | lookup order | read at |
|---|---|---|---|
| product field scope | `Name` inside `Holder` | first | `src/_2_lower/_10_index.rs:170-193`, `src/_3_check/_2_resolve.rs:70-77` |
| file module | `User` in `10_accounts.dl7` | after the fields | same |
| directory module | the edge `accounts` on the project root | through parents | `src/_4_comptime/_0_load/_2_project.rs:195-247` |
| prelude alias | `Option`, `bool` | after the file, only when the file does not bind the name | `src/_2_lower/_12_units.rs:310-342` |
| kernel | `intern`, `effect` | after every edge | `src/_2_lower/_8_express.rs:181-184`, `_2_resolve.rs:78-83` |
| primitive | `int`, `text` | last | `_2_resolve.rs:84-87` |
| runtime name table | every top-level name of every module, one flat map | not ordered: prelude yields, two user names stop | `src/_6_eval/_6_json.rs:178-221`, read by `program_names` `:333-343` |
| served name | `timer`, `fetch_json_error` | the `--serve` string against one arm each | `src/_9_runtime/_3_executors/mod.rs:61-100` |

## Collisions

| collision | probe | result |
|---|---|---|
| user product named `intern` | `probes/3_user_intern.dl7` | rc=0 |
| user `intern` beside a kernel `intern` goal | `probes/4_user_intern_kernel_goal.dl7` | `arity_mismatch(intern, 1, 3)` |
| user `Option` without a `return` column | `probes/5_user_option_shadows.dl7` | `expression_without_return` |
| field typed `bool` | `probes/17_bool_field.dl7` | the prelude product `bool`, never the primitive |
| user `timer` served with `--serve timer` | `probes/9_user_timer_rule.dl7` | rc=0, an `effect` row, no answer |
| two modules declaring `User` | `probes/6_user_a.dl7`, `7_user_b.dl7` | `duplicate_relation_name(User, ...)` |
| a goal qualified by module, same two modules | `probes/8_pick_user_a.dl7` | the goal resolves; `duplicate_relation_name` still stops |
| a relation named like a module | `probes/12_relation_named_accounts.dl7` | `duplicate_relation_name(accounts, ...)` |
| bare `User` from another user module | `probes/18_bare_consumer.dl7` | `undeclared_relation(User)` |
| dotted name | `probes/19_dotted_name.dl7` | `unresolved_name(boop)`: a path, never one atom |
| `accounts/User`, `accounts:User`, `accounts::User`, `http:fetch:get` | inspection sections 2 and 3 | `invalid_atom` |
| the path `http`, `fetch`, `get` as `:` goals | `probes/23_colon_path_walk.dl7` | rc=0, `(found get)` |
| a partial application as a field type | `probes/21_partial_in_field.dl7` | `partial_application_requires_more_arguments` |

## Forks

| spelling | example | what it touches | `src` files |
|---|---|---|---|
| A. prefix by convention, today | `boop_lane` | one `executors_for` arm per name | 1 |
| B. dotted atom, flat | `boop.lane` | one `executors_for` arm; the reader and the name table accept it today | 1 |
| C. dotted atom split into module and name | `accounts.User` | lowerer and checker lookups, name table key, `executors_for`, a named prelude module | 5 |
| D. graph goal in source, table keyed by module | `(: accounts User ?T ?I)`, `--serve accounts/User` | name table key, `serve_relations`, `executors_for` | 3 |
| E. Prolog colon | `accounts:User` | reader (conflicts with `label:`), lowerer, checker, name table, `executors_for` | not priced |
| F. import form | `(use accounts User)` | the alias installer: a user module as exporter | not priced |
| G. `Hosted` rows bind an executor | `(Hosted boop_lane BoopLaneExecutor)` | `executors_for` reads rows; `prelude/1_declarations.dl7:298-300` exists | not priced |
| H. kernel names reserved | `(: intern ...)` is a diagnostic | declaration lowering, with the prelude exempt | not priced |
| I. colon path, a walk over `:` edges | `http:fetch:get` | reader, expansion, one `:` goal per segment (the walk `_2_lower/_12_units.rs:311-342` makes), checker, name table, store `nameable`, `executors_for` | 8 |

| elsewhere | spelling |
|---|---|
| SWI-Prolog | `world:done` |
| SQL | `schema.table` |
| rxjs | none; ES module imports |
| Soufflé | `myInstance1.TheAnswer(x)` |
| CozoDB | flat `*name` per database |
| Logica | `import examples.scripts.queen_victoria.Parent as RoyalParent;` |

## Receipts

| claim | path | command |
|---|---|---|
| every probe compiles as its first line says | `book/src/probes/*.dl7` | `cargo test --test _22_book probes_compile_as_their_page_says` |
| the table of spaces, spellings and other systems with sources | `plans/v8/2026-09-15-v8-namespacing.inspection.md` sections 0, 1, 3, 6 | `sed -n 1,161p ../plans/v8/2026-09-15-v8-namespacing.inspection.md` |
| currying on a qualified name needs only the lookup | inspection section 4; `oracle/compile/sources/test/fixtures/5_curry.dl7:12-15`, `src/_2_lower/_6_partial.rs:150-156` | `bash book/show.sh compile book/src/probes/22_curry_prelude_name.dl7` |
| hot module reload, needs and forks | inspection section 5 | none |
| open decisions, one row each | inspection section 7 | none |

decision: Chris
