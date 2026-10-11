# Per-language callable-kind coverage for `call_def`

The `call_def` relation is the callable registry: one row per callable the
front-end can see — named *or anonymous*. This page audits which *kinds* of
callable each language extractor emits, in two tiers:

- **AST-tier** — does the diet (syntactic) front-end in `src/graph/typegraph/`
  emit a `call_def` row for this kind?
- **scip-tier** — can that row be paired with a compiler-truth scip symbol
  (`scip_occurrence`/`scip_def`) by a coordinate join (file + line + name)? The
  `scip_line_callee` pattern in `examples/loop-nests.dl` is the precedent. Values:
  `pairable` / `scip-does-not-index-this` / `unverified`.

**This page is checked by a rail.** `examples/callable-coverage.dl` joins the
`// @callable <lang> <kind>` markers stamped on every emitter arm against the
`call_def` rows extracted from the fixtures under `tests/fixtures/callables/`.
Every marked `(lang, kind)` must have a matching fixture row and vice versa
(`callable-coverage` error otherwise). The rail + fixtures are the living source
of truth; this doc is the human-readable projection.

Citations below are `file fn-name` anchors (2026-07-18 decomposition-
normalization step 3): `typegraph.rs` split into `src/graph/typegraph/`
per-language modules, and raw line numbers drift with every edit — a
function name survives a refactor a line number doesn't.

The emitters live across `src/graph/typegraph/`, split per language:

- Rust (`src/graph/typegraph/rust/mod.rs`): `rust_call_defs_from` + the
  `RustCallDefs` syn visitor (free/nested fns, impl/trait methods, closures)
- TypeScript / JavaScript (`src/graph/typegraph/ts/mod.rs`):
  `ts_call_defs_from` / `ts_fn_call_def` / `ts_class_call_defs` /
  `ts_var_call_defs` + `ts_push_lambda_defs` (unbound lambdas, derived from
  the df closure nodes), `TsNestedFnDefs` (nested fn visitor), `TsCallSites`
  (call-site visitor incl. `new` constructor calls)
- Kotlin (`src/graph/typegraph/kotlin.rs`): `kt_walk_call_defs` (fns,
  primary/secondary ctors, lambda literals)
- Go (`src/graph/typegraph/go.rs`): `go_walk_call_defs` (fns, methods, func
  literals)
- Python (`src/graph/typegraph/python.rs`): `py_call_defs_from` /
  `py_walk_call_defs` (fns, methods, `__init__`, lambdas)
- C: no extractor (see C status below)

Shared helpers live in `src/graph/typegraph/mod.rs`: `mint_sym` (sym minting
for named callables) and `lambda_sym` (anonymous-callable sym scheme, shared
with the dataflow lift).

## Model + vocabulary

`EntityKind` gains a `Lambda` variant (tag `"lambda"`, in `is_callable`) for the
anonymous-callable sym identity. `CallKind`'s unused `Closure` variant is renamed
`Lambda` (tag `"lambda"`) — so `call_def.kind` reads `"lambda"`, matching the
`EntityKind` tag and the `@callable … lambda` markers. **One vocabulary word.**

The dataflow `df_node.kind = "closure"` value node keeps its own word on purpose:
it is a *different* relation and concept — the closure-as-value in the enclosing
scope, not the callable definition — so `closure` (the value) and `lambda` (the
def) never collide.

### Sym scheme for anonymous callables

An anonymous callable's sym is `lambda_sym(enclosing_fn_sym, coord)` =
`{enclosing}::closure::{coord}` — the **exact string the dataflow lift already
mints** as a closure's `lam_sym` (shared helper `lambda_sym`, called by both the
df lift and the call_def emitters). `coord` is the language's stable node
coordinate: `<row>_<col>` for tree-sitter front-ends (Kotlin/Go/Python),
the byte offset for oxc (TS/JS), `<line>_<col>` for syn (Rust). Consequences:

- deterministic (coordinate-derived — no counters, satisfies the determinism law
  that fixed the exe-swap write storm, commit `80617b6b`);
