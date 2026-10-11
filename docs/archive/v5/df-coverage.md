# Per-language dataflow coverage

Callable-kind coverage (call_def): docs/callable-coverage.md

The engine's `df_node`/`df_edge` lift is syntax-only and conservative: every
language walk mints a node for each value-bearing position it understands and
edges from child values into their parent. Anything not explicitly handled is
silently skipped (no placeholder rows). Use this map to know where the graph is
complete and where it is knowingly sparse.

Citations below are `file fn-name` anchors (2026-07-18 decomposition-
normalization step 3): `typegraph.rs` split into `src/graph/typegraph/`
per-language modules, and raw line numbers drift with every edit — a
function name survives a refactor a line number doesn't. Each per-language
extractor centers on one recursive expression walker (`flow_expr` for Rust,
`ts_flow_expr` for TS, `flow_kt` for Kotlin, `flow_go` for Go, `py_flow_expr`
for Python) that handles most value-bearing constructs as match arms, so it
recurs as the anchor for several bullets below; narrower bullets cite the
specific fn that owns that behavior.

## Summary

| language | constructs covered | known gaps | evidence |
|---|---|---|---|
| Rust | free fns, impl methods, params, `let`, `let mut`, reassignment, return/tail, if/match/block tails, loop break values, closures, calls, methods, struct literals, fields, borrows, binops/unops, string literals | trait methods, const/static bodies, macros, async/await, `try` blocks, indexed (`[]`) reads | `src/graph/typegraph/rust/mod.rs` `rust_dataflow_from` |
| TypeScript / JavaScript | free fns, const-bound arrows/function exprs, params (incl. destructured), `let`/`const`/`var`, return, calls, member calls, `new`, object/array literals, JSX elements, member access, binops, `+` concat, template/tagged templates, ternary/short-circuit, arrows as values, top-level module statements, **class methods (instance, static, constructor, getters, setters)** | class field initializers, exported arrow/function expression bodies, switch/with, yield, await (transparent but body still walked) | `src/graph/typegraph/ts/flow.rs` `ts_dataflow_from` |
| Kotlin | free/top-level and nested `fun`, params, `val`/`var`, return, calls, member calls, constructors, member access, lambdas, binops, string/numeric literals, loops (span only) | if/when/match as value nodes (recursed but no union node), try/catch, break labels as values, destructuring binds | `src/graph/typegraph/kotlin.rs` `kotlin_dataflow_from` |
| Go | functions, methods, params, `:=`, `var`/`const`, assignments, return, calls, method calls, composite literals, selectors, binops/unops, `func` literals, loops (span + node), if | if as a value node (walked but no union node), switch/select, range with index/value slicing, defer/go statement values | `src/graph/typegraph/go.rs` `go_dataflow_from` |
| Python | functions, methods, params, assignments, return, calls, method calls, attribute/subscript, binops/unops, conditional expressions, lambdas, list/set/tuple/dict literals, comprehensions | f-string interpolation values (treated as `lit`), walrus `:=`, `with`/`try` values, async/await (transparent but body still walked), generators beyond comprehensions | `src/graph/typegraph/python.rs` `py_dataflow_from` |

## Rust

Parser: `syn`. Node ids are `file:line:col:kind`. File: `src/graph/typegraph/rust/mod.rs`.

Covered:

- Free functions and `impl` block methods (`Item::Fn`, `Item::Impl` with
  `ImplItem::Fn`) — `rust_dataflow_from`.
- Params (typed params only; `self` is skipped so positional indices align with
  `type_sig`) — `flow_fn_body`.
- `let` bindings and `let mut`, including tuple/struct destructuring
  — `bind_pat`.
- Reassignment via `=` (`Expr::Assign`) mints a `var_write` slot
  — `assign_flow`.
- Explicit `return EXPR` and implicit block/match/if tails: the last
  expression of a function body flows into a `ret` node
  — `flow_expr` (`Expr::Return`/`Expr::If`/`Expr::Match` arms), `flow_block`
  (implicit block tail).
- Loop break values: `loop { ... break v ... }` collects break-value tails and
  edges them into the `loop` node — `flow_expr` (`Expr::Loop`/break arm).
  `for`/`while` do not yield break values (matches Rust semantics).
- Calls and method calls, with args recorded in `df_arg` (receiver at slot -1)
  — `flow_expr`.
- Struct literals / tuple-struct/enum-variant constructors (`new`) and field
  reads (`member`) — `flow_expr` (`Expr::Struct`/`Expr::Field` arms).
