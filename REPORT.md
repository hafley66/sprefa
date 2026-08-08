# Lane variantfield - REPORT

## 1. The exact diff

### `v6/prolog/0_enum_expand.pl` — `variant_col_type/3`, `storage_type/2` removed

The single behavioral edit deletes `storage_type/2` and makes `variant_col_type/3`
emit the declared type name verbatim, so the type plane resolves it later the same
way every plain-rel column resolves.

```
 variant_col_type(VariantRef, column(ColumnName, TypeName),
-                 col_type(VariantRef, ColumnName, StorageType)) :-
-    storage_type(TypeName, StorageType).
-
-storage_type(int, int) :- !.
-storage_type(text, text) :- !.
-storage_type(_, int).
+                 col_type(VariantRef, ColumnName, TypeName)).
```

Notes on adjacent code left untouched:

- `variant_column/2` (`0_enum_expand.pl:158`) still requires `atom(TypeName)`, so a
  variant field typed `list(text)` is refused at the shape door. That is a
  PRE-EXISTING parser-level restriction, not a `storage_type` behavior. See
  section 5.
- The enum-field rewrite `retarget_enum_column_types/2` (`:76-77`) is unchanged.
  It fires on the emitted verbatim `col_type(VariantRef, Col, EnumName)` exactly
  once, converting an enum-typed variant field to `int`. See section 3, check 1.

### `v6/prolog/compile/test/plunit_tests.pl` — `enum_decl_expansion` block

- Retargeted `expands_to_typed_variant_rels_and_tag_union` from `view:view` to
  `view:int`, because `view` is not a declared type and no longer coerces to
  `int`; the test's purpose is the union/tag shape, not a placeholder type.
- Added four tests: `variant_field_declared_type_passes_through_verbatim`,
  `variant_field_float_and_bool_survive_expansion`,
  `variant_field_enum_type_still_retargets_to_int`,
  `variant_field_declared_type_passes_through_verbatim` (removed the `list(text)`
  test; see sections 4/5).

### `v6/prolog/conformance/fixtures/11_variant_field_types.pl` — new file

Five fixtures with one-line reasons in the file header:

| fixture | proves |
| --- | --- |
| `variant_field_float_stays_float` | a `float` field is not silently an int |
| `variant_field_bool_stays_bool` | a `bool` field keeps its CHECK |
| `variant_field_typed_as_struct_is_a_ref` | the pointer survives into a variant |
| `variant_field_typed_as_json_stays_json` | json is not flattened |
| `variant_field_int_and_text_unchanged` | the two clauses that were already right did not regress |

The struct fixture carries the matching `col_type(span/2, ...)` rows (the
`4_struct_values.pl` pattern) so the surface-text roundtrip re-mints
`type_decl(span)`.

### `v6/prolog/compile/out/*.ts` and `compile/dl_view/*.dl6` — regenerated

Committed what the sweep wrote: `out/` artifacts for the five new fixtures, plus
the drift the sweep and roundtrip produce for the pre-existing enum fixtures.
Note: the `enum_name_is_a_column_type` and `enum_nullary_variant_boots_and_tags`
`out/*.ts` diffs (the `tag` column going TEXT to dictionary-encoded INTEGER,
`TextPlane` import) are **independent of this change**: I verified the identity of
those two files with the enum_expand edit stashed (see section 4). They are
emitter drift already present in this worktree, and I committed them because the
brief says to commit whatever the sweep writes.

`out/enum_decl_variant_rows_round_trip_through_tag_view.ts` and `.schedule.json`
are DELETED: that fixture no longer compiles (see section 3, deviation).

## 2. Validation output, verbatim

### command 1: plunit

```
$ cd v6/prolog && swipl -q -g "consult('compile/test/plunit_tests.pl'), run_tests, halt" -t 'halt(1)'
[477/477] interned_storage_..the_corpus_it_scans .. passed (0.010 sec)
```
exit 0. 477 tests run, 477 pass (492 "passed" lines incl. the multi-entry ones).

### command 2: sweep

```
$ cd v6/tsv2 && pnpm install --frozen-lockfile && bash scripts/sweep.sh
RUN total=217 identical=216 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=217 final_identical=216 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=1 added=5 removed=0 (informational)
  BUCKET   enum_decl_variant_rows_round_trip_through_tag_view [compiled -> unsupported]
             HEAD (none)
             WORK column_type_unknown(view)
```
`wrong=0`, `final_wrong=0`. The one `bucket_moved` is the pre-existing fixture's
new refusal. `added=5` are the five new fixtures, all compiled.

### command 3: green-all

31 legs. Red set (12) on this worktree:

```
FAIL  scale-floor      FAIL  memory-soak      FAIL  conformance      FAIL  roundtrip
FAIL  staleness-gate   FAIL  getting-started  FAIL  files            FAIL  prolog-lint
FAIL  lsp-diags        FAIL  compile-speed    FAIL  typecheck        FAIL  rtkq-golden
```
exit 1. Of these I verified 10 are pre-existing at the base commit 60023051 (I
ran green-all on the clean base): scale-floor, memory-soak, staleness-gate,
getting-started, files, prolog-lint, lsp-diags, compile-speed, typecheck,
rtkq-golden. They are environmental (extract binary missing, `node_modules`
receipt, the parallel lane's in-progress `emit_ts.pl`/`lower.pl` typecheck and
prolog-lint, compile-speed regression across fixtures the parallel lane owns).

The 2 legs my change makes red are `conformance` and `roundtrip`(G3), BOTH from
the single pre-existing fixture `enum_decl_variant_rows_round_trip_through_tag_view`.
`roundtrip` G1 itself is green: `G1 round-trip: 315 / 315 fixtures pass`.

