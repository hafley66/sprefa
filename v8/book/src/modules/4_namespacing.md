# Namespacing

[Where names live](#where-names-live) · [Collisions](#collisions) · [Forks](#forks) · [Receipts](#receipts)

The full inspection, with every probe's pasted output: `plans/v8/2026-09-15-v8-namespacing.inspection.md`.

## Where names live

| space | example | lookup order | read at |
|---|---|---|---|
| product field scope | `Name` inside `Holder` | first | `src/_2_lower/_10_index.rs:170-193`, `src/_3_check/_2_resolve.rs:70-77` |
| file module | `User` in `10_accounts.dl7` | after the fields | same |
| directory module | the edge `accounts` on the project root | through parents | `src/_4_comptime/_0_load/_2_project.rs:195-247` |
| prelude alias | `Option`, `bool` | after the file, only when the file does not bind the name | `src/_2_lower/_12_units.rs:310-342` |
| kernel | `intern`, `effect` | after every edge | `src/_2_lower/_8_express.rs:181-184`, `_2_resolve.rs:78-83` |
| primitive | `int`, `text` | last | `_2_resolve.rs:84-87` |
| runtime name table | every top-level name of every module, one flat map | not ordered: prelude yields, two user names stop | `src/_6_eval/_6_json.rs:178-221` |
| served name | `timer`, `fetch_json_error` | a string match on the flat map | `src/_9_runtime/_3_executors/mod.rs:61-100` |

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
| dotted name | `probes/19_dotted_name.dl7` | rc=0, one atom `boop.lane` |
| `accounts/User`, `accounts:User`, `accounts::User` | inspection section 2 | `invalid_atom` |

## Forks

| spelling | example | what it touches |
|---|---|---|
| A. prefix by convention, today | `boop_lane` | one `executors_for` arm per name |
| B. dotted atom, flat | `boop.lane` | one `executors_for` arm; the reader and the name table accept it today |
| C. dotted atom split into module and name | `accounts.User` | lowerer and checker lookups, name table key, `executors_for`, a named prelude module |
| D. graph goal in source, table keyed by module | `(: accounts User ?T ?I)`, `--serve accounts/User` | name table key, `serve_relations`, `executors_for` |
| E. Prolog colon | `accounts:User` | reader (conflicts with `label:`), lowerer, checker, name table, `executors_for` |
| F. import form | `(use accounts User)` | the alias installer: a user module as exporter |
| G. `Hosted` rows bind an executor | `(Hosted boop_lane BoopLaneExecutor)` | `executors_for` reads rows; `prelude/1_declarations.dl7:298-300` exists |
| H. kernel names reserved | `(: intern ...)` is a diagnostic | declaration lowering, with the prelude exempt |

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
| the table of spaces, spellings and other systems with sources | `plans/v8/2026-09-15-v8-namespacing.inspection.md` sections 1, 3, 4 | `sed -n 1,97p ../plans/v8/2026-09-15-v8-namespacing.inspection.md` |
| open decisions, one row each | inspection section 5 | none |

decision: Chris
