# scan-into-json research: does the refusal hold?

Base `7b4af568`. Research + design only, zero compiler changes. Frame: a
refusal is a hypothesis, never an edict; trace `json_value_expression` to its
throw site and classify it (real impossibility vs unfinished work) before
proposing anything.

## TOC
1. Throw site
2. Manifest: every json-related refusal reason
3. Oracle behavior: one-door or both-door
4. What already works write-side (`json_group_array`) and its ORDER BY history
5. SQLite capability check on the pinned runtime
6. Verdict: one-door, unfinished work
7. Proposal: three candidate spellings, each with rx lowering, SQL, and phase
8. Verdict table

---

## 1. Throw site

```
$ grep -rn "json_value_expression" v6/prolog/*.pl v6/prolog/compile/*.pl
v6/prolog/ARCH.pl:27:%   literal in value or head position raises json_value_expression, and
v6/prolog/lower.pl:517:    -> throw(unsupported_construct(json_value_expression(Expr)))
```

The clause, `lower.pl:compile_expr/7` (the expression compiler that turns a
dl6 RHS into SQL text), one arm of a chain of `->` guards:

```prolog
    ; json_value_expr(Expr)
    -> throw(unsupported_construct(json_value_expression(Expr)))
    ; compound(Expr)
    -> Expr =.. [Functor | SubArgs], ...
```

`json_value_expr/1` (lower.pl:596-598):

```prolog
json_value_expr(Expr) :- compound(Expr), Expr = {}(_), !.
json_value_expr(Expr) :- is_list(Expr), Expr \== [], !.
json_value_expr(Expr) :- compound(Expr), Expr = [_ | _].
```

**Trigger condition:** the RHS of `:=` (or a head argument, same `compile_expr`
call) is a braces literal `{...}` or a non-empty list.
**Phase:** lowering (SQL compilation), which runs AFTER parse and AFTER the
static analysis pass — `parse_dl.pl` reads `:=`'s RHS as a generic term with
no function allow-list (confirmed: no `json_value_expr`, `json_patch`,
`json_set` hits anywhere in `parse_dl.pl`; the RHS grammar is unconditional
term-read, `parse_dl.pl:1500` `Var := Expr | Var is Expr`). So every candidate
below already PARSES; the refusal is purely a lowering-phase gate.

**Why the throw exists at all** (lower.pl:587-595, load-bearing comment):

> MEASURED, not predicted: with only the bind lift in place and no unsupported
> construct here, json_arm.pl's `braces_literal_canonicalizes` compiled clean
> and stored the text `"null"` where the oracle holds
> `obj([name-cli, stars-4])`, and `braces_in_head_position` ... stored
> `{}({"fn":":","args":["repo","cli"]})`.

The throw is a fail-safe against a real, measured wrong-value bug: without
it, the generic compound-term branch (lines 518-526, designed for domain
terms like `route_data(RouteId)`) silently wraps a braces literal in the
`json_object('fn', Functor, 'args', json_array(...))` tagged-term encoding —
correct for a domain fact, silently wrong for a JSON value. The refusal is
real but its cause is a MISSING lowering arm for `{}/1` and list literals,
not an impossibility: SQLite has a native `json_object(...)` builtin that
would render the correct JSON directly, and it is unused anywhere in this
path.

## 2. Manifest: every json-related refusal reason

