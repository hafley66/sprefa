# Declarations

## What

```mermaid
flowchart LR
  dl7[file.dl7] -->|dl8 read| reader[_0_read/_2_reader.rs: literals]
  reader --> range[integer_out_of_range]
  reader -->|dl8 lower| declare[_2_lower/_2_declare.rs]
  declare -->|"(* ...)"| product[product row, one relation of field arity]
  declare -->|"(+ ...)"| sum[sum row, no relation]
  declare -->|return| key[every other position is the key]
  product --> colon[: rows, the type graph]
  sum --> colon
  key --> colon
  prim[_3_check/_5_kernel.rs: int float bool text any type] -->|primitive_name| colon
  prelude[prelude/5_tsi_primitives.dl7: string] --> colon
  colon -->|dl8 check| unresolved[unresolved_name naming the field]
```

`(: Name (* (: field type) ...))` declares a product: a node, a `product` row, one edge per field, and one relation whose arity is the field count (`src/_2_lower/_2_declare.rs:189-194`, `:268-306`).
`(: Name (+ (: variant type) ...))` declares a sum: the node, a `sum` row and the edges, and no relation (`_2_declare.rs:268` "A sum declares no relation").
A field type is a primitive, a declared name, a literal, or an application such as `(Option text)`.
The primitive names are `int float bool text any type` (`src/_3_check/_5_kernel.rs:40-42`); `string` is a prelude product `(: string (* ))` (`prelude/5_tsi_primitives.dl7:6`).
Literals: 64-bit integers, decimal floats, `true`, `false`, and double-quoted text (`src/_0_read/_2_reader.rs:376-397`). A wider integer is `integer_out_of_range` (`_2_reader.rs:383`).
A product field whose name is `return` makes every other position the key (`_2_declare.rs:292-303`).

## Why

`plans/v8/2026-09-13-v8-tour.md` section 5: `(: owner name target index)` is the one row every declaration lowers to, so a declaration is data a rule can read ([Terms and the graph](5_terms.md)).
No written decision names why a sum carries no relation; the source line is `_2_declare.rs:268`.

## When to use

Use it when:

- a relation holds rows: a product, `fixtures/store/0_round_trip.dl7`
- a column holds values of mixed kinds: `any`, `fixtures/term_lt/0_mixed_kinds.dl7`
- a column holds a type or a node: `type`, `oracle/compile/sources/test/fixtures/2_partial.dl7`
- a value is one of several shapes: a sum, `oracle/compile/sources/test/fixtures/16_interned_storage.dl7:42-47`

Do not use it when:

- a sum should hold rows: it declares no relation, use a product per variant
- a field names a type nobody declared: `unresolved_name`, [Diagnostics](14_diagnostics.md)

## Example

Every scalar kind in one product:

```dl7
; fixture: fixtures/store/0_round_trip.dl7
(: Reading (* (: name text) (: value float) (: ok bool) (: count int)))

(Reading "a" 1.5 true 3)

(Reading "b" -0.25 false 4)

(: Copied (* (: name text) (: value float)))

(<- (Copied ?Name ?Value)
    (Reading ?Name ?Value ?Ok ?Count))
```

```console
$ bash book/show.sh eval fixtures/store/0_round_trip.dl7
(Reading "a" 1.5 true 3)
(Reading "b" -0.25 false 4)
(Copied "a" 1.5)
(Copied "b" -0.25)
exit 0
```

A sum, and a product whose field is that sum:

```dl7
; fixture: oracle/compile/sources/test/fixtures/16_interned_storage.dl7:40-47
; Sum fixture verifies that reachable variants use the same ordered field and
; dependency protocol as products.
(: SpanChoice
   (+ (: first Span)
      (: second Span)))

(: ChoiceRoot
   (* (: choice SpanChoice)))
```

```console
$ $DL8 compile oracle/compile/sources/test/fixtures/16_interned_storage.dl7 | jq -c '[.compiler_rows[] | select(.args[0].args[0].f == "kernel") | .args[0].args[0].args[0].a] | group_by(.) | map([.[0], length])'
[[":",612],["module",2],["nil",1],["node",164],["product",159],["sum",1]]
```

A field type nobody declared:

```dl7
; fixture: oracle/compile/sources/test/fixtures/lexical_binding/2_missing_name.dl7
; diagnostic: unresolved_name
(: Holder
   (* (: field Missing)))
```

```console
$ bash book/show.sh compile oracle/compile/sources/test/fixtures/lexical_binding/2_missing_name.dl7
diagnostic diagnostic(check, reader_node(oracle/compile/sources/test/fixtures/lexical_binding/2_missing_name.dl7, 5), unresolved_name(field))
exit 1
```

## What proves it

| claim | path | command |
|---|---|---|
| a float, a bool and text round-trip through a product | `fixtures/store/0_round_trip.expected.json` | `cargo test --test _13_store reopening_the_db_leaves_the_closure_identical` |
| float and bool literals | `fixtures/literals/0_float.expected.json`, `1_bool.expected.json` | `cargo test --test _10_literals` |
| a sum declares no relation | `src/_2_lower/_2_declare.rs:268-277` | `sed -n 268,277p src/_2_lower/_2_declare.rs` |
| six primitive names | `src/_3_check/_5_kernel.rs:40-42` | `grep -n primitive_name src/_3_check/_5_kernel.rs` |
| an undeclared field type is `unresolved_name` naming the field | `oracle/check/cases/0_unresolved_name.dl7`, `oracle/check/status.json` | `cargo test --test _4_check_oracle` |
