# Per-language dataflow coverage

The engine's `df_node`/`df_edge` lift is syntax-only and conservative: every
language walk mints a node for each value-bearing position it understands and
edges from child values into their parent. Anything not explicitly handled is
silently skipped (no placeholder rows). Use this map to know where the graph is
complete and where it is knowingly sparse.

## Summary

| language | constructs covered | known gaps | evidence |
|---|---|---|---|
| Rust | free fns, impl methods, params, `let`, `let mut`, reassignment, return/tail, if/match/block tails, loop break values, closures, calls, methods, struct literals, fields, borrows, binops/unops, string literals | trait methods, const/static bodies, macros, async/await, `try` blocks, indexed (`[]`) reads | `src/graph/typegraph.rs:4055-4624` |
| TypeScript / JavaScript | free fns, const-bound arrows/function exprs, params (incl. destructured), `let`/`const`/`var`, return, calls, member calls, `new`, object/array literals, JSX elements, member access, binops, `+` concat, template/tagged templates, ternary/short-circuit, arrows as values, top-level module statements | **class methods**, class field initializers, exported arrow/function expression bodies, switch/with, yield, await (transparent but body still walked) | `src/graph/typegraph.rs:1041-1763` |
| Kotlin | free/top-level and nested `fun`, params, `val`/`var`, return, calls, member calls, constructors, member access, lambdas, binops, string/numeric literals, loops (span only) | if/when/match as value nodes (recursed but no union node), try/catch, break labels as values, destructuring binds | `src/graph/typegraph.rs:612-959` |
| Go | functions, methods, params, `:=`, `var`/`const`, assignments, return, calls, method calls, composite literals, selectors, binops/unops, `func` literals, loops (span + node), if | if as a value node (walked but no union node), switch/select, range with index/value slicing, defer/go statement values | `src/graph/typegraph.rs:5395-5858` |
| Python | functions, methods, params, assignments, return, calls, method calls, attribute/subscript, binops/unops, conditional expressions, lambdas, list/set/tuple/dict literals, comprehensions | f-string interpolation values (treated as `lit`), walrus `:=`, `with`/`try` values, async/await (transparent but body still walked), generators beyond comprehensions | `src/graph/typegraph.rs:6777-7325` |

## Rust

Parser: `syn`. Node ids are `file:line:col:kind`.

Covered:

- Free functions and `impl` block methods (`Item::Fn`, `Item::Impl` with
  `ImplItem::Fn`) — `src/graph/typegraph.rs:4058-4073`.
- Params (typed params only; `self` is skipped so positional indices align with
  `type_sig`) — `src/graph/typegraph.rs:4124-4145`.
- `let` bindings and `let mut`, including tuple/struct destructuring
  (`bind_pat`) — `src/graph/typegraph.rs:4178-4186`, `src/graph/typegraph.rs:4631-4674`.
- Reassignment via `=` (`Expr::Assign`) mints a `var_write` slot
  — `src/graph/typegraph.rs:4618-4700`.
- Explicit `return EXPR` and implicit block/match/if tails: the last
  expression of a function body flows into a `ret` node
  — `src/graph/typegraph.rs:4146-4160`; `if`/`match`/`block` tails were recently
  added and now feed a dedicated `if`/`match`/`block` node that itself becomes
  the value — `src/graph/typegraph.rs:4516-4567`.
- Loop break values: `loop { ... break v ... }` collects break-value tails and
  edges them into the `loop` node — `src/graph/typegraph.rs:4425-4515`. `for`/`while`
  do not yield break values (matches Rust semantics).
- Calls and method calls, with args recorded in `df_arg` (receiver at slot -1)
  — `src/graph/typegraph.rs:4310-4351`.
- Struct literals / tuple-struct/enum-variant constructors (`new`) and field
  reads (`member`) — `src/graph/typegraph.rs:4356-4391`.
- References (`borrow`), binary/unary operators — `src/graph/typegraph.rs:4393-4412`.
- Closures lifted as their own fn scope (`param`, `ret`, `closure` value node)
  — `src/graph/typegraph.rs:4576-4614`.
- String literals populate `df_lit` — `src/graph/typegraph.rs:4297-4303`.

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

Covered:

- `function` declarations and exported function declarations
  — `src/graph/typegraph.rs:1058-1098`.
- `const`/`let`/`var` bindings, including destructuring targets that fall back
  to a single whole-pattern bind — `src/graph/typegraph.rs:1152-1184`.
- `const`-bound arrow functions and function expressions are lifted as their own
  fn scope (`param`, body, `ret`) — `src/graph/typegraph.rs:1158-1173`,
  `src/graph/typegraph.rs:1118-1140`.
- Inline arrow/function expressions produce a `closure` value node whose `var`
  is the synthetic lambda sym — `src/graph/typegraph.rs:1487-1499`.