- `call_def.sym == df_node.fn` for the lifted lambda body and
  `call_def.sym == df_node.var` for the closure value node are **exact joins**
  (modulo the uniform `repo::` prefix every `call_def` sym carries and `df_node`
  does not) — the whole point: taint flows through a callback because the callback
  is a first-class callable whose body df joins its call_def by sym.

Constructors reuse the df/type walker's own ctor sym so the same join holds: TS
`mint_sym(Method, "constructor", Some(class))`; Python `__init__` as an ordinary
Method; Kotlin `<init>` (primary) / `<init>@<row>` (secondary, so several stay
distinct rows — Kotlin df does not lift ctors, so there is no df sym to match).
A constructor's `call_def.name` is the **class name**, so a `new Widget(x)` /
`Widget(x)` call site resolves to the ctor via the bare-name resolver (proven for
TS: `new Widget(1)` → `…::method::Widget.constructor`; ambiguous when several
classes share a name across a mixed corpus, which honestly bails).

## Rust

| callable kind | AST-tier | scip-tier | evidence |
|---|---|---|---|
| free function | **EMITTED** ✓ | pairable | top-level `Item::Fn`. |
| nested/local function | **EMITTED** ✓ | see §Rust scip | `RustCallDefs::visit_item_fn`, Function (file-level mint). Was NOT EMITTED. |
| instance method | **EMITTED** | pairable | `Item::Impl` → `ImplItem::Fn`, Method. |
| static method | **EMITTED (as Method)** | pairable | same arm; no static filter. |
| constructor | **N/A** | — | Rust has no constructor syntax; `new` assoc fns are Methods. |
| getter / setter | **N/A** | — | no accessor syntax. |
| trait method declaration | **EMITTED** ✓ | pairable | `Item::Trait` → `TraitItem::Fn`, no body, Method owned by trait. Was NOT EMITTED. |
| trait default body | **EMITTED** ✓ | pairable | same arm, with body; walked for closures. Was NOT EMITTED. |
| trait impl method | **EMITTED** | pairable | `Item::Impl` → `ImplItem::Fn`. |
| closure / lambda (bound or unbound) | **EMITTED** ✓ | scip-does-not-index-this | `RustCallDefs::visit_expr_closure`, Lambda, `lambda_sym`. Was NOT EMITTED. scip emits no symbol for an anonymous closure — see §Rust scip. |
| operator overload | **EMITTED** | pairable | `impl Add` `fn add` is an `ImplItem::Fn`. |
| async variants | **EMITTED** | pairable | `async fn` is still `Item::Fn`. |
| generator functions | **N/A** | — | no stable generator-fn syntax. |

**Documented Rust non-goals (NOT EMITTED, matching the df lift, which walks only
`Item::Fn`/`Item::Impl` too):** fns/closures inside `const`/`static`
initializers; items inside inline `mod { … }` blocks; anything produced by a
macro body (a standing non-goal). A closure in any of these has no df scope to
join, so registering it would only create an orphan.

### Rust scip pairing (empirical)

`just oracle-index` produces `index.scip` (rust-analyzer) over sprefa's own
`src/`. Pairing measured on `src/graph/typegraph/rust/mod.rs` (the emitter
file itself, rich in closures and nested fns) by joining `call_def` name+line
against
`scip_occurrence` definition occurrences resolved through `scip_name`
(`occ_line + 1` for the 0-based scip line, the `examples/loop-nests.dl`
convention):

| Rust kind (call_def.kind) | call_def rows | paired to a scip def | rate |
|---|---|---|---|
| `function` (free + nested fns) | 209 | 209 | **100%** |
| `method` (impl + trait) | 56 | 56 | **100%** |
| `lambda` (closures) | 199 | 0 | **0%** |

