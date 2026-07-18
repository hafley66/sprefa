# Per-language callable-kind coverage for `call_def`

The `call_def` relation is the callable registry: one row per named callable the
syntactic front-end can see. This page audits which *kinds* of callable each
language extractor emits, independent of later call-site resolution.

The emitters live in `src/graph/typegraph.rs`. Each language has a dedicated
walk that pushes `CallDef` rows:

- Rust: `rust_call_defs_from` (`src/graph/typegraph.rs:3950-3982`)
- TypeScript / JavaScript: `ts_call_defs_from` / `ts_fn_call_def` /
  `ts_class_call_defs` / `ts_var_call_defs` (`src/graph/typegraph.rs:3569-3664`)
- Kotlin: `kt_walk_call_defs` (`src/graph/typegraph.rs:2328-2372`)
- Go: `go_walk_call_defs` (`src/graph/typegraph.rs:5234-5272`)
- Python: `py_call_defs_from` / `py_walk_call_defs`
  (`src/graph/typegraph.rs:6548-6594`)
- C: no extractor (see C status below)

`EntityKind` only has `Function` and `Method` callable variants
(`src/graph/typegraph.rs:32-40`), and `CallKind` has `Free`, `Method`, and
`Closure` (`src/graph/typegraph.rs:282-286`). No current extractor emits
`CallKind::Closure`; closures and lambdas appear only as `closure` nodes in the
dataflow lift.

## Rust

| callable kind | status | evidence / absence |
|---|---|---|
| free function | **EMITTED** | `rust_call_defs_from` matches top-level `Item::Fn` (`src/graph/typegraph.rs:3959-3963`). |
| nested/local function | **NOT EMITTED** | `rust_call_defs_from` matches only top-level items (`src/graph/typegraph.rs:3957`) and does not recurse into fn bodies. |
| instance method | **EMITTED** | `Item::Impl` → `ImplItem::Fn` becomes `Method` (`src/graph/typegraph.rs:3965-3975`). |
| static method | **EMITTED (as Method)** | same arm as instance methods; `ImplItem::Fn` has no static filter (`src/graph/typegraph.rs:3968-3975`). |
| constructor | **N/A** | Rust has no constructor syntax; `new` associated fns are captured as methods above. |
| getter / setter | **N/A** | no accessor syntax; methods named `get_x` / `set_x` are captured as methods. |
| trait method declaration | **NOT EMITTED** | `rust_call_defs_from` has no `Item::Trait` arm. |
| trait default body | **NOT EMITTED** | same reason; trait items are not matched. |
| trait impl method | **EMITTED** | `Item::Impl` → `ImplItem::Fn` (`src/graph/typegraph.rs:3965-3975`). |
| closure / lambda / anonymous fn (unbound) | **NOT EMITTED** | `rust_call_defs_from` has no closure arm. The dataflow lift mints `closure` df_nodes (`src/graph/typegraph.rs:4600-4637`) but no `CallDef`. |
| closure bound to a variable | **NOT EMITTED** | no variable-binding arm in `rust_call_defs_from`; `let f = \|…\| …` is not registered. |
| operator overload / dunder method | **EMITTED** | `impl Add for Foo { fn add(...) }` is an `ImplItem::Fn` (`src/graph/typegraph.rs:3968-3975`). |
| async variants | **EMITTED** | `async fn` is still `Item::Fn` (`src/graph/typegraph.rs:3959-3963`). |
| generator functions | **N/A** | Rust has no stable generator-function syntax. |

## TypeScript / JavaScript

