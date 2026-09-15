# v8 namespacing inspection

0. [One shape: kernel, served, declared](#0-one-shape-kernel-served-declared)
1. [Name spaces that exist today](#1-name-spaces-that-exist-today)
2. [Collision probes](#2-collision-probes)
3. [Candidate spellings for a qualified name](#3-candidate-spellings-for-a-qualified-name)
4. [Currying on a qualified name](#4-currying-on-a-qualified-name)
5. [Hot module reload](#5-hot-module-reload)
6. [What other systems do](#6-what-other-systems-do)
7. [Open decisions](#7-open-decisions)

Paths are from `v8/` unless they start with `plans/` or `hafley-rs/`. Probe programs live at `v8/book/src/probes/`, each compiled by `cargo test --test _22_book probes_compile_as_their_page_says`. Outputs come from `bash book/show.sh` with `DL8` at the release binary, base `256a140ed`.

## 0. One shape: kernel, served, declared

A kernel relation is a relation hosted by the evaluator; a served relation is a relation hosted by an executor; a declared relation is hosted by rules and facts. All three are one shape: a relation id with columns. Past the compiler, every one is a `TermId`; a name string meets a relation id in two places only.

| where | what it does | line |
|---|---|---|
| kernel relations are minted, never declared | `ref(kernel(Name))` from the kernel table; no `prelude/*.dl7` file declares one (the grep for a kernel name after `(: ` over `prelude/` prints 0 lines) | `src/_2_lower/_1_slots.rs:36-42`, `src/_6_eval/_4_kernel.rs:24-28`, table `src/_3_check/_5_kernel.rs:16-37` |
| served set is relation ids | `Program.served: HashSet<TermId>` | `src/_6_eval/_1_program.rs:171` |
| the effect row keys on relation id | `served.contains(&goal.rel)` then `write_effect(goal.rel, ...)` | `src/_6_eval/_5_evaluate.rs:244-249` |
| name to id, crossing 1 | `program_names`: the compile JSON `names` map, source name to relation ref; `serve_relations` and `Reconciler::new` read this map | `src/_6_eval/_6_json.rs:333-343`, `:346-369`; `src/bin/dl8.rs:393-396` |
| name to id, crossing 2 | `executors_for`: the `--serve` string matched to `timer::RELATION` and the others, one arm each | `src/_9_runtime/_3_executors/mod.rs:61-100` |

A qualified name therefore touches the source lookup (sections 1 and 3) and these two crossings; nothing between them reads a name.

## 1. Name spaces that exist today

```mermaid
flowchart LR
  F[product field scope] --> M[file module edges]
  M --> P[prelude alias edges]
  P --> K[kernel: hosted by the evaluator]
  K --> R[primitive names]
  M --> T["program_names: name to rel id"]
  P --> T
  T --> S["executors_for: --serve string to arm"]
  S --> X[served: hosted by an executor]
```

Compile-time lookup walks left to right; `program_names` and `executors_for` are the two places a name string meets a relation id.

| space | members | written at | read at | scope |
|---|---|---|---|---|
| product field scope | edges owned by a product, nested products | the declaration graph | `src/_2_lower/_10_index.rs:170-193` (`scoped`, owner then parent); `src/_3_check/_2_resolve.rs:70-77` | innermost product first |
| file module | top-level declarations of one file, owner `module(file(Path))` | `src/_4_comptime/_0_load/_2_project.rs:270-273` | same two resolvers | one file |
| directory module | one edge per file or subdirectory, labeled by module name, owner `module(directory(Path))` | `_2_project.rs:195-247`, name rule `:162-193` | `_2_resolve.rs:73-77` through parents | one project root |
| prelude | `prelude/*.dl7`, owner `module(prelude)` | `src/_8_driver/_0_read.rs:7-27` | aliased into each unit that does not bind the name, `src/_2_lower/_12_units.rs:310-342`, exporter `:475-492` | every unit |
| kernel names | `node` .. `int_ne`, plus `def`, `head`, `body` | `src/_2_lower/_9_kernel.rs:15-28`; `src/_3_check/_5_kernel.rs:16-37`, `:83` | `src/_2_lower/_8_express.rs:181-184` after scoped lookup; `_2_resolve.rs:78-83` at a module owner | global, after every edge |
| primitive names | `int`, `float`, `bool`, `text`, `any`, `type` | `src/_3_check/_5_kernel.rs:40-42` | `_2_resolve.rs:84-87` | global, last |
| runtime name table | every `(: module(_) Name ref(_) _)` edge of the root graph, prelude yields to non-prelude, two non-prelude refs stop | `src/_6_eval/_6_json.rs:178-221` | `serve_relations` `_6_json.rs:346-369`; `executors_for` `src/_9_runtime/_3_executors/mod.rs:46-100`; `store.name_relations` `src/bin/dl8.rs:316`, `:391`; `emit_sqlite` `src/bin/dl8.rs:231` | one program, flat |
| served executor names | string constants: `timer`, `fetch_json`, `fetch_json_error`, `soopy_refs`, `soopy_refs_error`, `soopy_history`, `soopy_history_error`, `repo_at`, `repo_at_error`, `extract`, `extract_error` | `_3_executors/timer.rs:12`, `fetch_json.rs:9-10`, `soopy_refs.rs:12-13`, `soopy_history.rs:10-11`, `repo_at.rs:10-11`, `extract.rs:14-15` | `_3_executors/mod.rs:61-100` | the binary |
| store table names | runtime table names passing `nameable` (letters, digits, `_`, `.`) | `src/_9_runtime/_0_store.rs:122-129` | `src/_9_runtime/_1_sqlite.rs:497-504` | one db file |
| prelude host declarations | `Hosted(relation, implementation)`, `HostPort(relation, label, direction)` | `prelude/1_declarations.dl7:292-305` | no reader in `src/` (`grep -rn Hosted src` prints nothing) | every unit |
| reader atoms | identifier chars include `-` and `.`; `/` and `::` are not atoms | `src/_0_read/_1_tokens.rs:16-38` | `src/_0_read/_2_reader.rs:397-402` | every file |

## 2. Collision probes

| probe | command | rc | output |
|---|---|---|---|
| user product named `intern` | `bash book/show.sh eval book/src/probes/3_user_intern.dl7` | 0 | `(intern "mine")` |
| user `intern` beside a rule calling the kernel `intern` | `bash book/show.sh compile book/src/probes/4_user_intern_kernel_goal.dl7` | 1 | `diagnostic diagnostic(lower, reader_node(book/src/probes/4_user_intern_kernel_goal.dl7, 36), arity_mismatch(intern, 1, 3))` |
| user `Option` without a return column | `bash book/show.sh compile book/src/probes/5_user_option_shadows.dl7` | 1 | `expression_without_return(target(owner(file(book/src/probes/5_user_option_shadows.dl7), reader_node(book/src/probes/5_user_option_shadows.dl7, 3))))` |
| field typed `bool` | `$DL8 compile book/src/probes/17_bool_field.dl7` then the edge renderer in `book/src/modules/1_names.md` | 0 | `(: Holder flag bool)` (prelude product, `prelude/5_tsi_primitives.dl7:55`); `(: Holder count primitive(int))` |
| user product named `timer`, read by a rule, no `--serve` | `bash book/show.sh run book/src/probes/9_user_timer_rule.dl7 --max-ticks 2` | 0 | `(Want "mine")`, `ticks 0` |
| same, `--serve timer` | `bash book/show.sh run book/src/probes/9_user_timer_rule.dl7 --serve timer --max-ticks 2` | 0 | `(effect timer ref(application(timer, ["mine"])))`, `(Want "mine")`, `ticks 0`: the Rust timer binds to the user's one-column `timer` and skips the text value (`_3_executors/timer.rs:140-148`) |
| two modules declaring `User` | `bash book/show.sh compile book/src/probes/6_user_a.dl7 --project book/src/probes book/src/probes/7_user_b.dl7` | 1 | `diagnostic duplicate_relation_name(User, ref(owner(file(book/src/probes/6_user_a.dl7), reader_node(book/src/probes/6_user_a.dl7, 3))), ref(owner(file(book/src/probes/7_user_b.dl7), reader_node(book/src/probes/7_user_b.dl7, 3))))` |
| a third module reading `(: user_a User ?T ?I)` | `bash book/show.sh compile book/src/probes/8_pick_user_a.dl7 --project book/src/probes book/src/probes/6_user_a.dl7 book/src/probes/7_user_b.dl7` | 1 | the goal resolves to `6_user_a.dl7`'s `User`: `(ref(owner(file(book/src/probes/8_pick_user_a.dl7), reader_node(book/src/probes/8_pick_user_a.dl7, 3))) ref(owner(file(book/src/probes/6_user_a.dl7), reader_node(book/src/probes/6_user_a.dl7, 3))))`; then the same `duplicate_relation_name(User, ...)` |
| a relation named like a module | `bash book/show.sh compile book/src/probes/12_relation_named_accounts.dl7 --project book/src/probes book/src/probes/10_accounts.dl7` | 1 | `diagnostic duplicate_relation_name(accounts, ref(module(file(book/src/probes/10_accounts.dl7))), ref(owner(file(book/src/probes/12_relation_named_accounts.dl7), reader_node(book/src/probes/12_relation_named_accounts.dl7, 3))))` |
| bare `User` from another user module | `bash book/show.sh compile book/src/probes/18_bare_consumer.dl7 --project book/src/probes book/src/probes/10_accounts.dl7` | 1 | `diagnostic diagnostic(lower, reader_node(book/src/probes/18_bare_consumer.dl7, 14), undeclared_relation(User))` |
| dotted relation name | `bash book/show.sh eval book/src/probes/19_dotted_name.dl7` | 0 | `(boop.lane "sprefa-coordinator")` |
| dotted name served | `bash book/show.sh run book/src/probes/19_dotted_name.dl7 --serve boop.lane` | 1 | `diagnostic served_relation_no_executor(boop.lane)` |
| `/`, `:` and `::` inside a name | `printf '(: accounts/User\n   (* (: id int)))\n' > /tmp/ns.dl7 && bash book/show.sh compile /tmp/ns.dl7` (and `accounts:User`, `accounts::User`) | 1 | `invalid_atom(accounts/User)`, `invalid_atom(accounts:User)`, `invalid_atom(accounts::User)` |

## 3. Candidate spellings for a qualified name

Files a spelling touches: reader `src/_0_read/_1_tokens.rs`, `_2_reader.rs`; lowerer `src/_2_lower/_8_express.rs:155-186`, `_10_index.rs:170-193`, `_12_units.rs:310-342`; checker `src/_3_check/_2_resolve.rs:47-90`; name table `src/_6_eval/_6_json.rs:178-221`; executors `src/_9_runtime/_3_executors/mod.rs:46-100`; prelude `prelude/*.dl7`. "Pinning fixture" names a file that does not exist yet.

| spelling | example | reader | lowerer | checker | name table | executors_for | prelude | pinning fixture |
|---|---|---|---|---|---|---|---|---|
| A. prefix by convention (today) | `boop_lane`, `soopy_refs_error` | none | none | none | none | one arm per name | none | exists: `fixtures/hosts/0_refs.dl7` |
| B. dotted atom, flat | `boop.lane` | none, already an atom (`_1_tokens.rs:17`) | none | none | none, key is the whole text | one arm per dotted name | none | `book/src/probes/19_dotted_name.dl7` compiles today; a served one needs `fixtures/hosts/5_dotted_serve.dl7` |
| C. dotted atom, split to module plus name | `accounts.User` resolves the edge `User` of module `accounts` | none | split before `scoped` | split before `resolve_name` | key by owner module and name | match on the last segment, or the whole text | a dotted prelude name such as `prelude.Option` needs the prelude to be a named module | `oracle/compile/sources/test/fixtures/modules/3_dotted_consumer.dl7` |
| D. graph goal only, table keyed by owner | `(: accounts User ?T ?I)` in source (exists, `modules/1_consumer.dl7:5`); `--serve accounts/User` on the CLI | none | none | none | key by `(module, name)`, stop on duplicate only inside one module | read `module/name` | none | `book/src/probes/8_pick_user_a.dl7` flips from `duplicate_relation_name` to rc=0 |
| E. Prolog-style colon | `accounts:User` | new token; conflicts with the label suffix `name:` (`_1_tokens.rs:32-36`) and infix `(label: T)` (`src/_0_read/_4_expand.rs:163-185`) | new goal form | new name term | key by module | read `module:name` | none | `oracle/compile/sources/test/fixtures/modules/3_colon_consumer.dl7` |
| F. import form | `(use accounts User)` or `(use boop)` | none, an ordinary form | `_12_units.rs:310-342` generalized: a user module becomes an exporter for the named importer | none | still flat unless combined with D | none | none | `oracle/compile/sources/test/fixtures/modules/3_use_consumer.dl7` |
| G. binding through `Hosted` rows | `(Hosted boop_lane BoopLaneExecutor)` | none | none | none | none | read `Hosted` rows instead of string arms | `prelude/1_declarations.dl7:298-300` already declares it | `fixtures/hosts/5_hosted_binding.dl7` |
| I. colon path, a walk over `:` edges | `http:fetch:get` | new token: `valid_atom` accepts one trailing `:` only (`_1_tokens.rs:28-38`); conflicts with infix `(label: T)` (`src/_0_read/_4_expand.rs:163-185`) | desugar to one `:` goal per segment, the goal `_7_execute.rs:203-205` already lowers; the same walk `install_module_aliases` makes over exporter edges (`_12_units.rs:311-342`) | none in goal position; `resolve_name` walks segments in type position | key by path | read the path | none | `book/src/probes/23_colon_path_walk.dl7` spells the walk as goals today: `(found get)`, rc=0 |
| H. kernel names reserved | a user `(: intern ...)` becomes a diagnostic | none | a reserved-name check in declaration lowering | none | none | none | the prelude writes rules with kernel heads (`prelude/3_derived_rules.dl7` heads `:`, `node`, `product`, `def`, `head`, `body`), so the check would exempt `module(prelude)` | `book/src/probes/3_user_intern.dl7` flips to a diagnostic |

Price by files touched, counting `v8/src` files a spelling must edit. Probes run today: `http:fetch:get` as one atom is `invalid_atom(http:fetch:get)`; `boop.lane` is one atom, rc=0.

| spelling | files | count |
|---|---|---|
| B. dotted atom, flat | `mod.rs` (one arm) | 1 |
| C. dotted atom, split | `_2_lower/_8_express.rs`, `_2_lower/_10_index.rs`, `_3_check/_2_resolve.rs`, `_6_eval/_6_json.rs`, `_3_executors/mod.rs`; reader none (`_1_tokens.rs:17` accepts `.`), store none (`_9_runtime/_0_store.rs:123-129` accepts `.`) | 5 |
| I. colon path | `_0_read/_1_tokens.rs`, `_0_read/_4_expand.rs`, `_2_lower/_7_execute.rs`, `_2_lower/_8_express.rs`, `_3_check/_2_resolve.rs`, `_6_eval/_6_json.rs`, `_3_executors/mod.rs`; store: `nameable` rejects `:` (`_0_store.rs:128`) so `_9_runtime/_0_store.rs` too | 8 |
| D. graph goal, table keyed by module | `_6_eval/_6_json.rs`, `_3_executors/mod.rs`, `bin/dl8.rs` (`--serve` text) | 3 |

## 4. Currying on a qualified name

| step | what happens | line |
|---|---|---|
| a named bind with fewer arguments | `(: PairUser (Pair User))` then `(PairUser Order PairResult)` | `oracle/compile/sources/test/fixtures/5_curry.dl7:12-15` |
| a relation returns a callable | `(CallableFactory User Pair)`, `(: ReturnedPair (CallableFactory User))` | `5_curry.dl7:31-41` |
| the bind's callable is found by name | `cx.reservations.scoped(owner, atom)`, the same lookup a goal head uses | `src/_2_lower/_6_partial.rs:150-156`; `src/_2_lower/_10_index.rs:170-193` |
| the curried identity is an ordinary prelude relation with a `return` column | `Curry(source, bindings, return)`; `PairUser` names `ref(application(Curry, [Pair, [bound(0, reference, User)]]))` | `prelude/1_declarations.dl7:164-167`; `dl8 compile` of `5_curry.dl7:1-15`, `.program.names.PairUser` |
| a name from another module curries | `(: KeyName (Key "name"))`, `Key` resolved through the prelude alias; `KeyName` keeps `options` and `return` | `book/src/probes/22_curry_prelude_name.dl7`, rc=0 |
| a field type does not curry | `(partial : (Pair int))` | `partial_application_requires_more_arguments`, `_6_partial.rs:35-55`, `book/src/probes/21_partial_in_field.dl7` |

A curried call on a qualified name needs the qualified lookup to return the same `target(Owner)` `scoped` returns; the `Curry` rows and the partial rules read the target, never the name.

## 5. Hot module reload

Reloading one module while `dl8 run` stays up.

| need | exists | gap | file |
|---|---|---|---|
| a unit boundary per module | one unit per path (`src/_8_driver/_4_project.rs:28-56`); `Units { basements, origins, diagnostics }` per lowering (`src/_2_lower/_12_units.rs:14-18`) | lowering takes every unit at once (`_12_units.rs:476-500`); no entry point re-lowers one unit | `src/_2_lower/_12_units.rs` |
| a trigger when a file changes | `DirectoryWatcher`, `SourceWatcher` in soopy (`hafley-rs/crates/soopy/src/_8_watch.rs:28`, `:357`) | dl8 opens neither (`grep -rn 'SourceWatcher\|DirectoryWatcher' v8/src` prints nothing) | `src/_9_runtime/_3_executors/`, `src/bin/dl8.rs` |
| re-lowering against the rest of the program | compiler rounds re-enter last round's edges as `edge_snapshot` and intern requests as `intern_snapshot` | the rounds run inside one compile; the run loop holds the compiled `Program` only | `src/_4_comptime/_2_rounds.rs:281-295` |
| retraction of the old unit's rows | sqlite_ivm deletes: delete-and-rederive over a member set (`sqlite_ivm/src/1a_relational.rs:725-726`, delete round `:844-846`); managed result tables reject direct writes (`sqlite_ivm/src/2_vtab.rs:380-385`) | evaluator tables are append-only | `src/_6_eval/_3_table.rs:1-3` |
| arena ids stay valid | the store appends arena terms above a watermark (`src/_9_runtime/_0_store.rs:149-150`) | `load_arena` needs a fresh `Universe` (`_0_store.rs:139-140`); a reload inside one process keeps the old one | `src/_9_runtime/_0_store.rs` |
| executors survive the swap | executors are built once from the name map (`src/bin/dl8.rs:393-396`); `effects_seen` is a position in the effect table (`src/_9_runtime/_2_reconcile.rs:44-47`) | a changed name map or a retracted effect row has no path back into `Reconciler` | `src/_9_runtime/_2_reconcile.rs` |
| the loop accepts a new program | `Reconciler::run(u, program, rows, store, max_ticks, fx)` | `program` is borrowed for the whole run (`_2_reconcile.rs:89-97`) | `src/_9_runtime/_2_reconcile.rs` |

Forks, no decision:

| fork | options |
|---|---|
| where retraction comes from | evaluator tables gain deletes; or the runtime moves onto sqlite_ivm arrangements |
| reload unit | one file module; or the whole project re-lowered with a digest early-out per unit |
| executor state on reload | keep executors whose served id survives; or rebuild all from the new name map |

## 6. What other systems do

| system | qualified name | source |
|---|---|---|
| SWI-Prolog modules | `Module:Goal` calls a predicate in that module whether or not it is exported; `use_module/2` imports exported predicates | `https://www.swi-prolog.org/pldoc/man?section=overrule` ("?- world:done. % calls done/0 in module world") |
| SQL | `schema.table`; SQLite names an attached database the same way, `aux.users` | `https://www.sqlite.org/lang_attach.html` |
| rxjs | no names of its own; ES module `import { x } from` scopes identifiers | the language, not the library |
| Soufflé components | `.init myInstance1 = MyComponent`, then `myInstance1.TheAnswer(x)`: "The qualified names of component elements are prepended using the name of the instantiation" | `https://souffle-lang.github.io/components` |
| CozoDB | stored relations are one flat space per database read as `*name`; `_name` marks an ephemeral relation, `r:idx` names an index of `r`, `::` prefixes system operations | `https://docs.cozodb.org/en/latest/stored.html` |
| Logica | `import examples.scripts.queen_victoria.Parent as RoyalParent;` imports one predicate by dotted file path, with an optional alias | `github.com/EvgSkv/logica` at `8ca7a3e`, `examples/scripts/closure_use.l:14` |

## 7. Open decisions

| decision | rows it selects among |
|---|---|
| does a user declaration take a kernel name, or is that a diagnostic | section 3, A versus H |
| does a user declaration shadow a prelude name silently (today: yes, `5_user_option_shadows.dl7`, `17_bool_field.dl7`) | section 2 |
| is the runtime name table flat or keyed by module | section 3, B versus C and D |
| which text qualifies a name in source: dot, colon path, graph goal, or an import form | section 3, B to F and I |
| does a partial application become a field type, beside the named bind | section 4 |
| how a module reloads while `dl8 run` stays up | section 5 forks |
| how a served name reaches an executor: a string arm or a `Hosted` row | section 3, A versus G |
| does a served relation's declared shape get checked against its executor's columns (today: no, `9_user_timer_rule.dl7`) | section 2 |
| does one module see another user module's names without a qualifier (today: no, `18_bare_consumer.dl7`) | section 3, F |

decision: Chris
