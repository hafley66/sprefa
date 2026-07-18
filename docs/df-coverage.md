# Per-language dataflow coverage

The engine's `df_node`/`df_edge` lift is syntax-only and conservative: every
language walk mints a node for each value-bearing position it understands and
edges from child values into their parent. Anything not explicitly handled is
silently skipped (no placeholder rows). Use this map to know where the graph is
complete and where it is knowingly sparse.

## Summary

| language | constructs covered | known gaps | evidence |
|---|---|---|---|
| Rust | free fns, impl methods, params, `let`, `let mut`, reassignment, return/tail, if/match/block tails, loop break values, closures, calls, methods, struct literals, fields, borrows, binops/unops, string literals | trait methods, const/static bodies, macros, async/await, `try` blocks, indexed (`[]`) reads | `src/graph/typegraph.rs:4079-4648` |
| TypeScript / JavaScript | free fns, const-bound arrows/function exprs, params (incl. destructured), `let`/`const`/`var`, return, calls, member calls, `new`, object/array literals, JSX elements, member access, binops, `+` concat, template/tagged templates, ternary/short-circuit, arrows as values, top-level module statements, **class methods (instance, static, constructor, getters, setters)** | class field initializers, exported arrow/function expression bodies, switch/with, yield, await (transparent but body still walked) | `src/graph/typegraph.rs:1041-1787` |
| Kotlin | free/top-level and nested `fun`, params, `val`/`var`, return, calls, member calls, constructors, member access, lambdas, binops, string/numeric literals, loops (span only) | if/when/match as value nodes (recursed but no union node), try/catch, break labels as values, destructuring binds | `src/graph/typegraph.rs:612-959` |
| Go | functions, methods, params, `:=`, `var`/`const`, assignments, return, calls, method calls, composite literals, selectors, binops/unops, `func` literals, loops (span + node), if | if as a value node (walked but no union node), switch/select, range with index/value slicing, defer/go statement values | `src/graph/typegraph.rs:5419-5882` |
| Python | functions, methods, params, assignments, return, calls, method calls, attribute/subscript, binops/unops, conditional expressions, lambdas, list/set/tuple/dict literals, comprehensions | f-string interpolation values (treated as `lit`), walrus `:=`, `with`/`try` values, async/await (transparent but body still walked), generators beyond comprehensions | `src/graph/typegraph.rs:6801-7349` |

## Rust

Parser: `syn`. Node ids are `file:line:col:kind`.

Covered:

- Free functions and `impl` block methods (`Item::Fn`, `Item::Impl` with
  `ImplItem::Fn`) — `src/graph/typegraph.rs:4082-4097`.
- Params (typed params only; `self` is skipped so positional indices align with
  `type_sig`) — `src/graph/typegraph.rs:4148-4169`.
- `let` bindings and `let mut`, including tuple/struct destructuring
  (`bind_pat`) — `src/graph/typegraph.rs:4202-4210`, `src/graph/typegraph.rs:4655-4698`.
- Reassignment via `=` (`Expr::Assign`) mints a `var_write` slot
  — `src/graph/typegraph.rs:4642-4724`.
- Explicit `return EXPR` and implicit block/match/if tails: the last
  expression of a function body flows into a `ret` node
  — `src/graph/typegraph.rs:4170-4184`; `if`/`match`/`block` tails were recently
  added and now feed a dedicated `if`/`match`/`block` node that itself becomes
  the value — `src/graph/typegraph.rs:4540-4591`.
- Loop break values: `loop { ... break v ... }` collects break-value tails and
  edges them into the `loop` node — `src/graph/typegraph.rs:4449-4539`. `for`/`while`
  do not yield break values (matches Rust semantics).
- Calls and method calls, with args recorded in `df_arg` (receiver at slot -1)
  — `src/graph/typegraph.rs:4334-4375`.
- Struct literals / tuple-struct/enum-variant constructors (`new`) and field
  reads (`member`) — `src/graph/typegraph.rs:4380-4415`.
- References (`borrow`), binary/unary operators — `src/graph/typegraph.rs:4417-4436`.
- Closures lifted as their own fn scope (`param`, `ret`, `closure` value node)
  — `src/graph/typegraph.rs:4600-4638`.
