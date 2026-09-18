# The prelude

[Example](#example) · [The six files](#the-six-files) · [Macrotime and `<+`](#macrotime-and-) · [Rule](#rule) · [Receipts](#receipts)

## Example

A program that declares only `Holder` still has the prelude's names:

```console
$ printf '(: Holder\n   (* (: direct (Option str))))\n' > /tmp/dl8-prelude-probe.dl7 && $DL8 compile /tmp/dl8-prelude-probe.dl7 | jq -r '.program.names | keys[] | select(. == "Option" or . == "Key" or . == "Conforms" or . == "string" or . == "Holder")'
Conforms
Holder
Key
Option
string
```

`Holder` is the program's; the other four come from four different prelude files.

## The six files

Each file is an ordinary `.dl7` text; they read in file order, one after another.

```console
$ for f in prelude/*.dl7; do printf '%s declarations=%s rules=%s lines=%s\n' "$f" "$(grep -c '^(: ' "$f")" "$(grep -c '^(<- ' "$f")" "$(grep -c '' "$f")"; done
prelude/0_constructors.dl7 declarations=2 rules=0 lines=9
prelude/1_declarations.dl7 declarations=58 rules=0 lines=305
prelude/2_constructor_rules.dl7 declarations=0 rules=12 lines=77
prelude/3_derived_rules.dl7 declarations=0 rules=71 lines=303
prelude/4_type_algebra.dl7 declarations=24 rules=36 lines=339
prelude/5_tsi_primitives.dl7 declarations=28 rules=0 lines=66
```

| file | what it declares | depends on | one line to read |
|---|---|---|---|
| `0_constructors.dl7` | `Partial`, `Option`: a `source` and a `return` column | the kernel | `prelude/0_constructors.dl7:6-8` |
| `1_declarations.dl7` | name helpers (`contains`, `closed_names`), the return-bearing `Pick`, `Key`, `HistoryV1`, `Exclude`, `Curry`, the reified program rows `program_*`, `Input`, `Output`, `Hosted`, `HostPort` | the kernel | `prelude/1_declarations.dl7:53-56` |
| `2_constructor_rules.dl7` | no declarations; the `intern` rules that make `Partial`, `Option`, `Key`, `HistoryV1`, `Pick`, `Exclude` callable | files 0 and 1 | `prelude/2_constructor_rules.dl7:12-15` |
| `3_derived_rules.dl7` | no declarations; rules for the helpers of file 1, plus rules whose heads are kernel names `:`, `node`, `product`, `def`, `head`, `body` | files 1 and 2 | `prelude/3_derived_rules.dl7:1-4` |
| `4_type_algebra.dl7` | `Conforms`, `ConformsAll`, `Intersect`, `Extend` and their candidate relations, with rules | the kernel | `prelude/4_type_algebra.dl7:3-6` |
| `5_tsi_primitives.dl7` | empty products for TypeScript and Rust primitive classes: `string`, `number`, `bool`, `str`, `i64` | nothing | `prelude/5_tsi_primitives.dl7:26-30` |

The "depends on" column reads the rule bodies; the load does not need it, because a name resolves against the whole unit whatever its order (`oracle/compile/sources/test/fixtures/binding_symmetry/3_declaration_order.dl7`).

One example per dependency step:

```console
$ sed -n 53,56p prelude/1_declarations.dl7 && sed -n 22,26p prelude/2_constructor_rules.dl7
(: Key
   (* (: name str)
      (: options type)
      (: return type)))
(<- (Key ?Name ?Options ?Result)
    (nil ?Empty)
    (cons ?Options ?Empty ?TailArguments)
    (cons ?Name ?TailArguments ?Arguments)
    (intern Key ?Arguments ?Result))
```

```console
$ sed -n 3,6p prelude/4_type_algebra.dl7 && sed -n 55p prelude/5_tsi_primitives.dl7
(: Conforms
   (* (: source type)
      (: contract type)
      (: return type)))
(: bool (* ))
```

## Macrotime and `<+`

Macrotime is a second built-in program, `macrotime/0_standard.dl7`, that runs over syntax rows before lowering. Its one macro claims a form whose item 0 is `<+` and emits the same form with `<-`:

```console
$ sed -n 68,74p macrotime/0_standard.dl7 && sed -n 100,102p macrotime/0_standard.dl7
; function plus(invocation: FormNode): readonly [
;   FormNode<[AtomNode<"<-">, ...Tail<typeof invocation.items>]>,
; ]
(<- (syntax_claim ?Form "<+")
    (syntax_form ?Form)
    (: ?Form item ?Head 0)
    (syntax_atom ?Head "<+"))

(<- (syntax_atom ?Output "<-")
    (plus_operator ?Form ?Output))
```

[Macros](../8_macros.md) traces the waves.

## Rule

- `PRELUDE` is the six files at compile time, `include_str!` in sort order (`src/_8_driver/_0_read.rs:7-15`); `prelude_text` joins them with one newline (`:20-27`).
- The joined text is one unit, `module(prelude)`, the exporter (`src/_2_lower/_12_units.rs:475-492`).
- Every other unit receives an alias for each top-level prelude edge it does not bind (`_12_units.rs:310-342`); [Names](1_names.md) shows the alias sitting before the kernel.
- In the runtime name table a program's declaration replaces a prelude declaration of the same name; two non-prelude declarations of one name stop with `duplicate_relation_name` (`src/_6_eval/_6_json.rs:178-221`).
- `prelude/` and `macrotime/` are byte copies of v7 at `f5018ad23` (`src/_8_driver/_0_read.rs:7`, `:17`).

## Receipts

| claim | path | command |
|---|---|---|
| the six files, lines per file | `prelude/*.dl7` | `wc -l prelude/*.dl7` |
| the prelude compiles into the binary | `src/_8_driver/_0_read.rs:7-18` | `sed -n 7,18p src/_8_driver/_0_read.rs` |
| `<+` lowers as `<-`, equal to v7 | `oracle/compile/sources/test/fixtures/15_standard_plus.dl7` | `cargo test --test _8_compile_oracle` |
| `Hosted` and `HostPort` still declared, read by no executor | `prelude/1_declarations.dl7:292-305` | [Not built yet](../16_not_built.md) |
