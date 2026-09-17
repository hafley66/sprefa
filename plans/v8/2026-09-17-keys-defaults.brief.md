# Keys and defaults brief

Lane branch `feat/keys-defaults-userland`. Runs after `feat/import-std-alias`
merges; independent of the caret lane but shares no files with it. First
action `git merge --ff-only <sha>`; failure = stop and report.

## Contents

1. Goal
2. Decisions (read `AGENTS.md:76-107` first)
3. Before / after
4. Files owned
5. Files forbidden
6. Steps and receipts
7. Gate
8. Laws
9. Report

## 1. Goal

`relation(Rel, Arity, Keys)` is derived by prelude rules from `Key` edges and
`return` edges. The Rust loop at `src/_2_lower/_2_declare.rs:278-305` is gone.
`(: title (text "untitled"))` is a typed default; `column_type` and `default`
are prelude rules; a column type mismatch is a comptime diagnostic row.

## 2. Decisions

| row | decision |
|---|---|
| `AGENTS.md:104` | keys are userland; `relation/3` derived from `Key` (inputs) and `return` (outputs); the Rust loop is a duplicate and goes; value-position calls lower to goals over `relation/3` and settle at eval |
| `:90` | `(text "title")` is `intern text ["title"]`, a value node; a column whose target is a value node has that type and default; `(: n 3)` infers from the literal; defaults consumed only in build mode; item 4 of `:` is free |
| `:91` | column typing is a comptime prelude rule; no Rust type pass before comptime reads column types |
| `:87` (09-16) | `intern` is construction; allowed in seeds, macrotime, comptime; refused in runtime cycles |
| `:83` (09-16) | explicit `return` field stays |

## 3. Before / after

Key sets. Before, Rust:

```
_2_declare.rs:278  for edge in edges { if name == return_atom { return_indices.push(index) } }
_2_declare.rs:293  key_sets = [all positions except the one return]
_2_declare.rs:305  relation(owner, arity, key_sets)
```

After, `prelude/3_derived_rules.dl7`:

```dl7
; relation(Rel, Arity, Keys): Keys is one key set, every non-output position.
(<- (relation ?Relation ?Arity ?KeySets)
    (product ?Relation)
    (product_arity ?Relation ?Arity)
    (output_positions ?Relation ?Outputs)
    (input_positions ?Relation ?Arity ?Outputs ?KeySets))

; output_positions: the index of the column the `return` edge names, or the
; `return` edge's own index when its target is a type.
(<- (output_positions ?Relation ?Outputs)
    (: ?Relation return ?Target ?ReturnIndex)
    (return_position ?Relation ?Target ?ReturnIndex ?Position)
    (nil ?Empty) (cons ?Position ?Empty ?Outputs))

(<- (return_position ?Relation ?Target ?ReturnIndex ?Position)
    (: ?Relation ?Name ?Target ?Position) (not (= ?Name return)))     ; sibling ref
(<- (return_position ?Relation ?Target ?ReturnIndex ?ReturnIndex)
    (type ?Target))                                                    ; own column
```

`keyed_edge`/`key_rank` (`3_derived_rules.dl7:43-63`) already exist; a
product with `Key` columns takes those as its key set and `return` as outputs.

Defaults. Before: no form parses; `(: name text "title")` is a 4-item `:`
that `_1_forms.rs:70` rejects. After:

```
(: User (* (: name text)
           (: title (text "untitled"))
           (: n 3)))
```

```dl7
(<- (column_type ?Product ?Name ?Type)
    (: ?Product ?Name ?Type ?_) (type ?Type))
(<- (column_type ?Product ?Name ?Type)
    (: ?Product ?Name ?Value ?_) (intern_snapshot ?Type ?_ ?Value) (type ?Type))
(<- (default ?Product ?Name ?Literal)
    (: ?Product ?Name ?Value ?_) (intern_snapshot ?_ ?Arguments ?Value)
    (cons ?Literal ?_ ?Arguments))
(<- (column_type_mismatch ?Product ?Name ?Expected ?Found)
    (column_type ?Product ?Name ?Expected)
    (column_value ?Product ?Name ?Found) (not (Conforms ?Found ?Expected ?_)))
```