- References (`borrow`), binary/unary operators — `flow_expr`
  (`Expr::Reference`/`Expr::Binary`/`Expr::Unary` arms).
- Closures lifted as their own fn scope (`param`, `ret`, `closure` value node)
  — `flow_expr` (`Expr::Closure` arm).
- String literals populate `df_lit` — `flow_expr` (`Expr::Lit` arm).

Known gaps:

- `trait` methods and default method bodies are not walked.
- `const`/`static` item bodies are not lifted.
- Macros (`macro_rules!`, `format!`, etc.) are not expanded; they mint a generic
  `expr` node only.
- `try` blocks, `async` blocks, and `await` are not specially handled.
- Index/slice expressions (`[]`) are not field-sensitive.

`df_node` kinds: `param`, `let_bind`, `var_read`, `var_write`, `lit`, `call_res`,
`new`, `member`, `ret`, `borrow`, `binop`, `unop`, `loop`, `if`, `match`, `block`,
`break`, `closure`, `expr`.

## TypeScript / JavaScript

Parser: `oxc`. Node ids are `file:<byte_off>:kind`; line numbers are recovered
from a byte-offset index. Handles `.ts`/`.tsx`/`.js`/`.jsx`/`.mjs`/`.cjs`/`.mts`/`.cts`.
File: `src/graph/typegraph/ts/flow.rs`.

Covered:

- `function` declarations and exported function declarations
  — `ts_flow_stmt`.
- `const`/`let`/`var` bindings, including destructuring targets that fall back
  to a single whole-pattern bind — `ts_flow_body_stmt`.
- `const`-bound arrow functions and function expressions are lifted as their own
  fn scope (`param`, body, `ret`) — `ts_lift_fn`, reached from
  `ts_flow_body_stmt`.
- Inline arrow/function expressions produce a `closure` value node whose `var`
  is the synthetic lambda sym — `ts_flow_expr`.
- Calls, member calls, `new`, object/array literals, JSX elements/fragments;
  receiver at `df_arg` slot -1, named props/attrs in `df_field`
  — `ts_flow_call`, `ts_flow_member`, `ts_flow_expr`, `ts_flow_jsx_element`.
- Member access (`recv.prop`, `recv?.prop`, `recv[expr]`) as `member`
  — `ts_flow_member`.
- `+` concat, other binary operators, logical short-circuit, ternary
  — `ts_flow_expr`.
- Template literals and tagged templates, with raw source slices in `df_lit`
  — `ts_flow_expr`.
- Top-level module statements are wrapped in a synthetic `<top>` fn scope
  — `ts_flow_stmt`.
