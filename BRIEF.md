# Lane catalogtype — make `__rel.type_id` describe non-primitive columns

You are PASS 1 of 2. A coordinator design-review follows. Favor plain, obvious
code over clever code. Do not refactor anything the brief does not name.

## Your first action, before anything else

```
cd /Users/chrishafley/projects/sprefa-lanes/catalogtype
git merge --ff-only ee56170c
```

If that fails, STOP and write REPORT.md saying so. Do not work around it.

## Files you own. Touch nothing else.

- `v6/prolog/lower.pl`
- `v6/prolog/compile/test/plunit_tests.pl`

Another lane owns `v6/prolog/compile.pl` and
`v6/prolog/conformance/fixtures/0_enum_variants.pl` right now. Do not open them.

## Why this matters

`__rel` is the program's own catalog, a real SQLite table every compiled
program carries. A generator that reads it can emit JSON Schema, OpenAPI, or a
Rust CLI without parsing any source. Today it cannot, because every
non-primitive column type collapses to `0` and the target becomes
unrecoverable.

Measured today, the catalog after boot for a two-rel program:

```
text|int|float|bool|json     kind=primitive   rel_id 1..5
<module name>                kind=module
__rel                        kind=rel     arity=11
  rel_id .. h_rule           kind=column  type_id 2,2,2,1,1,2,2,2,1,1,1
rel_name                     kind=rel     arity=1
  name                       kind=column  type_id=1
```

Primitives are already right. The hole is everything else.

## The current code

`v6/prolog/lower.pl`:

```prolog
catalog_type_id(text, 1) :- !.
catalog_type_id(int, 2) :- !.
catalog_type_id(float, 3) :- !.
catalog_type_id(bool, 4) :- !.
catalog_type_id(json, 5) :- !.
catalog_type_id(_, 0).
```

The last clause swallows `ref(Name)` and `list(Element)`.

Ids are assigned BY POSITION for a byte-stable recompile. Read
`catalog_rows/4` and `catalog_rel_rows/7`. The walk assigns ids sequentially in
declaration order: primitives 1..5, then the module row, then each rel followed
by its column rows.

## What to build

### Part 1. A ref column points at its target rel's id

A column whose declared type is another rel's name must carry that rel's
`rel_id` in `type_id`.

The blocker is ordering: `catalog_rel_rows/7` assigns ids while emitting rows,
so a forward reference names a rel whose id does not exist yet.

Fix with TWO PASSES over the SAME declaration order:

- Pass A walks `RelPlans` in declaration order and builds a
  `Name/Arity -> rel_id` map. It assigns exactly the ids the current single
  pass assigns. Do not reorder anything.
- Pass B walks again and emits the rows, resolving a ref column's `type_id`
  from the map built in pass A.

Pass A must reproduce today's ids exactly. If any existing fixture's emitted
`rel_id` values change, you have broken byte stability and must fix it.

To find the storage kind of a column type, use `column_storage/3`, exported
from `v6/prolog/0_type_plane.pl`. It returns `ref(Name)` for a column naming a
declared rel. `lower.pl` already imports from that module; read its existing
import list before adding to it.

### Part 2. A list column points at a synthetic list row

`list(Element)` needs two facts recorded, the list-ness and the element type,
and `type_id` is one integer. Mint a synthetic catalog row instead of widening
the table.

For each distinct `list(Element)` appearing in any column type, emit one row:

- `kind` = `list`
- `local_name` = the printed form of the type, for example `list(text)`
- `type_id` = the ELEMENT's id, resolved the same way a column's is
- `parent_id`, `ordinal`, `arity`, `module_id` = 0
- `h_id`, `h_schema`, `h_rule` = the empty atom

A column typed `list(Element)` then carries that synthetic row's id in its own
`type_id`.

Emit these synthetic rows AFTER the five primitives and BEFORE the module row,
so that a list's element id is always already assigned. This shifts the module
row's id and every id after it, which is expected and is why the sweep
acceptance below allows emitted output to change.

Nested lists such as `list(list(text))` must work: emit the inner row first.

## Validation. Every command must pass.

```bash
cd /Users/chrishafley/projects/sprefa-lanes/catalogtype

# 1. prolog unit tests. Must be 0 failures.
cd v6/prolog && swipl -q -g "consult('compile/test/plunit_tests.pl'), run_tests, halt" -t 'halt(1)'

# 2. the two-implementation agreement sweep.
cd ../tsv2 && bash scripts/sweep.sh
```

Sweep acceptance, read these exact lines:

- `wrong=0` and `final_wrong=0` are MANDATORY. These mean the prolog oracle and
  the emitted TypeScript still agree on every row of every tick.
- `MANIFEST_REASON_DIFF` must show `restated=0 args=0 bucket_moved=0 added=0
  removed=0`. You are changing catalog CONTENT, never which programs compile.
- `identical` WILL drop, because catalog ids shift. That is expected here and
  only here. Report the number you get.

## Also required

Add plunit tests to `v6/prolog/compile/test/plunit_tests.pl`. Follow the shape
of the tests already there. Cover:

1. a column typed by another rel carries that rel's `rel_id`
2. a `list(text)` column carries a synthetic list row's id, and that row's
   `type_id` is `1`
3. `list(list(text))` produces two synthetic rows, inner before outer
4. a program with no refs and no lists produces the ids it produces today,
   proving pass A did not reorder

Test 4 is the byte-stability receipt and is the most important one.

## Style laws, non-negotiable, checked by a pre-commit rail

- Max 2 CONSECUTIVE comment lines anywhere. The rail rejects 3.
- A comment states only a constraint the code cannot show. No change-log
  narrative, no dates, no "this fixes X".
- No em dashes anywhere, prose or code.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime. Use source, base layer, critical, mode.
- Never write `not X, Y` or `this isn't X. it's Y` phrasing in any prose.
- N+1 is banned: never a per-row write, collect the set and write once.
- Follow the existing style of each file you edit, even where it differs.

## Deliverable

Do NOT commit. Leave the working tree dirty.

Write `REPORT.md` at the worktree root containing:

1. The exact diff you made, file by file.
2. The full final output of both validation commands, pasted verbatim.
3. The `identical` count from the sweep, and your explanation of why it
   dropped by exactly that many.
4. A printed catalog dump for a program with a ref column and a list column,
   showing the resolved `type_id` values.
5. Anything the brief told you that turned out to be wrong. Required section;
   write "nothing" only if that is true.
6. Any edge case the brief did not enumerate that you hit.
