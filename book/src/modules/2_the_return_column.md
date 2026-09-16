# The return column

[Example](#example) · [Option, Key and Partial](#option-key-and-partial) · [A declaration form is never a label](#a-declaration-form-is-never-a-label) · [Application in a declaration](#application-in-a-declaration) · [Rule](#rule) · [Diagnostics](#diagnostics) · [Receipts](#receipts)

## Example

`Wrap` is an ordinary product whose last column is named `return`. A rule interns the argument list, so `(Wrap text)` names one node.

```dl7
{{#include ../probes/0_return_column.dl7}}
```

The label of `Holder`'s edge is the node `(Wrap text)`:

```console
$ $DL8 compile book/src/probes/0_return_column.dl7 | jq -r --arg owner Holder '(.program.names | to_entries | map({key: (.value | tojson), value: .key}) | from_entries) as $n | def cell: if type != "object" then tostring elif $n[tojson] then $n[tojson] elif $n[{args: [.], f: "ref"} | tojson] then $n[{args: [.], f: "ref"} | tojson] elif .f == "const" then (.args[0].a // (.args[0] | tostring)) elif .f == "ref" then (.args[0] | cell) elif .f == "application" then "(" + ([.args[0] | cell] + (.args[1] | map(cell)) | join(" ")) + ")" elif .f == "module" and .args[0].a then "module(\(.args[0].a))" elif .f == "module" then "module(\(.args[0].f)(\(.args[0].args[0].a | sub("^\($ENV.PWD)/"; ""))))" elif .f and (.args[0].a) then "\(.f)(\(.args[0].a))" else "?" end; .compiler_rows[] | select(.f == "call" and .args[0].args[0].args[0].a == ":") | .args[1] | select((.[0] | cell) == $owner) | "(: \(.[0] | cell) \(.[1] | cell) \(.[2] | cell))"'
(: Holder (Wrap primitive(text)) primitive(int))
```

The same shape with the column named `target`:

```dl7
{{#include ../probes/1_no_return_column.dl7}}
```

```console
$ bash book/show.sh compile book/src/probes/1_no_return_column.dl7
diagnostic diagnostic(lower, reader_node(book/src/probes/1_no_return_column.dl7, 20), expression_without_return(target(owner(file(book/src/probes/1_no_return_column.dl7), reader_node(book/src/probes/1_no_return_column.dl7, 3)))))
exit 1
```

```
step 0  (Wrap text) in label position    callable=target(Wrap)  arity=2
step 1  return indices of Wrap           [1]                    one: the expression's value is column 1
step 2  input slots                      [0]                    text fills column 0
step 3  goal (Wrap text ?Label)          the intern rule binds ?Label to one node
step 1' (Pair text)                      []                     none: expression_without_return
```

## Option, Key and Partial

The prelude declares them the way `Wrap` is declared; a user product of the same name hides them ([Names](1_names.md#shadowing)).

| prelude relation | declaration | intern rule |
|---|---|---|
| `Partial` | `prelude/0_constructors.dl7:2-4` | `prelude/2_constructor_rules.dl7:2-10` |
| `Option` | `prelude/0_constructors.dl7:6-8` | `prelude/2_constructor_rules.dl7:12-20` |
| `Key` | `prelude/1_declarations.dl7:53-56` | `prelude/2_constructor_rules.dl7:22-32` |

## A declaration form is never a label

`(: a 5)` in label position is a goal on the kernel relation `:`, which has no arm in `kernel_return_positions` (`_9_kernel.rs:73-79`).

```dl7
{{#include ../probes/2_colon_form_label.dl7}}
```

```console
$ bash book/show.sh compile book/src/probes/2_colon_form_label.dl7
diagnostic diagnostic(lower, reader_node(book/src/probes/2_colon_form_label.dl7, 7), expression_without_return(kernel(:)))
exit 1
```

## Application in a declaration

A full application on the right of a declaration is its return column's node:

```dl7
{{#include ../probes/20_full_application_bind.dl7}}
```

```console
$ $DL8 compile book/src/probes/20_full_application_bind.dl7 | jq -r --arg owner 'module(file(book/src/probes/20_full_application_bind.dl7))' '(.program.names | to_entries | map({key: (.value | tojson), value: .key}) | from_entries) as $n | def cell: if type != "object" then tostring elif $n[tojson] then $n[tojson] elif $n[{args: [.], f: "ref"} | tojson] then $n[{args: [.], f: "ref"} | tojson] elif .f == "const" then (.args[0].a // (.args[0] | tostring)) elif .f == "ref" then (.args[0] | cell) elif .f == "application" then "(" + ([.args[0] | cell] + (.args[1] | map(cell)) | join(" ")) + ")" elif .f == "module" and .args[0].a then "module(\(.args[0].a))" elif .f == "module" then "module(\(.args[0].f)(\(.args[0].args[0].a | sub("^\($ENV.PWD)/"; ""))))" elif .f and (.args[0].a) then "\(.f)(\(.args[0].a))" else "?" end; .compiler_rows[] | select(.f == "call" and .args[0].args[0].args[0].a == ":") | .args[1] | select((.[0] | cell) == $owner) | "(: \(.[0] | cell) \(.[1] | cell) \(.[2] | cell))"' | grep WrappedText
(: module(file(book/src/probes/20_full_application_bind.dl7)) WrappedText (Wrap primitive(text)))
```

A top-level named bind with fewer arguments curries: `(: PairUser (Pair User))` then `(PairUser Order PairResult)` (`oracle/compile/sources/test/fixtures/5_curry.dl7:12-15`), and a relation may return a callable (`5_curry.dl7:31-41`). The curried name here resolves through the prelude alias, a second module:

```dl7
{{#include ../probes/22_curry_prelude_name.dl7}}
```

```console
$ $DL8 compile book/src/probes/22_curry_prelude_name.dl7 | jq -r --arg owner KeyName '(.program.names | to_entries | map({key: (.value | tojson), value: .key}) | from_entries) as $n | def cell: if type != "object" then tostring elif $n[tojson] then $n[tojson] elif $n[{args: [.], f: "ref"} | tojson] then $n[{args: [.], f: "ref"} | tojson] elif .f == "const" then (.args[0].a // (.args[0] | tostring)) elif .f == "ref" then (.args[0] | cell) elif .f == "application" then "(" + ([.args[0] | cell] + (.args[1] | map(cell)) | join(" ")) + ")" elif .f == "module" and .args[0].a then "module(\(.args[0].a))" elif .f == "module" then "module(\(.args[0].f)(\(.args[0].args[0].a | sub("^\($ENV.PWD)/"; ""))))" elif .f and (.args[0].a) then "\(.f)(\(.args[0].a))" else "?" end; .compiler_rows[] | select(.f == "call" and .args[0].args[0].args[0].a == ":") | .args[1] | select((.[0] | cell) == $owner) | "(: \(.[0] | cell) \(.[1] | cell) \(.[2] | cell))"'
(: KeyName options primitive(type))
(: KeyName return primitive(type))
```

`KeyName` keeps the `options` and `return` columns of `Key`; `name` is captured. Currying exists as a named bind, not as a field type. No `v6/prolog/conformance/rulings.pl` row names partial application in a field; the nearest, `rulings.pl:700-708`, quarantines currying for initial use. A partial application as a field type stops:

```dl7
{{#include ../probes/21_partial_in_field.dl7}}
```

```console
$ bash book/show.sh compile book/src/probes/21_partial_in_field.dl7
diagnostic diagnostic(lower, expansion_node(reader_node(book/src/probes/21_partial_in_field.dl7, 19), infix_colon, 1, 0), partial_application_requires_more_arguments(partial))
exit 1
```

## Rule

- A product declares `relation(Owner, Arity, KeySets)`: the arity is its edge count; exactly one edge named `return` makes one key set of every other position (`src/_2_lower/_2_declare.rs:278-306`).
- An expression needs exactly one return position. A product reads its `return` edges; a kernel name reads `kernel_return_positions` (`src/_2_lower/_8_express.rs:213-242`, `src/_2_lower/_9_kernel.rs:72-80`).
- None is `expression_without_return`; more than one is `expression_multiple_returns(Callable, Indices)` (`_8_express.rs:235-239`).

## Diagnostics

| diagnostic | raised at | fixture |
|---|---|---|
| `expression_without_return(Callable)` | `src/_2_lower/_8_express.rs:231-233` | no fixture; probe only: `probes/1_no_return_column.dl7`, `2_colon_form_label.dl7` |
| `partial_application_requires_more_arguments(Label)` | `src/_2_lower/_6_partial.rs:35-55` | no fixture; probe only: `probes/21_partial_in_field.dl7` |

## Receipts

| claim | path | command |
|---|---|---|
| atom and compound labels resolve to the same target | `oracle/compile/sources/test/fixtures/binding_symmetry/0_atom_label_expression_target.dl7`, `1_compound_label_expression_target.dl7` | `cargo test --test _8_compile_oracle` |
| the probes on this page | `book/src/probes/0_`, `1_`, `2_`, `20_`, `21_`, `22_*.dl7` | `cargo test --test _22_book probes_compile_as_their_page_says` |