- Class methods — instance, static, constructor, getters, setters — flow like a
  free function's body, scoped under the `Owner.method` fn sym
  `ts_class_call_defs`/`ts_class_entity` already mint for the same method
  (`ts_flow_class`, reached from `ts_flow_stmt`'s `ClassDeclaration` arm and
  `ts_flow_decl`'s for the `export class` path)
  — `ts_flow_class`, `ts_flow_stmt`, `ts_flow_decl`. A getter and setter of the
  same name share one fn sym (`Owner.count` for both `get count()`/`set count()`)
  since neither the dataflow lift nor the call/type extraction distinguishes them —
  their df_node rows don't collide (ids are `file:byte_off:kind`) but a query
  joining on fn sym alone can't tell get from set.

Known gaps:

- **Class field initializers emit zero `df_node` rows.** A `PropertyDefinition`
  init expression (`class C { x = f(); }`) has no natural enclosing fn scope
  for `ts_flow_class` to attach nodes to — routing it through the constructor's
  scope would misrepresent field-init order relative to constructor statements,
  and TS/JS also allows fields with no constructor at all.
- Exported arrow/function expression bodies are lifted only when the binding is
  a `const`/`let`/`var` declaration; standalone exported function expressions
  are unverified.
- `switch`, `with`, `yield`, labeled statements, and `throw` are not specially
  handled.
- `await` is transparent (flows the argument) but async control flow is not
  modeled.

`df_node` kinds: `param`, `let_bind`, `var_read`, `lit`, `call_res`, `new`,
`member`, `ret`, `closure`, `binop`, `concat`, `cond`, `logic`, `template`,
`expr`.

## Kotlin

Parser: `tree-sitter-kotlin`. Node ids are `file:row:col:kind`; rows are 0-based
internally and bumped to 1-based before output. File: `src/graph/typegraph/kotlin.rs`.

Covered:

- `function_declaration` anywhere in the file (top-level, nested, or inside a
  class body) — `kt_walk_fns`.
- Params and `val`/`var` property declarations — `kt_flow_fn`.
- Explicit `return` (`jump_expression`) and implicit function-body tail
  — `kt_flow_fn` (implicit tail), `flow_kt` (`jump_expression` arm).
- Calls, member calls, constructors (capitalized bare callee treated as `new`)
  — `flow_kt` (`call_expression` arm).
- Member access outside call position (`navigation_expression`) —
  `flow_kt` (`navigation_expression` arm).
- Lambda literals lifted as their own fn scope; implicit `it` param when no
  parameter list is declared — `flow_kt` (`lambda_literal` arm).
- Binary/infix operators (`binop`) and literals — `flow_kt`.
- Loop spans recorded for `for`/`while`/`do_while` — `flow_kt`.

Known gaps:

- `if`/`when` expressions are recursed into but no `if`/`when` value node is
  minted, so value-position conditionals may only carry the last walked branch.
- `try`/`catch`/`finally` is not specially handled.
- Destructuring binds (`val (a, b) = ...`) mint no slots.
- `df_lit` is not populated; string content is not extracted.

`df_node` kinds: `param`, `let_bind`, `var_read`, `call_res`, `new`, `member`,
`closure`, `binop`, `lit`, `ret`, `loop` (loop span is recorded in `loop_over`,
but no `loop` value node is minted).

## Go

Parser: `tree-sitter-go`. Node ids are `file:row:col:kind`; rows are 0-based
internally and bumped to 1-based before output. File: `src/graph/typegraph/go.rs`.

Covered:

- `function_declaration` and `method_declaration` (with receiver type)
  — `go_walk_fns`.
- Params, including grouped params (`a, b int`), one param node per declared name
  — `go_flow_fn`.
- `:=` short declarations, `var`/`const` declarations, assignments (with
  `var_write` for identifier targets) — `flow_go` (`short_var_declaration`/
  `assignment_statement` arms, via `go_bind`).
- `return` statements, one `ret` node per returned expression
  — `flow_go`.
- Calls, method calls, selectors, composite literals, `func` literals
  — `flow_go`.
- Binary/unary operators — `flow_go` (`binary_expression`/`unary_expression`
  arms).
- `if` statements and `for` loops (span + `loop`/`if` value node)
  — `flow_go` (`if_statement`/`for_statement` arms).

Known gaps:

- `if` is walked but the union of branch values is not fed into the `if` node;
  the node exists mainly as a span marker.
- `switch`, `select`, `defer`, `go`, and range index/value slicing are not
  specially modeled.
- `df_lit` is not populated.

`df_node` kinds: `param`, `let_bind`, `var_read`, `var_write`, `lit`, `call_res`,
`new`, `member`, `ret`, `binop`, `unop`, `if`, `loop`, `closure`.

## Python

Parser: `tree-sitter-python`. Node ids are `file:row:col:kind`; rows are 0-based
internally and bumped to 1-based before output. File: `src/graph/typegraph/python.rs`.

Covered:

- `function_definition` anywhere in the file (module-level, method, or nested);
  `decorated_definition` is unwrapped first — `py_walk_fns`.
- Params; `self`/`cls` are skipped so positional indices align with `type_sig`
  — `py_flow_fn`.
- Assignments, return statements — `py_flow_stmt` (dispatches to
  `py_flow_assignment` for assignment, handles `return_statement` directly).
- Calls, method calls, attribute/subscript reads
  — `py_flow_expr`.
- Binary/boolean/comparison/unary operators, conditional expressions
  — `py_flow_expr`.
- Lambdas lifted as their own fn scope — `py_flow_expr` (`lambda` arm).
- List/set/tuple/dict literals and comprehensions (list/set/dict/generator)
  — `py_flow_expr`, `py_comprehension_flow`.
- `for`/`while` loop spans — `py_flow_for`, `py_flow_while`.

Known gaps:

- f-string interpolations are treated as a single `lit` node; interpolated
  expressions do not feed into it.
- Walrus operator (`:=`), `with`, `try`/`except`/`finally`, `raise`, `yield`,
  and `async`/`await` are not specially modeled (`await` is transparent to its
  argument).
- `df_lit` is not populated.

`df_node` kinds: `param`, `let_bind`, `var_read`, `lit`, `call_res`, `new`,
`member`, `binop`, `unop`, `cond`, `closure`, `ret`, `expr`.
