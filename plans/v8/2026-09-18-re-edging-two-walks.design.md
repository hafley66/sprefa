# Re-edging: two walks over one `:` table

Caret 3 design page. Rows, trees, traces. No mermaid. Sites cite today's code.

## TOC

- [Source and expansion](#source-and-expansion)
- [Rows](#rows)
- [Tree](#tree)
- [Two walks, one probe](#two-walks-one-probe)
- [Traces](#traces)
- [Re-edging: nothing overwrites](#re-edging-nothing-overwrites)
- [`key` decision](#key-decision)
- [Forks for Chris](#forks-for-chris)

## Source and expansion

```
(^User: (* (: age int))): (* (: greet (* (: user User) (: out str)))
                        (<- (greet ?Who ?Out) (User ?Who ?Name) (str.cons "hi " ?Name ?Out)))
(: User () Mixin)
(: Mixin (* (: greet (* (: user User) (: out str)))))
```

`dl8 expand` (macro emits two rows, never looks inside the body product):

```
(: User (* (: age int)))
(: User () (* (: greet (* ...)) (<- ...)))
```

## Rows

`(: owner name target index)`, keys `(owner, name)` and `(owner, index)`
(`src/_2_lower/_9_kernel.rs:36-39,66`). `()` label mints name = index.

| owner | name | target | index | what |
|---|---|---|---|---|
| module | User | user_product | 0 | module edge: the binding |
| user_product | age | int | 0 | column, arity counts it |
| user_product | 1 | user_body | 1 | anonymous, product-valued: namespace |
| user_body | greet | greet_sig | 0 | own label of the namespace |
| user_body | 1 | greet_rule | 1 | the rule, anonymous member of user_body |
| greet_sig | user | user_product | 0 | column |
| greet_sig | out | str | 1 | column |
| user_product | 2 | Mixin | 2 | second namespace edge |
| Mixin | greet | mixin_greet | 0 | shadows user_body.greet on the walk |

Columns take indices in source order; the `^` body takes the next index.
No reserved slot; "namespace is edge 1" holds because `age` is the one column.

## Tree

```
module
└─ User ─> user_product          module edge 0: the binding
   ├─ age ─> int                 user_product edge 0, column
   ├─ 1 ─> user_body             user_product edge 1, anonymous, walked
   │   ├─ greet ─> greet_sig     user_body edge 0
   │   │   ├─ user ─> user_product
   │   │   └─ out ─> str
   │   └─ 1 ─> greet_rule        user_body edge 1, rule, anonymous
   └─ 2 ─> Mixin                 user_product edge 2, anonymous, walked after user_body
       └─ greet ─> mixin_greet   shadowed_member(user_product, greet)
```

## Two walks, one probe

`scoped()` at `src/_2_lower/_10_index.rs:170-193` is the `.closest()` walk
today: probe `(owner, name)`, then `owner = parent(owner)`, cycle-guarded.
Caret 3 adds the second direction with the same probe.

| walk | direction | step relation | today | caller |
|---|---|---|---|---|
| scope (`.closest()`) | up | `parent(owner)`: the owner of the edge whose target is `owner` (`_3_check/_1_graph.rs:55`, `_10_index.rs:191`) | built | free atom in a rule body or expression (`_8_express.rs:127`) |
| prototype (Ruby ancestors, JS `__proto__`) | down | anonymous edges of `owner` whose target is a product, index order, depth-first | not built | dot segment `User.greet` (`_8_express.rs:105`), dotted head `(User.greet ..)` (`_7_execute.rs:270`), and each stop of the up walk |

One probe, two `next` relations. Not a macro: a macro cannot see rows that
arrive from another unit, an import, or a comptime round.

## Traces

`User.greet` in an expression:

```
step 0  owner=user_product  probe (user_product, greet)   miss
step 1  proto(user_product) = [user_body, Mixin]          index order
step 2  owner=user_body     probe (user_body, greet)      hit greet_sig   value = greet_sig, stop (first wins)
diag    (Mixin, greet) also on the walk                   shadowed_member(user_product, greet), prelude rule
```

`(User ?Who ?Name)` inside greet_rule's body, free atom `User`:

```
step 0  owner=greet_rule    probe (greet_rule, User)      miss   proto(greet_rule) = []
step 1  owner=user_body     probe (user_body, User)       miss   proto(user_body) = [greet_rule] (rule: skipped, fork 1)
step 2  owner=user_product  probe (user_product, User)    miss   proto(user_product) = [user_body visited, Mixin] miss
step 3  owner=module        probe (module, User)          hit user_product   steady state: the module binding
```

`(greet ?Who ?Out)` as greet_rule's head: step 0 greet_rule miss, step 1 user_body hit greet_sig. The rule
releases rows into greet_sig. `AGENTS.md:92` holds: greet_rule has no name, `1` is an index.

## Re-edging: nothing overwrites

| form | row | key `(owner, name)` | effect |
|---|---|---|---|
| `(: User email str)` | user_product email str 3 | fresh | new column; arity 2 |
| `(: User age str)` | user_product age str 3 | collides with row `user_product age int 0` | key-1 collision; diagnostic name unverified (no `duplicate_*` in `src/_2_lower`, `src/_3_check`, `prelude/`) |
| `(: User () Extra)` | user_product 3 Extra 3 | fresh (name = index) | walk order extended; no row replaced |
| `(: User greet own_greet)` | user_product greet own_greet 3 | fresh on user_product | own label wins over user_body.greet at step 0; `shadowed_member(user_product, greet)` |

Arity of user_product = edges whose target is a type or value node = 1 (`age`).
Anonymous product-valued edges are skipped (`_3_check/_1_graph.rs:60`).

## `key` decision

`Key` (interned constructor `keyed_edge` reads at
`prelude/3_derived_rules.dl7:61-63`) becomes `dl6.key`, declared in
`std/dl6.dl7`. The rules move with it: `keyed_edge`, `key_predecessor`,
`key_has_predecessor`, `key_rank`, `composite_key`
(`prelude/3_derived_rules.dl7:61-87`, decls `prelude/1_declarations.dl7:75-100`),
`program_key`, `program_key_position` (`prelude/1_declarations.dl7:255-262`).
The `:` row's own two key sets stay: a separate uniqueness concept, not an
emitter annotation. Own lane after caret 3.

## Forks for Chris

| # | fork | options |
|---|---|---|
| 1 | proto step into a rule-valued anonymous edge | skip rules (`not (rule ?T)`), or walk them (exposes `head`/`body` labels) |
| 2 | shadow policy | diagnostic only, first wins (proposed); or check error |
| 3 | up walk probes each stop's proto chain (Ruby: lexical, then ancestors) | yes (trace above); or up walk is own labels only |
| 4 | `User.1` and `User.proto` | plain label lookups, no int-segment arm (proposed) |

## Addendum, effect type and Result (user 2026-09-18)

- `Result` is a function on types: two type inputs, one type output, the output a sum whose branches name the inputs.

```
(: Result
   (* (: ok type)
      (: error type)
      (: return
         (+ (: ok ok)
            (: error error)))))
```

- `(Result Body str)` interns a node where the return sum's branches resolve with `ok = Body`, `error = str`. That substitution at intern is the generics work (`docs/generics-wrapper-inspection.md`); the caret-3 scope walk is how a branch target finds its parameter column.
- Effect: `(: json (effect (* (: url str)) (Result type str)))`. Input product row = request; output sum variant = answer. Served by rows; `--serve`, `_error` products, Rust cadence retire.
- AGENTS.md is edited by Chris only. The 2026-09-18 rows there that describe `Result` are superseded by this addendum until he edits them.

## Addendum, namespaces and userland effects (user 2026-09-18)

- `gh.` is GitHub, `git.` is git, `http.` is requests. Never `github.`.
- A userland effect is a view over hosted effects: declared with the same `(effect Input Output)` form, its request rows forwarded down by rules and its answer rows derived up by rules, no executor. `@std/gh` is the first one: `gh.repos`, `gh.prs`, `gh.issues` over `http.json` against `api.github.com`; pure dl7.
- `str.cons` stays arity 3. Variadic `str.cons` and `"{?x}"` interpolation are macrotime, unbuilt until the pain is earned.

## Addendum, one store (user 2026-09-18)

- Comptime rounds run on sqlite_ivm. The db is the compile artifact: edges, interns, effect answers, rules, names are tables. `dl8 eval app.db` resumes it; no re-lower, no refetch of answered requests. Compiler and runtime are one loop over one store at different times.
- Retires: the `Compile` JSON `program` object and `program_from_json`; the in-memory comptime `Store`; `RoundState`'s `Vec` freezes. Reload stays retract-relower-rederive, now on the shipped db.
- The `@std/dl6` emitter writes into this store; `fs.write` is for text targets (`@std/cli`) only.
- Executor verb: the boundary crossing is named by Chris from `handle` / `cross` / `admit`; `poll` stays.
- Lane order: list literal, effect type, comptime on sqlite_ivm, oai-rules, `@std/gh`, `@std/cli`.