**Findings.** Named callables — including **nested fns**, which rust-analyzer
emits as local scip symbols — pair 1:1 by name+line, so their scip-tier is
`pairable` (verified). **Closures pair 0%: rust-analyzer emits no scip symbol for
an anonymous closure** (there is no name to bind), so lambda `call_def` rows are
AST-tier only — `scip-does-not-index-this`, a finding, not a failure. A coarse
line-only join (ignoring the name) spuriously "pairs" ~96% of closures against a
same-line local/param definition; the name join above is the honest test.

Upstream status (researched 2026-07-18): unreported in both rust-lang/rust-analyzer
and the SCIP repo. Adjacent fixes exist — closure-CAPTURE scoping (PR 18758) and
builtin monikers returning None then being skipped (PR 19105) — but the
closure-definition gap itself is undiscussed. Likely mechanism: `def_to_moniker`
returns None for closures, so no symbol and no occurrence are emitted (the same
shape as the builtins bug). SCIP's `local N` symbol grammar could carry them.
Candidate upstream feature request; not filed.

The `callable-coverage.dl` rail's scip proof is therefore an **optional stratum**:
it activates only when `scip_occurrence` rows exist (an index is present). With no
index the rail stays green on the AST tier alone; the pairing table above is the
Rust-specific evidence. Non-Rust scip-tier cells are `unverified` (no index
generated for those languages in this arc).

## TypeScript / JavaScript

| callable kind | AST-tier | scip-tier | evidence |
|---|---|---|---|
| free function | **EMITTED** | unverified | `FunctionDeclaration` → `ts_fn_call_def`. |
| nested/local function | **EMITTED** ✓ | unverified | `TsNestedFnDefs` visitor emits `function inner(){}` below top level as Function (file-level mint), like Rust nested fns. Was NOT EMITTED. |
| instance method | **EMITTED** | unverified | `ClassDeclaration` → `ts_class_call_defs`. |
| static method | **EMITTED (as Method)** | unverified | same arm, no static filter. |
| constructor | **EMITTED** ✓ | unverified | `ts_class_call_defs` no longer skips the ctor: sym `…::method::<Class>.constructor` (df-matching), name = class. Was NOT EMITTED. |
| getter / setter | **EMITTED (share one Method sym)** | unverified | pass through `ts_class_call_defs`. |
| closure / lambda (unbound) | **EMITTED** ✓ | scip-does-not-index-this | `ts_push_lambda_defs`, derived from the df `closure` value nodes (inline arrow / function-expression argument). A const-bound arrow mints no closure node, so this set is disjoint. Was NOT EMITTED. |
| closure bound to a variable | **EMITTED (as Function)** | unverified | `ts_var_call_defs` — existing identity, unchanged. |
| object-literal / prototype / export-default-anon | NOT EMITTED | unverified | no arm (documented gaps, unchanged). |
| async variants | **EMITTED** | unverified | covered by the fn/arrow/fn-expr arms. |
| generator functions | **EMITTED** | unverified | `function*` / generator methods. |

Also new on the call-SITE side: `new Widget(x)` now emits a `CallSite`
(`TsCallSites::visit_new_expression`) whose callee is the constructed type name,
so it resolves to the ctor `call_def` when the name is unique.

## Kotlin

| callable kind | AST-tier | scip-tier | evidence |
|---|---|---|---|
| free function | **EMITTED** | unverified | top-level `function_declaration`. |
| nested/local function | **EMITTED (as Free)** | unverified | recurse with parent None. |
| instance method | **EMITTED** | unverified | `function_declaration` in a class/object. |
| companion / interface method | **EMITTED** | unverified | (unchanged). |
| constructor | **EMITTED** ✓ | unverified | `primary_constructor`/`secondary_constructor` → Method, sym `<init>` / `<init>@<row>`, name = class. Was NOT EMITTED. |
| getter / setter | NOT EMITTED | unverified | property accessors are not `function_declaration` (unchanged). |
| closure / lambda literal | **EMITTED** ✓ | unverified | `lambda_literal` → Lambda, `lambda_sym` under the enclosing **Function/None** df scope (Kotlin df lifts every fn as Function/None, even methods). Was NOT EMITTED. |
| operator overload | **EMITTED** | unverified | `operator fun` is a `function_declaration`. |
| async (suspend) | **EMITTED** | unverified | `suspend fun` is a `function_declaration`. |