Rows for `title`: `(: prod title v1 1)`, `(intern text ["untitled"] v1)`.
`(: n 3)`: lower interns the literal with its primitive, `(intern int [3] v2)`.

## 4. Files owned

| file | change |
|---|---|
| `src/_2_lower/_2_declare.rs:270-306` | delete the key-set loop; `lower_edge_bind` emits only `:` rows |
| `src/_2_lower/_8_express.rs:220-260` | value-position call: no `cx.relations` read; emit goals `(relation ?Rel ?Arity ?Keys)` + the call goal; `expression_return_position:280` reads `output_positions` the same way |
| `src/_2_lower/_8_express.rs`, apply on a node with no rows | `(text "title")` lowers to `intern`; the arm that today returns `not_relation` (`:226-236`) for a type node instead emits the intern goal |
| `src/_2_lower/_1_forms.rs` | bare literal target `(: n 3)` interns with its primitive |
| `prelude/1_declarations.dl7` | decls for `relation`, `output_positions`, `input_positions`, `return_position`, `column_type`, `default`, `column_type_mismatch`, `product_arity` |
| `prelude/3_derived_rules.dl7` | the rules above |
| `src/_3_check/_0_api.rs:101-114`, `_7_resolved.rs:24`, `_6_strata.rs:260` | read `relation/3` from graph rows after the prelude round; if these run before any round, stop and report the phase order with lines |
| `src/_3_check/` column type pass | removed; name the function and line in the PR |
| `src/_4_comptime/_3_assemble.rs:97-300` | verify it consumes the derived rows unchanged; paste the diff (expect none) |
| `oracle/**` | regoldens through `oracle/refreeze.py`; every changed golden listed in the PR with one line saying why |
| `tests/_3_lower_oracle.rs`, `tests/_4_check_oracle.rs`, `tests/_6_comptime_oracle.rs` | pick up regoldens |
| `plans/v8/probes/2026-09-17-defaults.dl7`, `2026-09-17-keys.dl7` | probes |
| `book/src/` the products/columns page | defaults section, before/after, rx lowering per snippet |

## 5. Files forbidden

`src/_0_read/**`, `macrotime/**`, `std/**`, `src/_6_eval/**`, `src/_9_runtime/**`,
`src/_4_comptime/_0_load/**`. If `_6_eval` must change for `intern` on a
primitive, stop and report the line.

## 6. Steps

| # | step | receipt |
|---|---|---|
| 1 | count `relation(` emitters and readers on base: `grep -rn '"relation"' src/ \| wc -l` | number |
| 2 | prelude decls + rules; `dl8 compile` of a product with one `return` prints one `relation/3` row equal to the Rust one | both rows pasted, byte-equal |
| 3 | delete the Rust loop; `cargo test --test _3_lower_oracle --test _4_check_oracle` | diff of goldens, PASS lines |
| 4 | value-position call over goals; `fixtures/` case with `(f (User "x"))` compiles rc=0 and eval binds the return column | eval closure rows |
| 5 | `(text "title")` intern arm; `(: n 3)`; probe compiles; `column_type`, `default` rows printed | rows pasted |
| 6 | `(: n (text 3))`: a literal of the wrong primitive under `text`. Expected: one `column_type_mismatch` row, rc=0 at compile. Paste the row; if it is anything else, stop and report | diagnostic row |
| 7 | remove the Rust column type pass; `cargo test --test _4_check_oracle` | goldens diff |
| 8 | full gate | summary lines |

Commit after every step; message names the step.

## 7. Gate

```bash
cargo test 2>&1 | grep -E "^test result|FAILED|panicked"
grep -rn "return_indices\|key_sets" src/_2_lower/_2_declare.rs | wc -l    # expect 0
git diff --stat origin/main...HEAD
```

Known red on the base outside this lane: `.github/CI-KNOWN-RED.md`, dl8
battery section. Measure each red leg three times.

## 8. Laws

Same as `plans/v8/2026-09-17-import-arc.brief.md` section 8. Plus: every
regoldened oracle file is listed in the PR with a one-line reason; a golden
that changes for a reason the brief does not name is a stop-and-report.

## 9. Report

```bash
boop beep --no-wait --as <lane> sprefa-coordinator "keys/defaults: PR #<n>, oracle <pass>/<total>, battery <pass>/<total>, regoldens <count>, red: <list or none>"
```
