# The return column

[Example](#example) · [Option, Key and Partial](#option-key-and-partial) · [A declaration form is never a label](#a-declaration-form-is-never-a-label) · [Rule](#rule) · [Receipts](#receipts)

## Example

`Wrap` is an ordinary product whose last column is named `return`. A rule interns the argument list, so `(Wrap text)` names one node.

```dl7
{{#include ../probes/0_return_column.dl7}}
```

The label of `Holder`'s edge is the node `(Wrap text)`:

```console
$ $DL8 compile book/src/probes/0_return_column.dl7 | jq -r --arg owner Holder '(.program.names | to_entries | map({key: (.value | tojson), value: .key}) | from_entries) as $n | def cell: if $n[tojson] then $n[tojson] elif $n[{args: [.], f: "ref"} | tojson] then $n[{args: [.], f: "ref"} | tojson] elif .f == "const" then (.args[0].a // (.args[0] | tostring)) elif .f == "ref" then (.args[0] | cell) elif .f == "application" then "(" + ([.args[0] | cell] + (.args[1] | map(cell)) | join(" ")) + ")" elif .f == "module" and .args[0].a then "module(\(.args[0].a))" elif .f == "module" then "module(\(.args[0].f)(\(.args[0].args[0].a | sub("^.*/v8/"; ""))))" elif .f and (.args[0].a) then "\(.f)(\(.args[0].a))" else "?" end; .compiler_rows[] | select(.f == "call" and .args[0].args[0].args[0].a == ":") | .args[1] | select((.[0] | cell) == $owner) | "(: \(.[0] | cell) \(.[1] | cell) \(.[2] | cell))"'
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
```

For `Pair`, step 1 finds `[]` and stops with `expression_without_return`.

## Option, Key and Partial

The prelude declares them the way `Wrap` is declared.

```console
$ sed -n 6,8p prelude/0_constructors.dl7 && sed -n 12,15p prelude/2_constructor_rules.dl7
(: Option
   (* (: source type)
      (: return type)))
(<- (Option ?Source ?Result)
    (nil ?Empty)
    (cons ?Source ?Empty ?Arguments)
    (intern Option ?Arguments ?Result))
```

```console
$ $DL8 compile oracle/compile/sources/test/fixtures/binding_symmetry/0_atom_label_expression_target.dl7 | jq -r --arg owner Holder '(.program.names | to_entries | map({key: (.value | tojson), value: .key}) | from_entries) as $n | def cell: if $n[tojson] then $n[tojson] elif $n[{args: [.], f: "ref"} | tojson] then $n[{args: [.], f: "ref"} | tojson] elif .f == "const" then (.args[0].a // (.args[0] | tostring)) elif .f == "ref" then (.args[0] | cell) elif .f == "application" then "(" + ([.args[0] | cell] + (.args[1] | map(cell)) | join(" ")) + ")" elif .f == "module" and .args[0].a then "module(\(.args[0].a))" elif .f == "module" then "module(\(.args[0].f)(\(.args[0].args[0].a | sub("^.*/v8/"; ""))))" elif .f and (.args[0].a) then "\(.f)(\(.args[0].a))" else "?" end; .compiler_rows[] | select(.f == "call" and .args[0].args[0].args[0].a == ":") | .args[1] | select((.[0] | cell) == $owner) | "(: \(.[0] | cell) \(.[1] | cell) \(.[2] | cell))"'
(: Holder direct (Option primitive(text)))
```

| prelude relation | declaration | intern rule |
|---|---|---|
| `Partial` | `prelude/0_constructors.dl7:2-4` | `prelude/2_constructor_rules.dl7:2-10` |
| `Option` | `prelude/0_constructors.dl7:6-8` | `prelude/2_constructor_rules.dl7:12-20` |
| `Key` | `prelude/1_declarations.dl7:53-56` | `prelude/2_constructor_rules.dl7:22-32` |

A user product with the same name and no `return` column hides the prelude one: [Names](1_names.md#shadowing).

## A declaration form is never a label

`(: a 5)` in label position is a goal on the kernel relation `:`, which has no return position.

```dl7
{{#include ../probes/2_colon_form_label.dl7}}
```

```console
$ bash book/show.sh compile book/src/probes/2_colon_form_label.dl7
diagnostic diagnostic(lower, reader_node(book/src/probes/2_colon_form_label.dl7, 7), expression_without_return(kernel(:)))
exit 1
```

## Rule

- A product declares `relation(Owner, Arity, KeySets)`: the arity is its edge count; exactly one edge named `return` makes one key set of every other position (`src/_2_lower/_2_declare.rs:278-306`).
- An expression needs exactly one return position. A product reads its `return` edges; a kernel name reads `kernel_return_positions` (`src/_2_lower/_8_express.rs:213-242`, `src/_2_lower/_9_kernel.rs:72-80`).
- None is `expression_without_return(Callable)` (`_8_express.rs:231-233`); more than one is `expression_multiple_returns(Callable, Indices)` (`:235-239`).
- The return slot leaves the input slots; the remaining arguments fill the rest by position or by field name (`_8_express.rs:246-276`).
- `:` has no arm in `kernel_return_positions`, so a `(: ...)` form in an expression position always stops (`_9_kernel.rs:73-79`).

## Receipts

| claim | path | command |
|---|---|---|
| atom and compound labels resolve to the same target | `oracle/compile/sources/test/fixtures/binding_symmetry/0_atom_label_expression_target.dl7`, `1_compound_label_expression_target.dl7` | `cargo test --test _8_compile_oracle` |
| generated callables as targets | `binding_symmetry/8_generated_callable_targets.dl7`, `9_saturated_generated_targets.dl7` | `cargo test --test _8_compile_oracle` |
| a bind to a call is a relation | [Modules and application](../9_modules.md) | `bash book/show.sh eval oracle/compile/sources/test/fixtures/4_generated_call.dl7` |
| the probes on this page | `book/src/probes/0_return_column.dl7`, `1_no_return_column.dl7`, `2_colon_form_label.dl7` | `cargo test --test _22_book probes_compile_as_their_page_says` |