```
$ grep -o '"reason":"[^"]*json[^"]*"' v6/prolog/compile/out/manifest.json | sort | uniq -c
      2 "reason":"aggregate_head(json_array(A))"
      2 "reason":"aggregate_head(json_object(A,B))"
      1 "reason":"edge_body_needs_json_destructure((demand_row(A,B),decode(B,fresh(C,D))))"
      5 "reason":"edge_body_needs_json_destructure((demand_row(A,B),decode(B,fresh(C,D)),stars_of(D,E)))"
      3 "reason":"edge_body_needs_json_destructure((demand_row(A,B),decode(B,fresh(C,D)),stars_of(D,E),E>100,not(muted(A))))"
      1 "reason":"json_capture_type_unknown(bool)"
      1 "reason":"json_capture_type_unknown(itn)"
      1 "reason":"json_value_expression({repo:A})"
      1 "reason":"json_value_expression({stars:4,name:A})"
      2 "reason":"level_body_goal(pull_request(A,B,C,D,E),json_each(F,G))"
      1 "reason":"level_body_goal(repo_lang(A),json_each(B,A))"
      1 "reason":"level_body_goal(repo_lang(A,B),json_each(C,B))"
      1 "reason":"type_arrival_shape_mismatch(batch/2,payloads,list(json),field_not_array(42))"
```

The two `json_value_expression` fixtures are `json_arm.pl:braces_literal_canonicalizes`
and `json_arm.pl:braces_in_head_position`.

## 3. Oracle behavior: one-door or both-door

Ran the full reference-interpreter suite:

```
$ swipl -q -l conformance/go.pl -g go -g halt   # from v6/prolog
... 334 lines, all PASS, zero FAIL ...
$ ... | grep -c FAIL
0
$ ... | grep -c PASS
334
$ ... | grep -E "braces_literal_canonicalizes|braces_in_head_position|json_round_trip_decode_to_document"
PASS  braces_literal_canonicalizes
PASS  braces_in_head_position
PASS  json_round_trip_decode_to_document
```

**One-door refusal.** The oracle (`v6/prolog/conformance/engine.pl` +
`level_eval.pl`) executes both `json_value_expression`-refused fixtures
correctly, and the whole 334-fixture suite passes with zero failures. By
this repo's own definition (`CLAUDE.md`, "a refusal is a hypothesis"), a
one-door refusal is unfinished work by construction: the semantics are
specified and proven on one door, only the SQL-emitting door lacks the arm.

## 4. What already works write-side, and its ORDER BY history

```
$ grep -n "json_group_array\|json_group_object" v6/prolog/lower.pl
4777:aggregate_select_expr(Mode, agg(json_group_array, Expr), Bound, Sql, direct) :- !,
4780:    format(atom(Sql), 'json_group_array(~w ORDER BY ~w)', ...
4782:aggregate_select_expr(Mode, agg(json_group_array_ordered, ValueExpr-OrdinalExpr), ...
4787:    format(atom(Sql), 'json_group_array(~w ORDER BY ~w)', ...
5207:% PRESERVES key order and json_group_object follows row order -- so
```

`json_group_array/1` and `/2` are `head(lower)` in `registry.pl:171-172` and
DO already emit `json_group_array(value ORDER BY ordinal-or-value)`. There is
no `json_group_object` lowering arm anywhere in `lower.pl` — line 5207 is a
comment, not code. `registry.pl:169-170`:

```prolog
surface(json_array/1,       aggregate, no_refs, head(refuse(aggregate)), refused).
surface(json_object/2,      aggregate, no_refs, head(refuse(aggregate)), refused).
```

`json_object/2` is the surface users write; it is refused, not lowered.
ARCH.pl's `json_wiring` task row records the stated reason as "json_group_object
row order vs sorted keys; no ORDER BY slot in the flat aggregate SELECT" —
but `json_group_array` already carries an `ORDER BY` inside its own
aggregate-call SELECT (line 4780/4787), so a "no ORDER BY slot" claim is
contradicted by code sitting nine lines away in the same file. Section 5
measures whether the SQL itself would work; the stale-refusal-reason
question is a second, separate finding.

`analyze.pl:1684-1685` — `classify_head_arg` ALREADY classifies
`json_object(KeyExpr, ValueExpr)` as `agg(json_object, KeyExpr-ValueExpr)`
unconditionally (first clause, cut, no registry check). The refusal fires
one layer up, in `refused_aggregate_head_shape/2` (analyze.pl:1718-1723),
which separately checks the registry's `refuse(aggregate)` tag and throws
before lowering is ever reached (analyze.pl:1616-1617). The oracle-side
implementation is also already present and passing:
`level_eval.pl:293-297`:

