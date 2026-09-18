# Names

[Example](#example) · [Two resolvers](#two-resolvers) · [The kernel names](#the-kernel-names) · [Shadowing](#shadowing) · [Receipts](#receipts)

## Example

A field named `Name` inside `Holder` hides the top-level `Name`.

```console
$ cat oracle/compile/sources/test/fixtures/lexical_binding/7_nearest_shadow.dl7
(: Name (Option str))
(: Shadow (*))
(: Holder
   (* (: Name Shadow)
      (: direct Name)
      (: deep direct)))
```

```console
$ $DL8 compile oracle/compile/sources/test/fixtures/lexical_binding/7_nearest_shadow.dl7 | jq -r --arg owner Holder '(.program.names | to_entries | map({key: (.value | tojson), value: .key}) | from_entries) as $n | def cell: if type != "object" then tostring elif $n[tojson] then $n[tojson] elif $n[{args: [.], f: "ref"} | tojson] then $n[{args: [.], f: "ref"} | tojson] elif .f == "const" then (.args[0].a // (.args[0] | tostring)) elif .f == "ref" then (.args[0] | cell) elif .f == "application" then "(" + ([.args[0] | cell] + (.args[1] | map(cell)) | join(" ")) + ")" elif .f == "module" and .args[0].a then "module(\(.args[0].a))" elif .f == "module" then "module(\(.args[0].f)(\(.args[0].args[0].a | sub("^\($ENV.PWD)/"; ""))))" elif .f and (.args[0].a) then "\(.f)(\(.args[0].a))" else "?" end; .compiler_rows[] | select(.f == "call" and .args[0].args[0].args[0].a == ":") | .args[1] | select((.[0] | cell) == $owner) | "(: \(.[0] | cell) \(.[1] | cell) \(.[2] | cell))"'
(: Holder Name Shadow)
(: Holder deep Shadow)
(: Holder direct Shadow)
```

The target of `direct` resolves by walking owners:

```
step 0  owner=Holder              name=Name    forward edge (: Holder Name Shadow), commit; resolve Shadow
step 1  owner=Holder              name=Shadow  no forward edge; parent
step 2  owner=module(file(...))   name=Shadow  forward edge -> the product Shadow
```

`deep` names `direct`, which walks the same steps: `(: Holder deep Shadow)`. The top-level `Name`, `(Option str)`, is never reached.

## Two resolvers

| position in the source | resolver | order walked | a miss |
|---|---|---|---|
| a goal head `(Name ...)`, a label call, an expression target | `expression_callable`, `src/_2_lower/_8_express.rs:269-326` | scoped reservation owner then parents (`src/_2_lower/_10_index.rs:170-193`), then a kernel name (`_8_express.rs:210-213`) | `undeclared_relation`, `not_relation` |
| a bare name as a value: a field target, the owner of a `(: ...)` goal | `resolve_name`, `src/_3_check/_2_resolve.rs:47-90` | forward edge, which commits (`:70-72`); parent (`:73-77`); kernel name, then primitive, only at a module owner (`:78-88`) | `unresolved_name` |

A module owner's forward edges hold its own declarations plus one alias per prelude name it does not bind ([A module is a file](0_a_module_is_a_file.md)). The prelude therefore sits between the file and the kernel:

```console
$ $DL8 compile book/src/probes/11_consumer.dl7 --project book/src/probes book/src/probes/10_accounts.dl7 | jq -r --arg owner accounts '(.program.names | to_entries | map({key: (.value | tojson), value: .key}) | from_entries) as $n | def cell: if type != "object" then tostring elif $n[tojson] then $n[tojson] elif $n[{args: [.], f: "ref"} | tojson] then $n[{args: [.], f: "ref"} | tojson] elif .f == "const" then (.args[0].a // (.args[0] | tostring)) elif .f == "ref" then (.args[0] | cell) elif .f == "application" then "(" + ([.args[0] | cell] + (.args[1] | map(cell)) | join(" ")) + ")" elif .f == "module" and .args[0].a then "module(\(.args[0].a))" elif .f == "module" then "module(\(.args[0].f)(\(.args[0].args[0].a | sub("^\($ENV.PWD)/"; ""))))" elif .f and (.args[0].a) then "\(.f)(\(.args[0].a))" else "?" end; .compiler_rows[] | select(.f == "call" and .args[0].args[0].args[0].a == ":") | .args[1] | select((.[0] | cell) == $owner) | "(: \(.[0] | cell) \(.[1] | cell) \(.[2] | cell))"' | grep -E ' (User|Option|bool) '
(: accounts Option Option)
(: accounts User User)
(: accounts bool bool)
```

Order for a bare name, nearest first: the enclosing products, the file, the prelude alias, the kernel, the primitives.

## The kernel names

`KERNEL_RELATIONS` (`src/_3_check/_5_kernel.rs:16-39`) lists every row of the table except `def`, `head` and `body`; the checker appends those three as rows and nodes (`:107-111`, `:173`), and the lowerer's `kernel_relation` names all of them (`src/_2_lower/_9_kernel.rs:16-31`).

| name | arity | return position | lowerer keys | source |
|---|---|---|---|---|
| `node`, `module`, `product`, `sum` | 1 | none | none | `_9_kernel.rs:19` |
| `nil` | 1 | 0 | `[[]]` | `_9_kernel.rs:19`, `:67`, `:79` |
| `:`, `edge_snapshot` | 4 | none | `[0,1]`, `[0,3]` | `_9_kernel.rs:20`, `:66` |
| `body` | 4 | none | none | `_9_kernel.rs:20` |
| `cons` | 3 | 2 | `[0,1]`, `[2]` | `_9_kernel.rs:21`, `:68`, `:80` |
| `edge_ref`, `intern`, `intern_snapshot` | 3 | 2 | `[0,1]` | `_9_kernel.rs:21`, `:69`, `:81` |
| `effect` | 2 | none | `[0,1]` | `_9_kernel.rs:22`, `:70` |
| `def`, `head` | 2 | none | none | `_9_kernel.rs:22` |
| `int.add` | 3 | 2 | `[0,1]` | `_9_kernel.rs:23`, `:70`, `:82` |
| `any.lt` | 2 | none | `[0,1]` | `_9_kernel.rs:24`, `:70` |
| `str.cons` | 3 | 2 | `[0,1]`, `[2]` | `_9_kernel.rs:25`, `:68`, `:80` |
| `str.nil` | 1 | 0 | `[[]]` | `_9_kernel.rs:26`, `:67`, `:79` |
| `int.lt`, `int.le`, `int.gt`, `int.ge`, `int.eq`, `int.ne` | 2 | none | `[0,1]` | `_9_kernel.rs:9`, `:27`, `:71` |

Primitive names: `int`, `float`, `bool`, `str`, `any`, `type` (`src/_3_check/_5_kernel.rs:56-58`).

## Shadowing

A user product named `intern` hides the kernel `intern` for every rule in that file:

```dl7
{{#include ../probes/4_user_intern_kernel_goal.dl7}}
```

```console
$ bash book/show.sh compile book/src/probes/4_user_intern_kernel_goal.dl7
diagnostic diagnostic(lower, reader_node(book/src/probes/4_user_intern_kernel_goal.dl7, 36), arity_mismatch(intern, 1, 3))
exit 1
```

A user `Option` hides the prelude `Option`; the user's has no `return` column ([The return column](2_the_return_column.md)):

```dl7
{{#include ../probes/5_user_option_shadows.dl7}}
```

```console
$ bash book/show.sh compile book/src/probes/5_user_option_shadows.dl7
diagnostic diagnostic(lower, reader_node(book/src/probes/5_user_option_shadows.dl7, 17), expression_without_return(target(owner(file(book/src/probes/5_user_option_shadows.dl7), reader_node(book/src/probes/5_user_option_shadows.dl7, 3)))))
exit 1
```

A field typed `bool` and a field typed `int` both reach the primitive. `@std/tsi` declares a product `bool` (`std/tsi.dl7:55`), and a program reaches it only as `tsi.bool` after `(tsi: (import "@std/tsi"))`:

```dl7
{{#include ../probes/17_bool_field.dl7}}
```

```console
$ $DL8 compile book/src/probes/17_bool_field.dl7 | jq -r --arg owner Holder '(.program.names | to_entries | map({key: (.value | tojson), value: .key}) | from_entries) as $n | def cell: if type != "object" then tostring elif $n[tojson] then $n[tojson] elif $n[{args: [.], f: "ref"} | tojson] then $n[{args: [.], f: "ref"} | tojson] elif .f == "const" then (.args[0].a // (.args[0] | tostring)) elif .f == "ref" then (.args[0] | cell) elif .f == "application" then "(" + ([.args[0] | cell] + (.args[1] | map(cell)) | join(" ")) + ")" elif .f == "module" and .args[0].a then "module(\(.args[0].a))" elif .f == "module" then "module(\(.args[0].f)(\(.args[0].args[0].a | sub("^\($ENV.PWD)/"; ""))))" elif .f and (.args[0].a) then "\(.f)(\(.args[0].a))" else "?" end; .compiler_rows[] | select(.f == "call" and .args[0].args[0].args[0].a == ":") | .args[1] | select((.[0] | cell) == $owner) | "(: \(.[0] | cell) \(.[1] | cell) \(.[2] | cell))"'
(: Holder count primitive(int))
(: Holder flag primitive(bool))
```

A bare `str` reaches the primitive the same way; [Application in a declaration](2_the_return_column.md#application-in-a-declaration) shows the compiled shape.

## Receipts

| claim | path | command |
|---|---|---|
| nearest wins, chains, cycles, missing names equal v7 | `oracle/compile/sources/test/fixtures/lexical_binding/*.dl7` | `cargo test --test _8_compile_oracle` |
| declaration order and label spelling do not change resolution | `oracle/compile/sources/test/fixtures/binding_symmetry/3_declaration_order.dl7`, `10_infix_colon_spelling.dl7` | `cargo test --test _8_compile_oracle` |
| a missing name in a field | `oracle/check/cases/0_unresolved_name.dl7` | `bash book/show.sh compile oracle/check/cases/0_unresolved_name.dl7` |
| a cycle of names | [Modules and application](../9_modules.md) | `bash book/show.sh compile oracle/compile/sources/test/fixtures/lexical_binding/4_two_cycle.dl7` |
| every probe on this page | `book/src/probes/4_user_intern_kernel_goal.dl7`, `5_user_option_shadows.dl7`, `17_bool_field.dl7` | `cargo test --test _22_book probes_compile_as_their_page_says` |