| callable kind | status | evidence / absence |
|---|---|---|
| free function | **EMITTED** | `FunctionDeclaration` → `ts_fn_call_def` (`src/graph/typegraph.rs:3574`, `src/graph/typegraph.rs:3606-3618`). |
| nested/local function | **NOT EMITTED** | `ts_call_defs_from` iterates `program.body` only (`src/graph/typegraph.rs:3571`) and does not recurse into function bodies. |
| instance method | **EMITTED** | `ClassDeclaration` → `ts_class_call_defs` (`src/graph/typegraph.rs:3589`, `src/graph/typegraph.rs:3620-3641`). |
| static method | **EMITTED (as Method)** | `ts_class_call_defs` matches `ClassElement::MethodDefinition` with no `static` filter (`src/graph/typegraph.rs:3624-3639`). |
| constructor | **NOT EMITTED** | explicitly skipped at `src/graph/typegraph.rs:3626-3628`. |
| getter / setter | **EMITTED (share one Method sym)** | only `Constructor` is skipped (`src/graph/typegraph.rs:3626`), so `Get`/`Set` methods pass through; same-name getter/setter mint the identical `Method` sym. |
| interface method declaration | **NOT EMITTED** | `ts_call_defs_from` has no `TSInterfaceDeclaration` / `TSMethodSignature` arm. |
| interface default body | **N/A** | TypeScript interfaces cannot carry bodies. |
| class method implementing interface | **EMITTED** | same class-method path (`ts_class_call_defs`). |
| closure / lambda / anonymous fn (unbound) | **NOT EMITTED** | inline arrow/function expression not bound to a variable is skipped by `ts_var_call_defs`; only a dataflow `closure` node is minted. |
| closure bound to a variable | **EMITTED** | `VariableDeclaration` with `ArrowFunctionExpression` or `FunctionExpression` init (`src/graph/typegraph.rs:3590`, `src/graph/typegraph.rs:3643-3664`). |
| object-literal method | **NOT EMITTED** | `ts_call_defs_from` has no `ObjectExpression` / `ObjectProperty` arm. |
| prototype-assigned function | **NOT EMITTED** | assignment expressions (`Foo.prototype.bar = function(){}`) are not matched. |
| export-default anonymous fn | **NOT EMITTED** | `ts_fn_call_def` requires `f.id` (`src/graph/typegraph.rs:3607`), so `export default function(){}` falls through. |
| operator overload | **N/A** | JavaScript has no user-defined operator overloading. |
| async variants | **EMITTED** | `FunctionDeclaration` / `ArrowFunctionExpression` / `FunctionExpression` cover `async function`, `async () =>`, etc. (`src/graph/typegraph.rs:3606`, `src/graph/typegraph.rs:3646-3652`). |
| generator functions | **EMITTED** | `FunctionDeclaration` / `MethodDefinition` cover `function*` / generator methods (`src/graph/typegraph.rs:3606`, `src/graph/typegraph.rs:3624`). |

## Kotlin

| callable kind | status | evidence / absence |
|---|---|---|
| free function | **EMITTED** | `function_declaration` with `parent=None` (`src/graph/typegraph.rs:2343-2366`). |
| nested/local function | **EMITTED (as Free)** | `kt_walk_call_defs` recurses into function bodies with `parent=None` (`src/graph/typegraph.rs:2367`). |
| instance method | **EMITTED** | `function_declaration` inside `class_declaration` / `object_declaration` (`src/graph/typegraph.rs:2347-2349`). |
| static method | **N/A** | Kotlin has no `static` keyword; companion-object methods are emitted as Method with the companion object as owner (`src/graph/typegraph.rs:2338-2341`), and top-level fns are Free. |
| constructor | **NOT EMITTED** | `kt_walk_call_defs` matches only `function_declaration`; `primary_constructor` / `secondary_constructor` have no arm. |
| getter / setter | **NOT EMITTED** | property accessors are not `function_declaration`; only `function_declaration` arms exist. |
| interface method declaration | **EMITTED (as Free, no owner)** | `kt_walk_call_defs` does not match `interface_declaration` as an owner (`src/graph/typegraph.rs:2337`), so a `function_declaration` inside an interface is emitted with `parent=None`. |
| interface default body | **EMITTED (as Free)** | same path as interface declarations. |
| interface impl method | **EMITTED (as Method)** | `function_declaration` inside a class that implements the interface (`src/graph/typegraph.rs:2347-2349`). |
| closure / lambda / anonymous fn (unbound) | **NOT EMITTED** | `lambda_literal` is not matched. |
| closure bound to a variable | **NOT EMITTED** | `property_declaration` with a lambda initializer is not matched. |
| operator overload | **EMITTED** | `operator fun` is still a `function_declaration` (`src/graph/typegraph.rs:2343-2366`). |
| async variants | **EMITTED** | suspending functions are still `function_declaration`. |
| generator functions | **N/A** | Kotlin has no generator-function syntax. |