- Calls, member calls, `new`, object/array literals, JSX elements/fragments;
  receiver at `df_arg` slot -1, named props/attrs in `df_field`
  — `src/graph/typegraph.rs:1298-1763`.
- Member access (`recv.prop`, `recv?.prop`, `recv[expr]`) as `member`
  — `src/graph/typegraph.rs:1335-1349`.
- `+` concat, other binary operators, logical short-circuit, ternary
  — `src/graph/typegraph.rs:1455-1590`.
- Template literals and tagged templates, with raw source slices in `df_lit`
  — `src/graph/typegraph.rs:1604-1628`.
- Top-level module statements are wrapped in a synthetic `<top>` fn scope
  — `src/graph/typegraph.rs:1075-1079`.

Known gaps:

- **Class methods and class field initializers emit zero `df_node` rows.** The
  dataflow statement walker (`ts_flow_stmt`) handles `FunctionDeclaration`,
  `ExportNamedDeclaration`, `VariableDeclaration`, `ExpressionStatement`, and
  `ReturnStatement`, but has no arm for `ClassDeclaration`
  — `src/graph/typegraph.rs:1058-1082`. Class methods are handled by the
  type/call passes but never reach the dataflow walk.
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
internally and bumped to 1-based before output.

Covered:

- `function_declaration` anywhere in the file (top-level, nested, or inside a
  class body) — `src/graph/typegraph.rs:627-635`.
- Params and `val`/`var` property declarations — `src/graph/typegraph.rs:643-654`,
  `src/graph/typegraph.rs:798-825`.
- Explicit `return` (`jump_expression`) and implicit function-body tail
  — `src/graph/typegraph.rs:655-664`, `src/graph/typegraph.rs:875-890`.
- Calls, member calls, constructors (capitalized bare callee treated as `new`)
  — `src/graph/typegraph.rs:702-776`.
- Member access outside call position (`navigation_expression`) —
  `src/graph/typegraph.rs:782-796`.
- Lambda literals lifted as their own fn scope; implicit `it` param when no
  parameter list is declared — `src/graph/typegraph.rs:844-874`.
- Binary/infix operators (`binop`) and literals — `src/graph/typegraph.rs:897-909`.
- Loop spans recorded for `for`/`while`/`do_while` — `src/graph/typegraph.rs:910-935`.

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
internally and bumped to 1-based before output.

Covered:

- `function_declaration` and `method_declaration` (with receiver type)
  — `src/graph/typegraph.rs:5407-5428`.
- Params, including grouped params (`a, b int`), one param node per declared name
  — `src/graph/typegraph.rs:5435-5458`.
- `:=` short declarations, `var`/`const` declarations, assignments (with
  `var_write` for identifier targets) — `src/graph/typegraph.rs:5582-5618`.
- `return` statements, one `ret` node per returned expression
  — `src/graph/typegraph.rs:5623-5642`.
- Calls, method calls, selectors, composite literals, `func` literals
  — `src/graph/typegraph.rs:5493-5742`.
- Binary/unary operators — `src/graph/typegraph.rs:5564-5576`.
- `if` statements and `for` loops (span + `loop`/`if` value node)
  — `src/graph/typegraph.rs:5643-5710`.

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
internally and bumped to 1-based before output.

Covered:

- `function_definition` anywhere in the file (module-level, method, or nested);
  `decorated_definition` is unwrapped first — `src/graph/typegraph.rs:6786-6795`.
- Params; `self`/`cls` are skipped so positional indices align with `type_sig`
  — `src/graph/typegraph.rs:6808-6835`.
- Assignments, return statements — `src/graph/typegraph.rs:6860-6868`.
- Calls, method calls, attribute/subscript reads
  — `src/graph/typegraph.rs:7108-7193`.
- Binary/boolean/comparison/unary operators, conditional expressions
  — `src/graph/typegraph.rs:7194-7226`.
- Lambdas lifted as their own fn scope — `src/graph/typegraph.rs:7242-7263`.
- List/set/tuple/dict literals and comprehensions (list/set/dict/generator)
  — `src/graph/typegraph.rs:7264-7315`.
- `for`/`while` loop spans — `src/graph/typegraph.rs:6869-7001`.

Known gaps:

- f-string interpolations are treated as a single `lit` node; interpolated
  expressions do not feed into it.
- Walrus operator (`:=`), `with`, `try`/`except`/`finally`, `raise`, `yield`,
  and `async`/`await` are not specially modeled (`await` is transparent to its
  argument).
- `df_lit` is not populated.

`df_node` kinds: `param`, `let_bind`, `var_read`, `lit`, `call_res`, `new`,
`member`, `binop`, `unop`, `cond`, `closure`, `ret`, `expr`.