- String literals populate `df_lit` — `src/graph/typegraph.rs:4321-4327`.

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
  — `src/graph/typegraph.rs:1058-1100`.
- `const`/`let`/`var` bindings, including destructuring targets that fall back
  to a single whole-pattern bind — `src/graph/typegraph.rs:1176-1208`.
- `const`-bound arrow functions and function expressions are lifted as their own
  fn scope (`param`, body, `ret`) — `src/graph/typegraph.rs:1182-1197`,
  `src/graph/typegraph.rs:1142-1164`.
- Inline arrow/function expressions produce a `closure` value node whose `var`
  is the synthetic lambda sym — `src/graph/typegraph.rs:1511-1523`.
- Calls, member calls, `new`, object/array literals, JSX elements/fragments;
  receiver at `df_arg` slot -1, named props/attrs in `df_field`
  — `src/graph/typegraph.rs:1322-1787`.
- Member access (`recv.prop`, `recv?.prop`, `recv[expr]`) as `member`
  — `src/graph/typegraph.rs:1359-1373`.
- `+` concat, other binary operators, logical short-circuit, ternary
  — `src/graph/typegraph.rs:1479-1614`.
- Template literals and tagged templates, with raw source slices in `df_lit`
  — `src/graph/typegraph.rs:1628-1652`.
- Top-level module statements are wrapped in a synthetic `<top>` fn scope
  — `src/graph/typegraph.rs:1076-1080`.
- Class methods — instance, static, constructor, getters, setters — flow like a
  free function's body, scoped under the `Owner.method` fn sym
  `ts_class_call_defs`/`ts_class_entity` already mint for the same method
  (`ts_flow_class`, reached from `ts_flow_stmt`'s `ClassDeclaration` arm and
  `ts_flow_decl`'s for the `export class` path)
  — `src/graph/typegraph.rs:1075`, `src/graph/typegraph.rs:1097`,
  `src/graph/typegraph.rs:1109-1122`. A getter and setter of the same name
  share one fn sym (`Owner.count` for both `get count()`/`set count()`) since
  neither the dataflow lift nor the call/type extraction distinguishes them —
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
  — `src/graph/typegraph.rs:5431-5452`.
- Params, including grouped params (`a, b int`), one param node per declared name
  — `src/graph/typegraph.rs:5459-5482`.
- `:=` short declarations, `var`/`const` declarations, assignments (with
  `var_write` for identifier targets) — `src/graph/typegraph.rs:5606-5642`.
- `return` statements, one `ret` node per returned expression
  — `src/graph/typegraph.rs:5647-5666`.
- Calls, method calls, selectors, composite literals, `func` literals
  — `src/graph/typegraph.rs:5517-5766`.
- Binary/unary operators — `src/graph/typegraph.rs:5588-5600`.
- `if` statements and `for` loops (span + `loop`/`if` value node)
  — `src/graph/typegraph.rs:5667-5734`.

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
  `decorated_definition` is unwrapped first — `src/graph/typegraph.rs:6810-6819`.
- Params; `self`/`cls` are skipped so positional indices align with `type_sig`
  — `src/graph/typegraph.rs:6832-6859`.
- Assignments, return statements — `src/graph/typegraph.rs:6884-6892`.
- Calls, method calls, attribute/subscript reads
  — `src/graph/typegraph.rs:7132-7217`.
- Binary/boolean/comparison/unary operators, conditional expressions
  — `src/graph/typegraph.rs:7218-7250`.
- Lambdas lifted as their own fn scope — `src/graph/typegraph.rs:7266-7287`.
- List/set/tuple/dict literals and comprehensions (list/set/dict/generator)
  — `src/graph/typegraph.rs:7288-7339`.
- `for`/`while` loop spans — `src/graph/typegraph.rs:6893-7025`.

Known gaps:

- f-string interpolations are treated as a single `lit` node; interpolated
  expressions do not feed into it.
- Walrus operator (`:=`), `with`, `try`/`except`/`finally`, `raise`, `yield`,
  and `async`/`await` are not specially modeled (`await` is transparent to its
  argument).
- `df_lit` is not populated.

`df_node` kinds: `param`, `let_bind`, `var_read`, `lit`, `call_res`, `new`,
`member`, `binop`, `unop`, `cond`, `closure`, `ret`, `expr`.
