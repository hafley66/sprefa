# A module is a file

[Example](#example) · [The module term](#the-module-term) · [Rule](#rule) · [Receipts](#receipts)

## Example

Two files under one project root, `book/src/probes`.

```dl7
{{#include ../probes/10_accounts.dl7}}
```

```dl7
{{#include ../probes/11_consumer.dl7}}
```

Both files, one project: the goal `(: accounts User ?UserType ?Index)` reads the `User` edge of the module `accounts`.

```console
$ bash book/show.sh compile book/src/probes/11_consumer.dl7 --project book/src/probes book/src/probes/10_accounts.dl7
(found_user User)
exit 0
```

The consumer alone: no project, so no module named `accounts`.

```console
$ bash book/show.sh compile book/src/probes/11_consumer.dl7
diagnostic diagnostic(check, none, unresolved_name(accounts))
exit 1
```

## The module term

A project installs a graph of module owners. The root directory is `module(directory(Root))`; each file is `module(file(Path))`; the prelude is `module(prelude)`. The root owns one edge per file, labeled with the file's module name.

```console
$ $DL8 compile book/src/probes/11_consumer.dl7 --project book/src/probes book/src/probes/10_accounts.dl7 | jq -r --arg owner 'module(directory(book/src/probes))' '(.program.names | to_entries | map({key: (.value | tojson), value: .key}) | from_entries) as $n | def cell: if $n[tojson] then $n[tojson] elif $n[{args: [.], f: "ref"} | tojson] then $n[{args: [.], f: "ref"} | tojson] elif .f == "const" then (.args[0].a // (.args[0] | tostring)) elif .f == "ref" then (.args[0] | cell) elif .f == "application" then "(" + ([.args[0] | cell] + (.args[1] | map(cell)) | join(" ")) + ")" elif .f == "module" and .args[0].a then "module(\(.args[0].a))" elif .f == "module" then "module(\(.args[0].f)(\(.args[0].args[0].a | sub("^.*/v8/"; ""))))" elif .f and (.args[0].a) then "\(.f)(\(.args[0].a))" else "?" end; .compiler_rows[] | select(.f == "call" and .args[0].args[0].args[0].a == ":") | .args[1] | select((.[0] | cell) == $owner) | "(: \(.[0] | cell) \(.[1] | cell) \(.[2] | cell))"'
(: module(directory(book/src/probes)) accounts accounts)
(: module(directory(book/src/probes)) consumer consumer)
```

The goal's name `accounts` resolves by walking owners:

```
step 0  owner=module(file(11_consumer.dl7))       name=accounts  forward edge: none
step 1  owner=module(directory(book/src/probes))  name=accounts  forward edge -> module(file(10_accounts.dl7)), commit
step 2  goal (: module(file(10_accounts.dl7)) User ?UserType ?Index) matches the edge  ?UserType=User
```

Steady state: one row, `(found_user User)`.

## Rule

- `dl8 compile a.dl7 b.dl7 --project Root` loads one unit per path, in the order given (`src/_8_driver/_4_project.rs:28-56`). More than one file, or `--project`, selects the project door (`src/bin/dl8.rs:448-458`).
- A module name is the path relative to the root, split on `/`, each part losing a leading `<digits>_` (`src/_4_comptime/_0_load/_2_project.rs:162-193`). `10_accounts.dl7` is `accounts`; a file in `billing/` is the edge `billing` on the root, then the edge `accounts` on the directory (`:195-247`).
- The owners are `module(directory(Path))` and `module(file(Path))` (`_2_project.rs:265-273`).
- The whole project graph installs or nothing does: any path diagnostic returns the caller's graph untouched (`_2_project.rs:4-5`, `:42-48`).
- A name in a goal walks owner, then parent, then the kernel, then the primitives; a forward edge commits (`src/_3_check/_2_resolve.rs:45-90`). [Names](1_names.md) covers the walk.
- Every non-prelude module receives an alias of each prelude edge it does not bind itself (`src/_2_lower/_12_units.rs:310-342`, exporter chosen at `:475-492`). A user module receives no alias from another user module.

| path diagnostic | when | line |
|---|---|---|
| `invalid_project_unit(Unit)` | the unit is not `dl7_unit/5` | `_2_project.rs:109-115` |
| `project_unit_without_file_origin` | the unit's origin is not `file(Path)` | `_2_project.rs:117-123` |
| `outside_project_root(Root, Path)` | the relative path starts with `..` | `_2_project.rs:128-135` |
| `invalid_dl7_module_path(Relative)` | no `.dl7` extension, or an empty stem | `_2_project.rs:137-143` |

A root one directory away from the files:

```console
$ bash book/show.sh compile book/src/probes/11_consumer.dl7 --project oracle/check book/src/probes/10_accounts.dl7
diagnostic diagnostic(module, filesystem(book/src/probes/11_consumer.dl7), outside_project_root(oracle/check, book/src/probes/11_consumer.dl7))
diagnostic diagnostic(module, filesystem(book/src/probes/10_accounts.dl7), outside_project_root(oracle/check, book/src/probes/10_accounts.dl7))
exit 1
```

A root that is a sibling of the files' directory is inside. `relative_file_name/3` reads the root as a file and climbs one level fewer (`src/_4_comptime/_0_load/_1_paths.rs:36-54`, `up.saturating_sub(1)` at `:47`), the v7 behavior it ports:

```console
$ bash book/show.sh compile book/src/probes/11_consumer.dl7 --project fixtures book/src/probes/10_accounts.dl7
(found_user User)
exit 0
```

A bare `User` in the consumer is not an alias of the other file's `User`:

```dl7
{{#include ../probes/18_bare_consumer.dl7}}
```

```console
$ bash book/show.sh compile book/src/probes/18_bare_consumer.dl7 --project book/src/probes book/src/probes/10_accounts.dl7
diagnostic diagnostic(lower, reader_node(book/src/probes/18_bare_consumer.dl7, 14), undeclared_relation(User))
exit 1
```

## Receipts

| claim | path | command |
|---|---|---|
| both module consumers equal v7 under `--project` | `oracle/compile/cases/project-modules-consumer.json`, `project-modules-plus.json` | `cargo test --test _8_compile_oracle` |
| the fixture pair this page's probes mirror | `oracle/compile/sources/test/fixtures/modules/0_accounts.dl7`, `1_consumer.dl7:5` | [Modules and application](../9_modules.md) |
| every probe compiles as its first line says | `book/src/probes/10_accounts.dl7`, `11_consumer.dl7` | `cargo test --test _22_book probes_compile_as_their_page_says` |
| the prefix rule | `_2_project.rs:176-193` `semantic_segment` | `sed -n 176,193p src/_4_comptime/_0_load/_2_project.rs` |