```prolog
agg_compute(json_object, Pairs, obj(Object)) :-
    sort(Pairs, Distinct), keysort(Distinct, Object),
    pairs_keys(Object, Keys),
    ( sort(Keys, DistinctKeys), length(Keys, N), length(DistinctKeys, N)
    -> true ; throw(json_object_dup_key(Keys)) ).
```

`json_object_builds_document` and `json_object_dup_key_rejected` both PASS
under the oracle (confirmed in the section 3 run).

## 5. SQLite capability check on the pinned runtime

`v6/tsv2/package.json:22`: `"@libsql/client": "^0.17.4"`. Repo comments
(`lower.pl:329`, `:465`, `analyze.pl:1538`) already record the bundled engine
as sqlite 3.45.1, distinct from the system CLI (3.43.2, confirmed
`sqlite3 --version` here). ORDER BY inside an aggregate function call
(SQLite 3.44+) fails on the system CLI but the repo's own driver is newer;
tested directly against the real `@libsql/client` module (from
`~/projects/sprefa/v6/tsv2/node_modules`, this worktree has no
`node_modules`):

```
$ node -e "... db.execute('select sqlite_version() as v') ..."
sqlite_version [{"v":"3.45.1"}]

$ node -e "... json_group_object(k, v ORDER BY k) ..."
json_group_object ORDER BY -> [{"doc":"{\"a\":1,\"b\":2,\"c\":3}"}]

$ node -e "... json_group_array(v ORDER BY v) ..."
json_group_array ORDER BY -> [{"arr":"[1,2,3]"}]

$ node -e "... json_patch('{\"a\":1}', '{\"b\":2}') ..."
json_patch -> [{"doc":"{\"a\":1,\"b\":2}"}]

$ node -e "... json_set('{}', '$.' || 'cpu', 42) ..."
json_set dynamic path -> [{"doc":"{\"cpu\":42.0}"}]   # NOTE: int -> float via libsql param binding

$ node -e "... json_group_object(k,v) with duplicate k=name twice ..."
dup key json_group_object -> [{"doc":"{\"name\":\"cli\",\"name\":\"shell\"}"}]
```

Measured, on the exact pinned driver:
- `json_group_object(k, v ORDER BY k)` — works. Contradicts the "no ORDER BY
  slot" refusal reason at face value; the slot exists, it is unused.
- `json_group_array(v ORDER BY v)` — already what's shipped.
- `json_patch(a, b)` — RFC 7396 merge patch, works, single native call.
- `json_set(doc, '$.' || key, value)` — dynamic key path via string concat,
  works, but silently upcasts an integer bind param to float (`42` ->
  `42.0`); a real gotcha, not a blocker, needs a cast or literal-not-param
  emission to avoid.
- `json_group_object` on a duplicate key does NOT throw and does NOT dedupe
  — it emits both pairs into the object text (`{"name":"cli","name":"shell"}`).
  SQLite's own docs do not promise last-write-wins here; matching the
  oracle's `json_object_dup_key` throw needs an explicit guard, not a bare
  aggregate call.

## 6. Verdict

**One-door refusal, unfinished work, not an impossibility.** The oracle
passes all 334 fixtures including both `json_value_expression`-refused ones.
The write path already has one fully-lowered json aggregate
(`json_group_array`, ORDER BY and all) whose pattern is directly reusable.
SQLite on the pinned runtime supports every primitive candidate #3 below
needs (`json_group_object` with `ORDER BY`, `json_patch`, `json_set`).

---

## 7. Proposal: candidate spellings for scan-into-json

Golden-demo shape used across all three: a stream of metric samples folds
into one running snapshot document.

```
metric_sample(SessionId, MetricName, MetricValue).   % arrival rel
metric_doc(SessionId, Snapshot).                      % keyed json doc, one row per session
```