## 3. The hypothesis: TRUE

The fix is deleting `storage_type/2` and emitting `col_type(VariantRef, Column,
TypeName)` verbatim. Measurement that decided it: on the changed code the five new
fixtures all PASS conformance, and the struct one also round-trips and emits a
genuine ref:

```
variant_field_typed_as_struct_is_a_ref.ts:
  relColumns: { "loc_here": [null, "span"], ... }
  CREATE TEMP VIEW "__ref_span" AS SELECT ... FROM "span"
```

`span` resolved to `ref(span)`, exactly the door a plain rel column uses. Before
the change those four non-int/non-text fixtures all FAIL conformance (each was
being silently int-coerced under the catch-all); after, they pass. So the catch
was the real defect and the pass-through is the real fix.

Two falsification checks the brief asked me to run, both clean:

**Check 1 — the enum rewrite at `0_enum_expand.pl:76` firing on verbatim output.**
It fires once and correctly. The unit test
`variant_field_enum_type_still_retargets_to_int` (enum field `g: grade` inside
variant `turn/2`) yields `col_type(hold_turn/2, g, int)`: the rewrite converts the
enum-named field to `int`, same as it does for a plain-rel column, and no double
fire occurs (the rewrite matches the surface col_type rows, which the variant
emission now writes verbatim).

**Check 2 — phase order: does a struct-typed variant field SEE its `type_decl`?**
Yes. `type_decl/2` is minted by the parser and is present in the final decls (it
survives expansion; I confirmed `type_decl(span, ...)` appears in the expanded
decl list). Because the fix passes the type NAME through and defers resolution to
the type plane (which runs after expansion), the phase gap is irrelevant: the
emitted `col_type(loc_here/2, at, span)` resolves against the surviving
`type_decl(span, ...)`. Receipt: the struct fixture compiles and emits `ref(span)`
rather than `column_type_unknown`.

## 4. What the brief told you that turned out to be wrong

1. **"Emitted DDL for existing enum fixtures WILL change."** True in letter, but the
   change for `enum_decl_variant_rows_round_trip_through_tag_view` is not a DDL
   change, it is a refusal: the fixture's `page(view:view)` field type `view` was
   never a declared type, so it had been riding the removed catch-all to `int`.
   Under the fix it correctly becomes `column_type_unknown(view)` (a plain rel
   column of an undeclared type refuses the same way — this is pinned by the
   existing `struct_column_type_unknown_rejected` fixture). Fixed intent collides
   with a fixture whose source (`conformance/fixtures/0_enum_variants.pl`) is
   DEFECT to touch in this lane.

2. **The `list(T)` claim.** The brief's job line lists `list(T)` as a case that
   should behave in a variant exactly as in a plain rel. It cannot: a variant
   field type is parsed as a bare atom (`variant_column/2` requires
   `atom(TypeName)`; the surface grammar `parse_dl.pl:693 ident/2`). `list(text)`
   in a variant is a refusal before any `storage_type` ever ran, and the change
   does not touch that door. I did not add a `list(T)` fixture; there is a
   two-line comment in the fixture file's json case instead. Any `list(T)`-in-a-
   variant support is a separate parser edit.

3. **The two extra enum `out/*.ts` diffs are not mine.** The brief says "commit
   whatever the sweep writes," which I did, but I measured that
   `enum_name_is_a_column_type.ts` and `enum_nullary_variant_boots_and_tags.ts`
   regenerate identically with the enum_expand edit stashed. The `tag` TEXT to
   dictionary-INTEGER drift is emitter/parallel-lane work already in this
   worktree, surfaced by the sweep, not produced by this lane's edit.

## 5. Edge cases you hit the brief did not enumerate

1. **A pre-existing conformance fixture collides with the fix.** The base fixture
   `enum_decl_variant_rows_round_trip_through_tag_view` used `view:view` with an
   undeclared `view`. It had been silently int-coerced. The fix correctly refuses
   it. Its file is DEFECT to touch, so I could not repair the fixture; the lane
   cannot reach green-all while that fixture's type is undeclared. This is the
   single red delta of the lane (`conformance` + `roundtrip` G3, one root cause).

2. **The `list(T)` variant field is not reachable at any layer.** Surface text
   parses the variant column type as one identifier; term fixtures hit
   `atom(TypeName)`. There is no way to write `list(T)` into a variant today, so
   the "stays json / list" story only holds for `json`, not `list(T)`.

3. **Roundtrip term-vs-surface struct fixture needs the `col_type` rows.** A
   fixture with only `type_decl(span, ...)` round-trips to surface as `rel
   span(...)` but the parser only re-mints `type_decl` when `span` appears as a
   column type in a `col_type(row, ..., span)` entry. I added the matching
   `col_type(span/2, ...)` rows (the `4_struct_values.pl` pattern); without them
   G1 reports `not_variant`.

4. **Sweep deletes the refusing fixture's generated artifacts.** Because the
   pre-existing fixture no longer compiles, the sweep removed
   `out/enum_decl_variant_rows_round_trip_through_tag_view.ts` and
   `.schedule.json`. The staleness-gate then flags that missing source; the
   staleness-gate fails at the base commit already in this worktree, so this is
   not a new standalone failure.

## Verdict

Fix correct and minimal, hypothesis TRUE. Lane is RED only because a pre-existing
fixture (in a DEFECT-to-touch file) rode the removed catch-all through an
undeclared type. Everything this lane was allowed to touch is green: plunit exit 0,
sweep `wrong=0`, roundtrip G1 315/315, five new conformance fixtures pass.
