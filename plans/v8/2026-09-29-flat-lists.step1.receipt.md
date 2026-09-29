# flat-lists step 1: plan receipt (no edits)

```ts
type ListRow      = { node; label: number /* position */; target; index }   // ':' edge, one per element
type AnonProduct  = { node: application(Product<n>, Args) }                  // n = written length
type Product<n>   = { return: (* (: 0 any) ... (: n-1 any)) }                // name unchosen
```

## Lowering of `(doc.Doc.concat [(doc.Doc.text "a") (doc.Doc.text "b")])`

Today (`src/_0_read/_2_reader.rs:296-339`): reader emits nested form per item headed by atom `[`, tail form, `]` at end; `macrotime/2_list.dl7` rewrites each level to `(cons item tail)` / `(nil)`; lower (`_8_express.rs:505-540`) builds goals `nil`, `cons` x n, `intern`.

Proposed rows (ground case, `_8_express.rs:505-514` path):

```dl7
; node N0 = [ (doc.Doc.text "a") (doc.Doc.text "b") ]
(: N0 0 (ref (application doc.Doc.text ["a"])) 0)
(: N0 1 (ref (application doc.Doc.text ["b"])) 1)
; N0 itself = application(Product<2>, [ref A, ref B])
; concat's `parts` column = ref N0
```

Non-ground case: one `intern Product<2> [?A ?B] ?N0` goal; no `nil`/`cons` goals. Reader emits one form `([ item item ...)` (single node, head atom `[`, items in order); macrotime claim in `2_list.dl7` becomes one rule per form instead of cons/nil pair.

## Anonymous product of arity n

Reuse `application(Callee, Args)` and `instance_rows` (`_3_check/_2a_instance.rs`) with a per-arity template `Product<n>` whose `return` is `(* (: 0 any) .. (: n-1 any))`; `columns` sorting by `edge.index` and the arity check (`expression_arity_mismatch`, line ~68) apply unchanged. Labels are positions (ints), currently `edge.name` is an atom label; `walk` copies `edge.name` into `:` rows, so int labels flow through as-is. Open: where `Product<n>` templates come from (materialized on demand at lower/check time per n seen; no keyword). Awaiting go/name.

`doc_layout.rs:94-118` reads `concat` parts with `u.as_list`; reads `application(_, args)` args as a const list term (`Universe::list`), which stays the argument container. `parts` column must then read positional edges of the anon node.

## `cons`/`nil` sites (`(cons ` / `(nil ` line counts)

| file | cons | nil | replacement |
|---|---|---|---|
| prelude/2_constructor_rules.dl7 | 20 | 12 | build fixed arg list for `intern`: `[a b c]` |
| prelude/3_derived_rules.dl7 | 12 | 5 | 4 arg-builders -> `[..]`; `contains` (29-33), `closed_names` (35-41) -> positional edge lookup |
| prelude/4_type_algebra.dl7 | 19 | 10 | arg-builders -> `[..]`; `all_contracts_conform` (205-215) recursive walk -> positional edge lookup; `ConformsAll.contracts` comment line 35 |
| macrotime/0_standard.dl7 | 6 | 2 | arg-builders (85-96) -> `[..]` |
| macrotime/1_caret.dl7 | 3 | 1 | arg-builder (116-119) -> `[..]` |
| macrotime/2_list.dl7 | 6 | 2 | list macro itself: emits anon product instead; also atoms `"cons"`/`"nil"` at its `syntax_atom` rules |

## `src/` readers of `cons`/`nil`

| file:line | what |
|---|---|
| _2_lower/_9_kernel.rs:19-26,41-44,69-70,82-83 | arity, column labels, modes, return columns for `nil`, `cons`, `str.nil`, `str.cons` |
| _3_check/_5_kernel.rs:25-26,39-40,59-62,80-81,183-184,212-215 | KERNEL_RELATIONS, TYPED_OPS, modes, kernel edges |
| _3_check/_4_mode.rs:206,268 | `cons`/`str.cons` mode, kernel name list |
| _6_eval/_4_kernel.rs:89-90,104-105,146-149,163-186 | `Kernel::{Nil,Cons,StrNil,StrCons}`, `nil_row`, `cons_row` |
| _6_eval/_5_evaluate.rs:471-472 | binding-mode by Kernel |
| _6_eval/_8_functions.rs:74-86,180-183 | UDFs `dl_cons`/`dl_head`/`dl_tail` |
| _5_reify/_7_sqlite.rs:863-865,987,1186-1210 | Kernel::Nil/Cons reify to UDF calls, `empty_list_cell` |
| _6_eval/_7_sqlite_eval.rs:874-906,1305 | `nil`/`cons` rel walk building list vars |
| _6_eval/_9a_ir_lower.rs:474,549,946,1059,1087,1120-1132,2069 | ir lowering of Cons/Nil (known_tail, known_proper_list, cons_rows) |
| _2_lower/_7_execute.rs:333-336 | sum-variant `cons`/`nil` goals |
| _2_lower/_8_express.rs:519-527 | constructor application `nil`/`cons`/`intern` goals |
| _0_read/_2_reader.rs:296-339 | nested `[`/`]` form reader |
| _6_eval/_9_dd_eval.rs | via Kernel enum |

Further `as_list`/`Universe::list` users (268 lines across `src/`) hold the const argument-list term and stay.

## Notes

- `plans/v8/2026-09-28-term-dict-json-args.md` named in the brief is absent from this worktree; measured counts above are from `grep` here.
- `AGENTS.md` row 86 (worktree) still records the cons-chain decision (2026-09-18); the brief cites a 2026-09-29 row.
- Step 4 touches `Kernel` enum consumers in `_9a_ir_lower.rs`, `_7_sqlite.rs`, `_7_sqlite_eval.rs`; `str.cons`/`str.nil` (arity 3/1) are separate kernel rows and are named here only because they share the tables.
