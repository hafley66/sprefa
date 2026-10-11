# Generics, traced on `fixtures/generic/0_eresult.dl7`

Every row below is `dl8 compile fixtures/generic/0_eresult.dl7` output,
printed short: `rnN` is `reader_node(file, N)`, `xrn0.K` is
`expansion_node(reader_node(file, 0), infix_colon, 1, K)`.

## Source

```lisp
(EResult:
   (* (Next: type)
      (Error: type)
      (return:
         (+ (loading: (*))
            (next: (* (value: Next)))
            (error: (* (message: Error)))
            (complete: (*))))))

(: FetchStatus (EResult str str))

(: Event (* (: status FetchStatus)))
(Event (FetchStatus.next "hi"))
(Event (FetchStatus.error "boom"))
(Event FetchStatus.loading)
```

```ts
type EResult<Next, Error> =
  | { tag: "loading" } | { tag: "next"; value: Next }
  | { tag: "error"; message: Error } | { tag: "complete" };
type FetchStatus = EResult<string, string>;
class Event { constructor(status: FetchStatus) {} }
new Event({ tag: "next", value: "hi" });
```

## Stage 1, lower: the bind becomes a pending application

`(: FetchStatus (EResult str str))` is a bind whose target is a product with a
`return` edge, so `generic_bind_edge` (`src/_2_lower/_5a_generic.rs:10`) writes

```
pending_edge(module, FetchStatus, application(EResult, [str, str]), 1)
```

`application_frame` (`:51`) accepts the head only when it is a generic
callable (`is_generic_callable`, `:27`: one `return` edge targeting an
`owner` expression), has no bounded parameter (`bounds`, `:321`), and the
argument count equals its arity. A bounded one builds by rule instead
(`bounded_item`, `:379`), behind `obligation` and `discharged` goals.

## Stage 2, check: the instance becomes ground `:` rows

`instance_rows` (`src/_3_check/_2a_instance.rs:23`) resolves the callee,
reads its `return` template, pairs the columns `Next`, `Error` with the
arguments `str`, `str`, and `walk`s the template (`:169`). Each template
edge becomes one `:` row on the instance node `application(EResult, [str, str])`;
a nested `(* ...)` becomes its own node `application(Inner, [str, str])`;
an edge naming a column takes that column's argument.

Rows in `compiler_rows`:

```
:(module,                          FetchStatus, application(EResult,[str,str]), 1)
:(application(EResult,[str,str]),  loading,     application(xrn0.21,[str,str]), 0)
:(application(EResult,[str,str]),  next,        application(xrn0.26,[str,str]), 1)
:(application(EResult,[str,str]),  error,       application(xrn0.35,[str,str]), 2)
:(application(EResult,[str,str]),  complete,    application(xrn0.40,[str,str]), 3)
:(application(xrn0.26,[str,str]),  value,       primitive(str),                  0)
:(application(xrn0.35,[str,str]),  message,     primitive(str),                  0)
```

`Next` and `Error` are gone: the `next` variant's `value` edge targets
`primitive(str)`. That is the substitution. It runs at check, once per
distinct argument list (`built` set, `:120`), so `(EResult str str)` named
twice is one node.

## Stage 3, values: a constructor call is an `intern` row

`(FetchStatus.next "hi")` lowers to the value term
`application(application(xrn0.26,[str,str]), ["hi"])` and two seeds
(`program.seeds`):

```
Event( application(application(xrn0.26,[str,str]), ["hi"]) )
intern(         application(xrn0.26,[str,str]), tuple("hi"), <that value> )
intern_snapshot(application(xrn0.26,[str,str]), tuple("hi"), <that value> )
```

So every constructed value carries its type node and its argument tuple, and
the type node carries `:` edges to each field's type.

## Where nothing checks

| stage | has | missing |
|---|---|---|
| check | the type node's edges: `value: primitive(str)` | the values: user seeds are in `program.seeds`, never in `compiler_rows` |
| comptime prelude | `column_type` (`prelude/3_derived_rules.dl7:24`) | a rule comparing a value against it |
| output program | `intern(Type, tuple(Args), Value)` per value | any comparison of `Args[i]` with `Type`'s edge `i` |

Probe: `plans/v8/probes/vec_generic/7_eresult_wrong_value.dl7` writes
`(FetchStatus.next 1)`; it compiles with zero diagnostics and seeds
`intern(application(xrn0.26,[str,str]), tuple(1), ...)`.

The same gap covers plain columns: `(seed: (* (value: int)))` with
`(seed "a")` is a `program.seeds` row against a `:` edge `value: int`
(`plans/v8/probes/prover/7_string_in_int_column.dl7`).

## The check, as rows

For a value: `intern(Type, Arguments, Value)` and `:(Type, Label, FieldType, Index)`
give one obligation per position, `Arguments[Index]` against `FieldType`.
For a seed row: `Relation(Values...)` and `:(Relation, Label, ColumnType, Index)`
give one per column. Both reduce to: a value, a type, a site.

| value kind | conforms to `primitive(int)` when | to a product or sum node when |
|---|---|---|
| `const(N)` int | always | never |
| `const("s")` | never (str) | never |
| `application(T, Args)` | never | `T` is the node, or a variant edge of it |

## Fork for Chris

Where the comparison runs; the rows above exist in every option.

| option | seeds reach it by | cost |
|---|---|---|
| A. comptime prelude rule (`AGENTS.md` 2026-09-17 row) | user seeds enter the comptime round as rows | every user seed row joins the compiler fixpoint |
| B. Rust pass after comptime over `program.seeds` and `compiler_rows` edges | already both in hand at `finish` | a Rust type pass, which the 2026-09-17 row ruled out before comptime only |
| C. rule in the output program, diagnostic as a runtime row | seeds are already there | a bad value is caught at `eval`/`run`, not at `compile` |
