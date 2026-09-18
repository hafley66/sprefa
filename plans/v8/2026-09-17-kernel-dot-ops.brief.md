# Kernel dot ops brief

Lane branch `feat/kernel-dot-ops`. Base `origin/main` ef3147565f5150fe6407b05844ac866a0898c6fb (PRs #789, #790 merged; the str primitive and @std/tsi are in)
First action `git merge --ff-only <sha>`;
failure = stop and report. Preset opus.

Decision rows, read first (`AGENTS.md` decisions table, grep each): "No
underscore namespacing in kernel op names", "`str.cons` is text concatenation,
two-way", "The type is the namespace for scalar ops". `str.cons` is an emitter op; the caret macro does not read it (fork C row).

## Goal

Typed kernel ops are `:` edges off their primitive node. `int_add`, `int_lt`,
`int_le`, `int_eq`, `int_ne`, `int_ge`, `int_gt`, `term_lt` retire. `str.cons`
and `str.nil` arrive, two-way like list `cons`/`nil`. The kernel identity term
is `kernel(<owner>, <label>)` for typed ops; untyped ops keep `kernel(<name>)`.

## Before / after

| before | after |
|---|---|
| `(int_add ?Value 1 ?Next)` | `(int.add ?Value 1 ?Next)` |
| `(int_lt ?A ?B)` | `(int.lt ?A ?B)` |
| `(term_lt ?A ?B)` | `(any.lt ?A ?B)` |
| absent | `(str.cons "hi " ?Name ?Greeting)` builds; `(str.cons ?Head ?Rest "hello")` splits `"h"`/`"ello"` |
| absent | `(str.nil ?Empty)` binds `""` |
| `kernel(int_add)` | `kernel(int, add)` |
| `kernel(cons)` | `kernel(cons)` unchanged |

Untyped set that stays bare: `node`, `module`, `product`, `sum`, `:`,
`edge_snapshot`, `nil`, `cons`, `edge_ref`, `intern`, `intern_snapshot`,
`effect`, `def`, `head`, `body`, `count_step`, `min_step`, `max_step`.

## Sites

| site | today | after |
|---|---|---|
| `src/_2_lower/_9_kernel.rs` `INTEGER_COMPARISONS`, `kernel_relation`, `kernel_slot_label`, `kernel_keys`, `kernel_return_positions` | keyed by bare name | keyed by `(owner: Option<&str>, label: &str)`; typed rows `("int","add")`, `("int","lt")`.., `("any","lt")`, `("str","cons")` 3 two-way, `("str","nil")` 1 |
| `src/_2_lower/_1_slots.rs:30-45` `Callable::Kernel(String)` | one atom | `Callable::Kernel { owner: Option<String>, label: String }`; `term()` builds `kernel(label)` or `kernel(owner, label)` |
| `src/_2_lower/_8_express.rs:265-283` kernel arm of `expression_callable` | bare name lookup | when `owner` is `name(_, <primitive>)` (`_7_execute.rs:255-260` builds it for a dotted head) or the primitive node itself, look up `(primitive, label)`; bare lookup only for the untyped set |
| `src/_2_lower/_8_express.rs:309` `kernel_goal` | bare | takes the pair |
| `src/_3_check/_5_kernel.rs:13-37` `COMPARISONS`, `KERNEL_RELATIONS`; `:117-160` `kernel_graph` | bare names | typed rows become `(: primitive(int) add kernel(int,add) idx)` edges on the primitive node, one per op, so the dot walk in expression position resolves; `kernel_relation_rows` emits `relation(kernel(int,add), 3, keys)` |
| `src/_3_check/_2_resolve.rs:110`, `_4_mode.rs:118` | read `kernel(name)` | read both shapes |
| `src/_6_eval/_4_kernel.rs:62-85` `Kernel::of` | matches `kernel(name)` zero-arg | matches `kernel(name)` for the untyped set and `kernel(owner, label)` for typed; new `Kernel::StrCons`, `Kernel::StrNil` |
| `src/_6_eval/_4_kernel.rs:142-159` `cons_row` | list only | `str_cons_row` beside it, same three-arg two-way shape over `Term::Str`; `str_nil_row` mirrors `nil` |
| `src/_6_eval/_4_kernel.rs:32` `LINEAR_STEPS`, `_1_program.rs:46` `("int_add", Seed::Zero)` | bare | the pair |
| `src/_5_reify/_7_sqlite.rs` | emits the bare name | emits the pair; `tests/_18_sqlite_emit.rs` expectations follow |
| `src/_9_runtime/_0_store.rs:161-170` `kernel_owned` | `kernel(_)` functor test | both arities |
| `fixtures/**/*.dl7` (18 call sites, `grep -rhoE "\((int_(add\|lt\|le\|eq\|ne\|ge\|gt)\|term_lt) " fixtures`) | bare | dotted |
| `tests/_12_term_lt.rs`, `tests/_4_check_oracle.rs`, `tests/_18_sqlite_emit.rs` | bare names in assertions | dotted / pair |
| `book/src/{2_declare,4_negation,5_terms,6_compare,7_aggregate,13_sqlite,14_diagnostics,hosting/5_citations_both_ways,modules/1_names}.md` | bare in `.dl7` blocks and tables | dotted; no new prose |
| `oracle/**` | frozen bare rows | `bash oracle/refreeze.sh`, own commit |
| `fixtures/eval/` or wherever `cons` two-way is pinned (grep `cons` in `fixtures`) | list case | sibling `str_cons` case: build, split, and `str.nil` |

NOT touched: `src/_2_lower/_2_declare.rs`, `_3_check/_0_api.rs`, `_1_graph.rs`
(keys-2 just landed there; a needed change is a stop-and-report row),
`prelude/**` (the primitive nodes and their edges come from `kernel_graph`,
not the prelude, in this lane), `macrotime/**`, `std/**`.

## Steps

| # | step | receipt |
|---|---|---|
| 1 | on base: `dl8 compile fixtures/fold/1_program_step.dl7` rc; `cargo test --test _12_term_lt --test _18_sqlite_emit` | pasted |
| 2 | `_9_kernel.rs` + `Callable` pair; `cargo build` | build tail |
| 3 | `_8_express.rs` kernel arm resolves `(int.add ?A ?B ?C)`; probe `plans/v8/probes/2026-09-17-int-dot-add.dl7` compiles rc=0 and `dl8 lower` shows `kernel(int, add)` | pasted row |
| 4 | `_5_kernel.rs` graph edges + relation rows; `_4_check_oracle` after fixture rename | PASS |
| 5 | `Kernel::of` both shapes; `str_cons_row`, `str_nil_row`; probe `plans/v8/probes/2026-09-17-str-cons.dl7` with build and split rules, `dl8 run` prints both rows | pasted rows |
| 6 | reify + runtime pair; `_18_sqlite_emit`, `_12_term_lt` green | PASS |
| 7 | fixtures 18 sites, tests, book blocks; `grep -rnE "\b(int_(add\|lt\|le\|eq\|ne\|ge\|gt)\|term_lt)\b" src fixtures tests book/src` | 0 hits, or each listed with why |
| 8 | `bash oracle/refreeze.sh`; regolden alone, stat line in the message | stat |
| 9 | full gate | summary |

Commit after every step; subject names the step.

## Gate

```bash
cargo test --no-fail-fast 2>&1 | grep -E "^test result|FAILED|panicked"
cargo test --test _4_check_oracle --test _0_eval_oracle --test _12_term_lt --test _18_sqlite_emit --test _22_book 2>&1 | grep -E "^test |test result"
grep -rnE "\b(int_(add|lt|le|eq|ne|ge|gt)|term_lt)\b" src fixtures tests book/src | wc -l
git diff --stat origin/main...HEAD
```

Known red on the base: `.github/CI-KNOWN-RED.md` dl8 battery section (6
nested-worktree legs). `_22_book::chapter_executors` is slow under load; measure
alone three times before calling it red.

## Laws

`plans/v8/2026-09-17-import-arc.brief.md` section 8. No long-form markdown;
PR body = steps table with receipts, under 20 lines of prose. A design
question the decision rows do not answer (example: whether `float` gets
`float.add` now) is a stop-and-report row, not a guess; `float` ops are out of
scope unless a fixture already needs one.

## Report

```bash
boop beep --no-wait --as feat-kernel-dot-ops sprefa-coordinator "kernel dot ops: PR #<n>, underscore hits <n>, battery <pass>/<total>, red: <list or none>"
```