## Go

| callable kind | AST-tier | scip-tier | evidence |
|---|---|---|---|
| free function | **EMITTED** | unverified | `function_declaration` → Free. |
| instance method | **EMITTED** | unverified | `method_declaration` → Method. |
| constructor / static / getter-setter / nested-named | **N/A** | — | not Go constructs. |
| closure / func literal | **EMITTED** ✓ | unverified | `func_literal` inside a fn/method body → Lambda, `lambda_sym`. A package-level `var f = func(){}` (no enclosing fn) is skipped — df does not lift it either. Was NOT EMITTED. |
| interface method spec | NOT EMITTED | unverified | part of `interface_type` (unchanged). |

## Python

| callable kind | AST-tier | scip-tier | evidence |
|---|---|---|---|
| free function | **EMITTED** | unverified | top-level `function_definition`. |
| nested/local function | **EMITTED (as Free)** | unverified | recurse with parent None. |
| instance method | **EMITTED** | unverified | `function_definition` in a class. |
| static method | **EMITTED (as Method)** | unverified | `@staticmethod`/`@classmethod` unwrap to `function_definition`. |
| constructor | **EMITTED (as Method)** | unverified | `__init__` — verified unchanged. |
| getter / setter | **EMITTED (share one Method sym)** | unverified | `@property`/`@x.setter` unwrap. |
| closure / lambda expression | **EMITTED** ✓ | unverified | `lambda` (named-node gate: the `lambda` KEYWORD token shares the node kind, so `is_named()` blocks a double-emit) → Lambda, `lambda_sym` under the enclosing **Function/None** df scope. Was NOT EMITTED. |
| operator / dunder method | **EMITTED** | unverified | `__add__` etc. |
| async variants | **EMITTED** | unverified | `async def` folds into `function_definition`. |
| generator functions | **EMITTED** | unverified | `def` containing `yield`. |

## C status

Unchanged: C has an empty alias list (`src/engine/lang_tables.rs`) and no
`type_langs()` entry, so `.c` corpora produce zero `call_def` rows.

## Summary matrix (AST-tier)

| callable kind | Rust | TS/JS | Kotlin | Go | Python | C |
|---|---|---|---|---|---|---|
| free function | EMITTED | EMITTED | EMITTED | EMITTED | EMITTED | N/A |
| nested/local function | **EMITTED** | **EMITTED** | EMITTED (Free) | N/A | EMITTED (Free) | N/A |
| instance method | EMITTED | EMITTED | EMITTED | EMITTED | EMITTED | N/A |
| static method | EMITTED (Method) | EMITTED (Method) | N/A | N/A | EMITTED (Method) | N/A |
| constructor | N/A | **EMITTED** | **EMITTED** | N/A | EMITTED (Method) | N/A |
| getter / setter | N/A | EMITTED (share sym) | NOT EMITTED | N/A | EMITTED (share sym) | N/A |
| trait/interface method decl | **EMITTED** | NOT EMITTED | EMITTED (Free) | NOT EMITTED | EMITTED (Method) | N/A |
| trait/interface default body | **EMITTED** | N/A | EMITTED (Free) | N/A | EMITTED (Method) | N/A |
| trait/interface impl method | EMITTED | EMITTED | EMITTED | EMITTED | EMITTED | N/A |
| closure / lambda | **EMITTED** | **EMITTED** (unbound) | **EMITTED** | **EMITTED** | **EMITTED** | N/A |
| operator overload / dunder | EMITTED | N/A | EMITTED | N/A | EMITTED | N/A |
| async variants | EMITTED | EMITTED | EMITTED | N/A | EMITTED | N/A |
| generator functions | N/A | EMITTED | N/A | N/A | EMITTED | N/A |

Cells flipped to EMITTED by the callable-lambda-ctor arc are **bold**. Every
EMITTED row for a fixture-covered kind is enforced by `examples/callable-coverage.dl`.
