# json as a value in scan

Base `26f3f25f`. Branch `feature/json-as-value-in-scan`. User decision
2026-08-11, verbatim: "get json as value in scan done please".

## TOC
1. What landed, in one table
2. Piece 1: the guard chain, before and after
3. Piece 1: fail-first receipts
4. Piece 2: RFC 7396 clause per behavior
5. Piece 2: fail-first receipts
6. The sabotage receipt
7. New .dl spellings with rx lowerings
8. Gate output
9. Forks for the user
10. ARCH ROW TO ADD

---

## 1. What landed, in one table

| step | commit | manifest `compiled` | fixtures | TEXT_DOOR | conformance |
|---|---|---|---|---|---|
| base | `26f3f25f` | 270 | 370 | 272/272/0 (brief) | 372 PASS 0 FAIL |
| piece 1 | `bde23e25` | 272 | 370 | 274/274/0 | 372 PASS 0 FAIL |
| piece 2 | `c75ccec6` | 279 | 377 | 281/281/0 | 379 PASS 0 FAIL |
| lint | `b9fa49b9` | 279 | 377 | 281/281/0 | 379 PASS 0 FAIL |
| decl order | `35cb9a1d` | 279 | 377 | 281/281/0 | 379 PASS 0 FAIL |

`unsupported` 100 -> 98. The two rows that moved:

```
BUCKET   braces_literal_canonicalizes [unsupported -> compiled]
           HEAD json_value_expression({stars:4,name:A})
           WORK (none)
BUCKET   braces_in_head_position [unsupported -> compiled]
           HEAD json_value_expression({repo:A})
           WORK (none)
```

Seven fixtures added, all `compiled`, all byte-identical against the oracle
tick log (one is a graded stop, see §4).

---

## 2. Piece 1: the guard chain, before and after

`json_value_expr/1` is UNCHANGED in extension. Its three clauses match the
same terms before and after; only the consequent moved.

```prolog
json_value_expr(Expr) :- compound(Expr), Expr = {}(_), !.
json_value_expr(Expr) :- is_list(Expr), Expr \== [], !.
json_value_expr(Expr) :- compound(Expr), Expr = [_ | _].
```

| clause | matches | before | after |
|---|---|---|---|
| 1 | braces literal `{}/1`, any nesting | throw | `json_object(...)` keys sorted at compile time |
| 2 | proper non-empty list | throw | `json_array(...)` |
| 3 | partial list `[H\|T]`, T unbound | throw | STILL throws `json_value_expression` |

Clause 3's behavior is preserved by `compile_json_document/4`'s fall-through
arm, which re-raises the original name. Pinned by
`json_document_value:partial_list_value_keeps_its_unsupported`.

`compile_expr/7`'s chain, `lower.pl:527-573`, arms in order:

```
var -> bool_lit -> bound compound -> integer -> float -> atomic ->
concat -> text_scalar -> json_scalar (NEW, piece 2) -> arithmetic ->
json_value_expr (WAS throw, NOW lowers) -> compound (tagged term) -> throw
```

Proof nothing else widened, three ways:

1. **The arms above are untouched.** `json_value_expr` is tested at the same
   position in the same chain. Nothing that reached an earlier arm can now
   reach this one, because `->` commits. The empty object atom `{}` and the
   empty list `[]` are `atomic/1` in SWI and still take the `atomic` arm,
   unchanged. §9 fork 2.
2. **The arm below receives exactly the same set.** `json_value_expr(Expr)`
   is the same guard, so the set of terms falling through to the generic
   compound arm is identical. Pinned by
   `json_document_value:compound_term_value_still_renders_as_tagged_term`,
   which was GREEN before the change and GREEN after:
   ```
   INSERT OR IGNORE INTO "doc" ("col1") SELECT json_object('fn', 'route_data', 'args', json_array(b0."col1")) FROM "seed" b0
   ```
3. **The manifest moved by exactly 2.** 270 -> 272, with `MANIFEST_REASON_DIFF
   restated=0 args=0 bucket_moved=2 added=0 removed=0`. No other fixture
   changed bucket or reason.