### Candidate A — `json_set` fold over a dynamic path

```prolog
metric_doc(SessionId, Next) <+
    metric_sample(SessionId, MetricName, MetricValue),
    pre(metric_doc(SessionId, Prior)),
    Next := json_set(Prior, concat(['$.', MetricName]), MetricValue).
```

rx (this is the literal rxjs `scan` operator, one accumulator per session):

```js
metricSample$.pipe(
  groupBy(row => row.sessionId),
  mergeMap(session$ => session$.pipe(
    scan((doc, { metricName, metricValue }) =>
      ({ ...doc, [metricName]: metricValue }), {})
  ))
);
```

SQL each occurrence emits (single-row upsert, `pre/1`'s existing
`__pre_metric_doc` snapshot join feeds `Prior`):

```sql
INSERT INTO metric_doc (session_id, snapshot)
VALUES (:session_id,
        json_set(:prior, '$.' || :metric_name, :metric_value))
ON CONFLICT(session_id) DO UPDATE SET snapshot = excluded.snapshot;
```

Phase/arm it would ride: `pre/1` read (already live,
`lower.pl:920-933 catalog_pre_plane_row`, proven by
`occurrence_identity.pl:concat_fold_follows_arrival_order`, the text-concat
sibling of this exact shape) + keyed edge-rule UPSERT
(`lower.pl:2722-2726`, already live) + a NEW `compile_expr` arm parallel to
`text_scalar_expr` (lower.pl:543-546) that accepts JSON-typed and TEXT
operands instead of `compile_text_operand`'s TEXT-only gate
(`lower.pl:548-553`), plus a new `registry.pl` row for `json_set/3`.

### Candidate B — `json_patch` fold over pre-shaped JSON rows

```prolog
metric_doc(SessionId, Next) <+
    metric_sample_doc(SessionId, RowDoc),   % RowDoc already json-typed, e.g. {cpu: 42}
    pre(metric_doc(SessionId, Prior)),
    Next := json_patch(Prior, RowDoc).
```

rx:

```js
metricSampleDoc$.pipe(
  groupBy(row => row.sessionId),
  mergeMap(session$ => session$.pipe(
    scan((doc, rowDoc) => ({ ...doc, ...rowDoc }), {})
  ))
);
```

SQL:

```sql
INSERT INTO metric_doc (session_id, snapshot)
VALUES (:session_id, json_patch(:prior, :row_doc))
ON CONFLICT(session_id) DO UPDATE SET snapshot = excluded.snapshot;
```