## Go

| callable kind | status | evidence / absence |
|---|---|---|
| free function | **EMITTED** | `function_declaration` → `CallKind::Free` (`src/graph/typegraph.rs:5238-5249`). |
| nested/local function | **N/A** | Go does not allow nested named function declarations. |
| instance method | **EMITTED** | `method_declaration` → `CallKind::Method` (`src/graph/typegraph.rs:5252-5266`). |
| static method | **N/A** | Go has no static methods; all methods have receivers. |
| constructor | **N/A** | Go has no constructors; `NewT` factory functions are captured as Free functions. |
| getter / setter | **N/A** | no accessor syntax; `GetX` / `SetX` methods are captured as instance methods. |
| interface method declaration | **NOT EMITTED** | `go_walk_call_defs` matches only `function_declaration` and `method_declaration`; interface method specs are part of `interface_type` nodes. |
| interface default body | **N/A** | Go interfaces cannot carry method bodies. |
| interface impl method | **EMITTED** | any `method_declaration` implements an interface (`src/graph/typegraph.rs:5252-5266`). |
| closure / lambda / anonymous fn (unbound) | **NOT EMITTED** | `function_literal` is not matched. |
| closure bound to a variable | **NOT EMITTED** | `f := func(){}` is a variable with function-literal init, not a `function_declaration`. |
| object-literal method | **N/A** | not a Go construct. |
| prototype-assigned function | **N/A** | not a Go construct. |
| export-default anonymous fn | **N/A** | not a Go construct. |
| operator overload | **N/A** | Go does not allow user-defined operators. |
| async variants | **N/A** | Go has no `async` keyword; goroutines are call sites, not declarations. |
| generator functions | **N/A** | Go has no generator-function syntax. |

## Python

| callable kind | status | evidence / absence |
|---|---|---|
| free function | **EMITTED** | `function_definition` with `parent=None` (`src/graph/typegraph.rs:6571-6585`). |
| nested/local function | **EMITTED (as Free)** | `py_walk_call_defs` recurses into function bodies with `parent=None` (`src/graph/typegraph.rs:6587-6589`). |
| instance method | **EMITTED** | `function_definition` inside `class_definition` (`src/graph/typegraph.rs:6573-6575`). |
| static method | **EMITTED (as Method)** | `@staticmethod` / `@classmethod` `function_definition` inside a class is still emitted as Method (`src/graph/typegraph.rs:6565-6585`); not distinguished from instance methods. |
| constructor | **EMITTED (as Method)** | `__init__` inside a class is a `function_definition` (`src/graph/typegraph.rs:6573-6575`). |
| getter / setter | **EMITTED (share one Method sym)** | `@property` / `@x.setter` unwrap to `function_definition` (`py_unwrap_decorated` at `src/graph/typegraph.rs:6180-6186`) and emit as Method; same-name getter/setter mint the identical sym. |
| interface method declaration | **EMITTED (as Method)** | ABC abstract methods are `function_definition` inside a class, emitted as Method. |
| interface default body | **EMITTED (as Method)** | same as above. |
| interface impl method | **EMITTED (as Method)** | same as class method. |
| closure / lambda / anonymous fn (unbound) | **NOT EMITTED** | `lambda` expression is not matched. |
| closure bound to a variable | **NOT EMITTED** | `f = lambda ...` is an assignment, not matched. |
| object-literal method | **N/A** | not a Python construct. |
| prototype-assigned function | **N/A** | not a Python construct. |
| export-default anonymous fn | **N/A** | not a Python construct. |
| operator overload / dunder method | **EMITTED** | `__add__`, `__call__`, `__init__`, etc. inside a class are `function_definition` (`src/graph/typegraph.rs:6573-6575`). |
| async variants | **NOT EMITTED** | `py_walk_call_defs` matches only `function_definition`; `async_function_definition` has no arm. |
| generator functions | **EMITTED** | `def` containing `yield` is still `function_definition` (`src/graph/typegraph.rs:6571-6585`). |