The one thing that DID widen, deliberately: `analyze.pl:expression_type/3`
gained a `json` clause for the same construct, so the head column is `json`
storage. Without it the column is `text` and `canonical_column_expr`'s text
arm passes the document through as a JSON STRING at the boundary while the
oracle renders an object. That is classifying the construct, which the brief
names as the allowed reason to touch `analyze.pl`.

### The stale reason at the throw site

The comment that justified the throw (`lower.pl:621-638` on base) said the
tick-log encoder renders a braces literal as right-nested cons text,
`obj([|](-(name,cli),[|](-(stars,4),[])))`. That is no longer true.
`conformance/ticklog.pl:132-159` renders `obj(Pairs)` as real JSON with sorted
keys, and `tsv2/runtime/ticklog.ts:70` parses and canonicalizes a `json`
column on read. The json_flex arc (ARCH.pl:855, "oracle tick log was not
JSON") fixed that and the comment was never updated. The comment has been
replaced with what the code cannot show.

### Duplicate keys

House pattern, copied from the `json_object` aggregate arm at
`lower.pl:5015-5022`: emit text that is not valid JSON so SQLite fails the
statement, matching `body.pl:json_canon/2`'s run-time `json_dup_key` throw.
The check walks EVERY level before any subtree renders, because
`canonical_json_text/2` would otherwise throw at COMPILE time on a ground
duplicate-key subtree, which is the wrong phase.

```
json('json_dup_key')
```

---

## 3. Piece 1: fail-first receipts

Unit `json_document_value`, 8 tests. RED on `26f3f25f`, verbatim:

```
% [1/8] json_document_val.._sorted_json_object .... **FAILED (0.003 sec)
ERROR:     test json_document_value:braces_literal_value_lowers_to_sorted_json_object: throw(unsupported_construct(json_value_expression({stars:4,name:_1000})))
% [2/8] json_document_val..one_ground_document .... **FAILED (0.000 sec)
ERROR:     test json_document_value:braces_head_position_lowers_to_one_ground_document: throw(unsupported_construct(json_value_expression({repo:cli})))
% [3/8] json_document_val..owers_to_json_array .... **FAILED (0.000 sec)
ERROR:     test json_document_value:list_literal_value_lowers_to_json_array: throw(unsupported_construct(json_value_expression([_702,7])))
% [4/8] json_document_val.._column_stores_json .... **FAILED (0.000 sec)
ERROR:     test json_document_value:braces_literal_column_stores_json: failed
% [5/8] json_document_val.._emits_invalid_json .... **FAILED (0.000 sec)
ERROR:     test json_document_value:duplicate_key_document_emits_invalid_json: throw(unsupported_construct(json_value_expression({name:_996,name:other})))
% [6/8] json_document_val.._emits_invalid_json .... **FAILED (0.000 sec)
ERROR:     test json_document_value:nested_duplicate_key_document_emits_invalid_json: throw(unsupported_construct(json_value_expression({outer:_996,inner:{key:1,key:2}})))
% [7/8] json_document_val..ders_as_tagged_term ..
ERROR: [Thread main] 6 tests failed
```

Tests 7 and 8 are the CONTROLS: the tagged-term arm below and the partial-list
stop. Both green before, both green after. 6 red / 2 green.

GREEN after, verbatim:

```
% [1/8] json_document_val.._sorted_json_object ..
% [2/8] json_document_val..one_ground_document ..
% [3/8] json_document_val..owers_to_json_array ..
% [4/8] json_document_val.._column_stores_json ..
% [5/8] json_document_val.._emits_invalid_json ..
% [6/8] json_document_val.._emits_invalid_json ..
% [7/8] json_document_val..ders_as_tagged_term ..
% [8/8] json_document_val..eps_its_unsupported ...... passed (0.000 sec)
```

Manifest fail-first, the same fact from the other side: both target fixtures
carried `bucket: unsupported` with `reason: json_value_expression(...)` on
base, and `bucket: compiled` after.

---

## 4. Piece 2: RFC 7396 clause per behavior

New registry family `json_scalar` (the `text_only` type rule rejects a json
operand), one row:

```prolog
expression(json_patch/2, json_scalar,      3, json_patch,           json_only).
```

| behavior | RFC 7396 clause | oracle | emitter | fixture |
|---|---|---|---|---|
| objects merge recursively | §2 `Target[Name] = MergePatch(Target[Name], Value)` | `json_merge_patch/3` recurses per key | native `json_patch` | `json_patch_merges_nested_objects_recursively` |
| arrays replace wholesale | §2 `else: return Patch`; §1 "if the patch is anything other than an object, the result will always be to replace the entire target with the entire patch" | non-`obj/1` patch returns itself | native `json_patch` | `json_patch_replaces_arrays_wholesale` |
| scalars replace wholesale | same clause | same | same | `json_patch_non_object_patch_replaces_the_document` |
| non-object target empties | §2 `if Target is not an Object: Target = {}` | `TargetPairs = []` | native `json_patch` | `json_patch_non_object_target_becomes_empty` |
| null deletes a key | §1 "Null values in the merge patch are given special meaning to indicate the removal of existing values"; §2 `if Value is null: remove the Name/Value pair from Target` | **NOT IMPLEMENTED, STOPPED** | **NOT IMPLEMENTED, STOPPED** | `json_patch_null_stand_in_stops_both_doors` |

### Why the null clause stops instead of running

This language has no term that renders as JSON null.
`compile/scripts/0_json_arrival.pl:92` folds `null` onto the atom `none`, and
`0_type_plane.pl:canonical_json_text/2` renders `none` as the STRING
`"none"`. Measured on the pinned driver:

```
real null deletes   {"r":"{\"b\":2}"}
string none keeps   {"r":"{\"a\":\"none\",\"b\":2}"}
```

So a `none` in a patch could mean delete (the oracle's own read side already
treats `none` as the json-null stand-in: `body.pl:json_capture_type/2`
excludes it from `text`, and `braces_decode/2` fails a bare field pattern on
it) or an ordinary string. Choosing is a language decision. Both doors stop:

- oracle: `throw(json_patch_null_unruled)`
- emitter, house pattern:

```sql
CASE WHEN EXISTS (SELECT 1 FROM json_tree(json(d0."patch")) WHERE "type" = 'null' OR "atom" = 'none')
     THEN json('json_patch_null_unruled')
     ELSE json_patch(json(b0."snapshot"), json(d0."patch")) END
```

The guard reads the PATCH operand only; a target's own null is data RFC 7396
never touches. It catches both a real JSON null (`type = 'null'`, reachable
only through a host-fed row) and the string the stand-in renders as. Graded
in the sweep as `REJECTION ... SQLITE_ERROR: malformed JSON` plus
`NO_ORACLE_FINAL ... oracle threw on this schedule too`, the same shape the
two landed dup-key fixtures produce.

### Key order, measured

SQLite's `json_patch` does NOT sort keys. On `@libsql/client` 3.45.1:

```
key order out       {"r":"{\"b\":1,\"a\":2}"}
nested key order    {"r":"{\"z\":1,\"a\":{\"q\":1,\"b\":2,\"c\":3}}"}
```

The oracle keysorts. `json_patch_fold_result_is_key_sorted` grades exactly
that gap (target `{zeta: 1}`, patch `{alpha_key: 2}`) and is BYTE-IDENTICAL,
because `tsv2/runtime/ticklog.ts:45-52` canonicalizes a `json` column on read.
So storage order is not part of the graded contract for a json column. That
contradicts `lower.pl:5455-5462`'s claim that canonicalization "has to happen
once, on the way in"; it is a card, not a defect, and it is fork 3 in §9.

---

## 5. Piece 2: fail-first receipts

On base, `json_patch/2` had NO registry row, so both doors were SILENTLY
WRONG rather than stopped: the oracle left the call unevaluated as a compound
term and `compile_expr`'s generic compound arm wrapped the same call in the
json1 tagged-term encoding. All 7 fixtures RED, verbatim:

```
      got [metric_doc(alpha,json_patch(json_patch({cpu:1},{mem:2}),{cpu:9}))]
fail  json_patch_fold_merges_arrival_documents
      got [metric_doc(alpha,json_patch({zeta:1},{alpha_key:2}))]
fail  json_patch_fold_result_is_key_sorted
      got [metric_doc(alpha,json_patch({cpu:{user:1,sys:2}},{cpu:{sys:9}}))]
fail  json_patch_merges_nested_objects_recursively
      got [metric_doc(alpha,json_patch({tags:[red,green]},{tags:[blue]}))]
fail  json_patch_replaces_arrays_wholesale
      got [metric_doc(alpha,json_patch({cpu:1},[7,8]))]
fail  json_patch_non_object_patch_replaces_the_document
      got [metric_doc(alpha,json_patch([7,8],{cpu:1}))]
fail  json_patch_non_object_target_becomes_empty
fail  json_patch_null_stand_in_stops_both_doors
```

GREEN after, verbatim:

```
PASS  json_patch_fold_merges_arrival_documents
PASS  json_patch_fold_result_is_key_sorted
PASS  json_patch_merges_nested_objects_recursively
PASS  json_patch_replaces_arrays_wholesale
PASS  json_patch_non_object_patch_replaces_the_document
PASS  json_patch_non_object_target_becomes_empty
PASS  json_patch_null_stand_in_stops_both_doors
```

Unit `json_merge_patch`, 8 tests, all green:

```
% [1/8] json_merge_patch:..null_stand_in_guard ..
% [2/8] json_merge_patch:..eps_its_unsupported ...... passed (0.001 sec)
% [3/8] json_merge_patch:..objects_recursively ...... passed (0.000 sec)
% [4/8] json_merge_patch:..d_scalars_wholesale ...... passed (0.000 sec)
% [5/8] json_merge_patch:..a_non_object_target ...... passed (0.000 sec)
% [6/8] json_merge_patch:..ult_keys_are_sorted ...... passed (0.000 sec)
% [7/8] json_merge_patch:.._json_null_stand_in ...... passed (0.000 sec)
% [8/8] json_merge_patch:.._json_null_stand_in ...... passed (0.000 sec)
```

### Two authoring corrections, stated rather than hidden

The merged VALUES in the fixture expectations were right first time. Two
spellings around them were not, and both were corrected against the oracle:

1. The Initial seed row stays as the raw braces term in the oracle's
   retraction delta (`-metric_doc(alpha,{cpu:1})`), not `obj([cpu-1])`. The
   expectation was rewritten to the term the oracle actually holds.
2. A schedule of N ticks produces N+1 delta entries; the trailing `[]` was
   missing. `concat_fold_follows_arrival_order` shows the same shape.

Neither widened a value. The merged documents asserted before the run
(`obj([cpu-1,mem-2])`, `obj([cpu-9,mem-2])`, `obj([alpha_key-2,zeta-1])`)
are the ones that passed.

A third correction was to fixture DECL ORDER, caught by `roundtrip`'s G1 leg
(`fail(not_variant)` on all 7): `print_dl.pl` reconstructs decls per-rel as
`col_type` before `kind`/`keep`/`keyed`. G1 went 372/379 -> 379/379.

A fourth was caught by `prolog-lint`: the two new plunit units reached
`body:` and `lower:` directly, 8 `private_cross_module_call` findings.
`json_scalar_value/3` is now a real export beside `json_capture_type/2`, for
the stated reason, and the storage claim is pinned through the already-exported
`column_def/4`.

---

## 6. The sabotage receipt

One line, `body.pl:json_merge_patch_pair/3`, removing RFC 7396 §2's recursion
so a nested object REPLACES instead of merging:

```prolog
-    json_merge_patch(Prior, Value, Merged).
+    ( true -> Merged = Value ; json_merge_patch(Prior, Value, Merged) ).
```

Caught on all three legs.

Oracle leg:

```
    MISMATCH final metric_doc/2
      got [metric_doc(alpha,obj([cpu-obj([sys-9])]))]
      want [metric_doc(alpha,obj([cpu-obj([sys-9,user-1])]))]
fail  json_patch_merges_nested_objects_recursively
```

plunit leg:

```
% [3/8] json_merge_patch:..objects_recursively .... **FAILED (0.000 sec)
ERROR:     test json_merge_patch:merge_patch_merges_nested_objects_recursively: failed
```

Sweep parity leg, byte-level, both directions:

```
RUN total=279 identical=274 wrong=1 emitted_crash=0 rejection=4 no_oracle_log=0
  WRONG json_patch_merges_nested_objects_recursively first diff at line 1: actual={"tick":1,"deltas":{"metric_doc":{"add":[["alpha",{"cpu":{"sys":9,"user":1}}]],"del":[["alpha",{"cpu":{"sys":2,"user":1}}]]},"metric_sample":{"add":[["alpha",{"cpu":{"sys":9}}]],"del":[]}}} oracle={"tick":1,"deltas":{"metric_doc":{"add":[["alpha",{"cpu":{"sys":9}}]],"del":[["alpha",{"cpu":{"sys":2,"user":1}}]]},"metric_sample":{"add":[["alpha",{"cpu":{"sys":9}}]],"del":[]}}}
FINAL total=279 final_identical=274 final_wrong=1 no_oracle_final=4
  FINAL_WRONG json_patch_merges_nested_objects_recursively actual={"final":{"metric_doc":[["alpha",{"cpu":{"sys":9,"user":1}}]],"metric_sample":[["alpha",{"cpu":{"sys":9}}]]}} oracle={"final":{"metric_doc":[["alpha",{"cpu":{"sys":9}}]],"metric_sample":[["alpha",{"cpu":{"sys":9}}]]}}
```

Reverted; tree clean; re-measured green:

```
RUN total=279 identical=275 wrong=0 emitted_crash=0 rejection=4 no_oracle_log=0
FINAL total=279 final_identical=275 final_wrong=0 no_oracle_final=4
```

---

## 7. New .dl spellings with rx lowerings

Every snippet is the GENERATED text-door surface
(`v6/prolog/compile/dl_view/*.dl6`), not hand-written.

### 7.1 json document in value position

```
doc(Value) <-
  seed(Name),
  Value := {stars: 4, name: Name}.
```

```js
seed$.pipe(
  map(({ name }) => ({ value: { stars: 4, name } }))
);
```

SQL:

```sql
INSERT OR IGNORE INTO "doc" ("value")
SELECT json_object('name', (SELECT s."content" FROM "__str" s WHERE s."__id" = b0."name"), 'stars', json('4'))
FROM "seed" b0
```

### 7.2 json document in head position, fully ground

```
doc_out({repo: Name}) <- seed(Name).
```

```js
seed$.pipe(
  map(({ name }) => ({ document: { repo: name } }))
);
```

A ground document collapses to one call, `json('{"repo":"cli"}')`, rendered by
`canonical_json_text/2`: the oracle's own canonicalizer, so the bytes agree by
construction rather than by two independent renderings.

### 7.3 the json_patch fold, candidate B

```
rel metric_sample(session: text, patch: json) log keep(all).
rel metric_doc(session: text, snapshot: json) key(1).

metric_doc(SessionId, Next) <+
  metric_sample(SessionId, Patch),
  pre(metric_doc(SessionId, Prior)),
  Next := json_patch(Prior, Patch).
```

This is the literal rxjs `scan` operator, one accumulator per session:

```js
metricSample$.pipe(
  groupBy(row => row.sessionId),
  mergeMap(session$ => session$.pipe(
    scan((snapshot, { patch }) => mergePatch(snapshot, patch), {})
  ))
);

// RFC 7396 §2, the same four behaviors the two doors implement
function mergePatch(target, patch) {
  if (patch === null || typeof patch !== "object" || Array.isArray(patch)) return patch;
  const merged = (target !== null && typeof target === "object" && !Array.isArray(target))
    ? { ...target } : {};
  for (const [key, value] of Object.entries(patch)) merged[key] = mergePatch(merged[key], value);
  return merged;
}
```

This matches the research doc's candidate B rx lowering with one difference,
stated: the doc wrote the accumulator as `{ ...doc, ...rowDoc }`, a SHALLOW
spread. That is not RFC 7396 -- a shallow spread replaces a nested object
instead of merging into it, which is exactly what the sabotage in §6 did and
exactly what `json_patch_merges_nested_objects_recursively` catches. The
recursive `mergePatch` above is the correct lowering.

### 7.4 the null stop

```
metric_doc(SessionId, Next) <+
  metric_sample(SessionId, Patch),
  pre(metric_doc(SessionId, Prior)),
  Next := json_patch(Prior, Patch).
```

with `+metric_sample(alpha, {cpu: none})` arriving.

```js
// no rx lowering, because there is no ruled semantics to lower.
scan((snapshot, { patch }) => {
  if (carriesNullStandIn(patch)) throw new Error("json_patch_null_unruled");
  return mergePatch(snapshot, patch);
}, {})
```

---

## 8. Gate output

Every number below was measured in this worktree.

```
conformance      379 PASS, 0 fail            (base 372 PASS, 0 fail)
plunit           1 known red only:
                 ERROR: test catalog_plane_rail:level_plane_family_corpus_counts: failed
                 (plunit_tests.pl:1312, KNOWN RED ON BASE, 614 tests)
text-door        TEXT_DOOR compiled=281 byte_identical=281 failures=0
roundtrip        G1 round-trip: 379 / 379 fixtures pass ; G1: ALL PASS ; G2: NO PARSE ERRORS
prolog-lint      PROLOG_LINT findings=0 baseline=0 OK
sweep            RUN total=279 identical=275 wrong=0 emitted_crash=0 rejection=4 no_oracle_log=0
                 FINAL total=279 final_identical=275 final_wrong=0 no_oracle_final=4
manifest         total 377 Counter({'compiled': 279, 'unsupported': 98})
```

The 4 sweep rejections, 3 pre-existing plus the new graded stop:

```
  REJECTION json_object_throws_on_duplicate_keys SQLITE_ERROR: malformed JSON
  REJECTION log_retraction_rejected retract from log rel 'event'
  REJECTION json_object_dup_key_rejected SQLITE_ERROR: malformed JSON
  REJECTION json_patch_null_stand_in_stops_both_doors SQLITE_ERROR: malformed JSON
```

### green-all delta

Both runs in this worktree, same machine, base `26f3f25f` measured first.

| | base | branch | delta |
|---|---|---|---|
| FAIL legs | 13 | 12 | -1 |
| legs turned red | | | **0** |

The one leg that moved:

```
< FAIL  staleness-gate         14s  exit=1     (base)
> PASS  staleness-gate         14s              (branch)
```

`staleness-gate` fails on base in a fresh worktree because the generated sweep
artifacts are stale there; regenerating them as part of this arc fixed it.

Branch FAIL set, all 12 also FAIL on base:

```
FAIL  compile-speed / flagship / getting-started / golden-flex / leak-soak /
      lsp-diags / memory-soak / plunit / rtkq-golden / scale-floor /
      serve-leak-soak / tsv2-test
```

`compile-speed`, `flagship`, `golden-flex`, `lsp-diags`, `plunit`,
`rtkq-golden`, `tsv2-test` are the brief's named KNOWN RED. The other five
(`getting-started`, `leak-soak`, `memory-soak`, `scale-floor`,
`serve-leak-soak`) fail identically on base in this worktree;
`leak-soak`'s reason is a `mktemp` collision on a shared `/var/folders` path,
which is a worktree-environment fact, not a code fact.

---

## 9. Forks for the user

Three, none settled by this lane.

### Fork 1: what spells JSON null

RFC 7396's delete clause needs one. Today `null` arrives as the atom `none`
(`0_json_arrival.pl:92`) and leaves as the string `"none"`
(`canonical_json_text/2`), so the two are indistinguishable in a document.
`body.pl` already treats `none` as the json-null stand-in on the READ side
(`json_capture_type/2` excludes it from `text`; `braces_decode/2` fails a bare
field pattern on it, graded by `decode_missing_key_fails_quietly`), so the
WRITE side reading it as an ordinary string is a live inconsistency. This is
ARCH.pl:855's Q2 null-collapse card, now with a second construct depending on
it. `json_patch` stops loudly on it rather than picking a side.

Options: (a) mint a distinct null term that renders as JSON `null`, (b) rule
that `none` IS null everywhere and make the writer render it as `null`, (c)
leave the stop in place.

### Fork 2: the empty object `{}` and the empty list `[]` in value position

Both are `atomic/1` in SWI, so they take `compile_expr`'s `atomic` arm before
the json arm is reached and store as TEXT, not `json`. Nested inside a
document they are handled correctly (`compile_json_element/4` renders them
through `canonical_json_text/2`), but at the TOP of a value they are not.
Reaching them means inserting a check ABOVE the `atomic` arm, which would take
the atom `{}` away from every program that uses it as an ordinary text value.
Deliberately not done. No fixture exercises it today.

### Fork 3: json column storage is not canonical, and the graded contract does not care

`lower.pl:5455-5462` states that canonicalization "has to happen once, on the
way in" and that reading a json column back through `json()` would be "a
second, weaker canonicalizer". `json_patch` breaks that: SQLite returns the
merged document in insertion order and the store keeps it that way. It is
byte-identical anyway because `ticklog.ts:45-52` canonicalizes json columns on
read. So the header's claim is already false in one direction and harmless in
the other. It becomes a real defect the moment a json column is compared with
`==`, used in a key, or grouped on. No such fixture exists.

### Candidate A is now cheap

The brief asked. Piece 1 built `compile_json_document/4` and
`json_document_operand_sql/3`; the `json_scalar` family and its operand
admission are in place. Candidate A (`json_set(Prior, concat(['$.', Key]),
Value)`) needs one more registry row, one `json_scalar_rendering/3` clause,
and an oracle `json_scalar_value/3` clause implementing json1 path semantics.
The measured gotcha from the research doc still stands: `json_set` upcasts an
integer bind param to float (`42` -> `42.0`), so the value must be emitted as
a literal or cast. Not in scope, not started.

---

## 10. ARCH ROW TO ADD

`ARCH.pl` is not this lane's to edit.

```prolog
task(json_as_value_in_scan, done, [json_flex_lab]). % LANDED 2026-08-11 (branch feature/json-as-value-in-scan, base 26f3f25f; plans/2026-08-11-json-as-value-in-scan.md). TWO PIECES. Piece 1: lower.pl:559's json_value_expression throw becomes a lowering arm -- a braces literal or list in value/head position renders as json_object/json_array with keys KEYSORTED at compile time (json1 keeps argument order, the log contract is sorted keys), a ground subtree through canonical_json_text/2 (the oracle's own canonicalizer, so bytes agree by construction), a bound json operand re-tagged through json(), a bool operand as true/false, a ref operand a named unsupported. Duplicate keys at any level emit json('json_dup_key'), the house pattern from lower.pl:5015. analyze.pl:expression_type/3 classifies the construct json so the column is json storage. json_value_expr/1 UNCHANGED in extension; the arm below still renders the json1 tagged term (pinned green before AND after). The throw's stated reason was STALE: it claimed the tick-log encoder renders obj(Pairs) as cons text, which json_flex fixed (ticklog.pl:132-159 emits real JSON, ticklog.ts:70 canonicalizes on read). Piece 2: candidate B of plans/2026-08-09-scan-into-json-research.md, json_patch/2 as RFC 7396 merge patch on a NEW registry family json_scalar (text_only rejects a json operand). Three of four RFC clauses land on both doors (§2 recursive merge, §2 else-return-patch for arrays and scalars, §2 non-object-target-empties); the §1/§2 null-delete clause STOPS on both doors because this language has no term that renders as JSON null (0_json_arrival.pl:92 folds null onto `none`, canonical_json_text renders `none` as "none") -- ARCH Q2 null-collapse, now with a second construct depending on it, escalated to the user rather than guessed. MEASURED on @libsql/client 3.45.1: json_patch does NOT sort keys ({"b":1}+{"a":2} -> {"b":1,"a":2}) yet json_patch_fold_result_is_key_sorted is byte-identical, because ticklog.ts canonicalizes json columns on read -- which refutes lower.pl:5455's "canonicalization has to happen once, on the way in". Receipts: manifest compiled 270->279 / unsupported 100->98 / fixtures 370->377; conformance 372->379 PASS 0 FAIL; TEXT_DOOR 272/272/0 -> 281/281/0; sweep RUN 279 identical=275 wrong=0 FINAL wrong=0 (4 rejections, one the new graded stop); G1 roundtrip 379/379; prolog-lint findings=0; green-all FAIL legs 13 -> 12, ZERO turned red. Fail-first both pieces (6/8 plunit red on the throw, all 7 json_patch fixtures red with the oracle holding the UNEVALUATED term = silently wrong on both doors). Sabotage: removing RFC 7396 §2's recursion in one line was caught on all three legs including a byte-level tick-log diff. THREE OPEN FORKS: null spelling; top-level {} / [] still take the atomic arm and store as text; json column storage not canonical and the graded contract does not care. Candidate A is now cheap (one registry row + one rendering clause + one oracle clause).
```