Phase/arm: identical `pre/1` + UPSERT machinery as candidate A. The
`compile_expr` arm is simpler than A's — no dynamic path string to build,
both operands are already `direct`-encoded json-column text
(`canonical_column_expr`'s existing json case, `lower.pl:5212-5214`), so the
rendering is close to `text_scalar_rendering`'s pass-through clause
(`lower.pl:573-576`) generalized to two json args. Registry needs one new
row, `json_patch/2`. Caveat: this candidate does NOT fix the brace-literal
bug (section 1) — it sidesteps it by requiring `RowDoc` to already be
JSON-shaped on arrival (host-supplied or via `decode`/`spread`, both of
which already compile on the read side).

### Candidate C — `json_group_object` aggregate head (batch, non-incremental)

```prolog
metric_doc(SessionId, json_group_object(MetricName, MetricValue)) <-
    metric_sample(SessionId, MetricName, MetricValue).
```

rx (this is NOT the streaming `scan` operator — it's a full recompute per
tick over the current row set, the same shape `count`/`sum`/`json_group_array`
already use, including on retraction, per
`aggregate_min_recomputes_when_the_minimum_is_retracted`):

```js
metricSample$.pipe(
  groupBy(row => row.sessionId),
  mergeMap(session$ => session$.pipe(
    toArray(),
    map(rows => Object.fromEntries(
      rows.slice().sort((a, b) => a.metricName < b.metricName ? -1 : 1)
          .map(r => [r.metricName, r.metricValue])))
  ))
);
```

SQL, riding `aggregate_select_expr`'s existing `json_group_array` pattern
almost verbatim:

```sql
SELECT session_id, json_group_object(metric_name, metric_value ORDER BY metric_name) AS snapshot
FROM metric_sample
GROUP BY session_id;
```

Phase/arm: `aggregate_select_expr` (lower.pl:4777-4788) gets ONE new clause,
copy-shaped from the `json_group_array_ordered` clause already there:

```prolog
aggregate_select_expr(Mode, agg(json_group_object, KeyExpr-ValueExpr), Bound, Sql, direct) :- !,
    compile_expr(Mode, value, KeyExpr, Bound, KeySql, _, _),
    compile_expr(Mode, value, ValueExpr, Bound, ValueSql, ValueType, _),
    json_group_array_value_sql(ValueType, ValueSql, AggregateValueSql),
    format(atom(Sql), 'json_group_object(~w, ~w ORDER BY ~w)',
           [KeySql, AggregateValueSql, KeySql]).
```

`classify_head_arg` (analyze.pl:1684-1685) already routes `json_object/2`
into `agg(json_object, KeyExpr-ValueExpr)` unconditionally; the only gate is
`registry.pl:170`'s `head(refuse(aggregate))` -> `head(lower)` flip plus
renaming the classified kind to match the new `aggregate_select_expr` clause
(or adding `json_group_object` as the registry's live surface name instead
of reusing `json_object`, matching the `json_array`/`json_group_array`
naming split already in the registry). The oracle side needs zero new code
(`level_eval.pl:293-297` already computes this, already passing). One real
gap: SQLite's own `json_group_object` does not throw on a duplicate key
(measured section 5); matching the oracle's `json_object_dup_key` contract
needs an explicit `HAVING count(DISTINCT metric_name) = count(metric_name)`
style guard per group, the same shape as `lower.pl`'s existing
`keyed_conflict` guards elsewhere in the file.

## 8. Verdict table

| spelling | parser cost | lowering cost | oracle cost | matches existing machinery |
|---|---|---|---|---|
| A: `json_set` fold, dynamic path | none — `:=` RHS is a generic term, no allow-list at parse | medium — new `compile_expr` arm (JSON+TEXT operand mix, not TEXT-only), new `registry.pl` row, path built via `concat` | medium — `json_set` path-based patch has no oracle predicate yet, needs a new `agg`-free scalar in `engine.pl`/`level_eval.pl` matching SQLite path semantics | medium — reuses proven `pre/1` fold idiom (`concat_fold_follows_arrival_order`) and keyed UPSERT, but the JSON-scalar operand path is new |
| B: `json_patch` fold, pre-shaped rows | none | low — near pass-through rendering on two already-`direct`-encoded json columns, one registry row | medium — RFC 7396 merge-patch has no oracle predicate yet, but semantics are a well-specified standard, not an invented one | high — identical `pre/1` + UPSERT skeleton as A with less new SQL-generation code; doesn't touch the brace-literal bug at all |
| C: `json_group_object` aggregate head | none — construct already classified in `analyze.pl`, sits behind a registry flag | lowest — ~6-line clause copy-shaped from the already-shipped `json_group_array_ordered` arm | lowest — `level_eval.pl:293-297` already implements and passes both fixtures today | highest — nearly line-for-line reuse of `json_group_array`'s landed, measured lowering arm; needs a duplicate-key guard to keep oracle parity |

Candidate C is not the streaming `scan` operator (it recomputes the group
each tick, like every other aggregate head in this engine); candidates A and
B are. All three are additive registry + lowering-arm work, none require
touching the parser or the brace-literal bug directly, though fixing that
bug (section 1) would let a future candidate build `RowDoc` for B inline
with a literal instead of requiring a pre-shaped json column.