## C status

- C is present in the tree-sitter language table at
  `src/engine/lang_tables.rs:8-10`, but its alias list is empty (`&[]`).
- There is no `CTypes` entry in `type_langs()`
  (`src/graph/typegraph.rs:443-445`), so there is no call-graph extractor for C.
- As a result, `.c` corpora produce zero `call_def` rows through the auto
  call-graph family.

## Model limits

- `EntityKind` has only two callable variants: `Function` and `Method`
  (`src/graph/typegraph.rs:32-40`). There is no distinct kind for constructors,
  accessors, closures, or static methods.
- `CallKind` adds `Closure` (`src/graph/typegraph.rs:282-286`), but no extractor
  currently emits it; closures/lambdas are lifted only as `closure` nodes in the
  dataflow graph.
- A getter and setter with the same name share one `Method` symbol. This is the
  same behavior already documented for the dataflow lift in
  `docs/df-coverage.md`.
- Static methods are not distinguished from instance methods; both become
  `Method` keyed to the owner type.
- Constructors are not a first-class kind. TS explicitly skips class
  constructors; Kotlin skips primary/secondary constructors; Python captures
  `__init__` as an ordinary method; Rust and Go have no constructor syntax.

## Summary matrix

| callable kind | Rust | TS/JS | Kotlin | Go | Python | C |
|---|---|---|---|---|---|---|
| free function | EMITTED | EMITTED | EMITTED | EMITTED | EMITTED | N/A |
| nested/local function | NOT EMITTED | NOT EMITTED | EMITTED (Free) | N/A | EMITTED (Free) | N/A |
| instance method | EMITTED | EMITTED | EMITTED | EMITTED | EMITTED | N/A |
| static method | EMITTED (Method) | EMITTED (Method) | N/A | N/A | EMITTED (Method) | N/A |
| constructor | N/A | NOT EMITTED | NOT EMITTED | N/A | EMITTED (Method) | N/A |
| getter / setter | N/A | EMITTED (share sym) | NOT EMITTED | N/A | EMITTED (share sym) | N/A |
| trait/interface method declaration | NOT EMITTED | NOT EMITTED | EMITTED (Free, no owner) | NOT EMITTED | EMITTED (Method) | N/A |
| trait/interface default body | NOT EMITTED | N/A | EMITTED (Free) | N/A | EMITTED (Method) | N/A |
| trait/interface impl method | EMITTED | EMITTED | EMITTED | EMITTED | EMITTED | N/A |
| closure / lambda / anonymous fn (unbound) | NOT EMITTED | NOT EMITTED | NOT EMITTED | NOT EMITTED | NOT EMITTED | N/A |
| closure bound to a variable | NOT EMITTED | EMITTED | NOT EMITTED | NOT EMITTED | NOT EMITTED | N/A |
| object-literal method | N/A | NOT EMITTED | N/A | N/A | N/A | N/A |
| prototype-assigned function | N/A | NOT EMITTED | N/A | N/A | N/A | N/A |
| export-default anonymous fn | N/A | NOT EMITTED | N/A | N/A | N/A | N/A |
| operator overload / dunder method | EMITTED | N/A | EMITTED | N/A | EMITTED | N/A |
| async variants | EMITTED | EMITTED | EMITTED | N/A | NOT EMITTED | N/A |
| generator functions | N/A | EMITTED | N/A | N/A | EMITTED | N/A |
