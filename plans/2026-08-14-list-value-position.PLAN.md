# list(T) in expression position: the value is the interned list_id

Decided with Chris 2026-08-14: no type erasure. A list value traveling through
a column or an `:=` binding is the surrogate INTEGER id of a content-interned
list; elements rest in the minted member rel. json is transport only (compute
and boundary render), never the resting representation.

## TOC
1. Receipts (what exists today)
2. Contract (signatures first)
3. Slices
4. Validation
5. Ownership and laws

## 1. Receipts

| fact | where |
|---|---|
| split returns erased `json` | `v6/prolog/compile/registry.pl:291` `expression(split/2, typed_scalar, 3, split_json_array, typed([text, text], json))` |
| spread lowers to `json_each` re-parse per read | `v6/prolog/lower.pl:5294` `json_pattern_sql(spread(Sub), ...)` |
| `list(T)` mints entity + member rels | `v6/prolog/0_generic_expand.pl:700-708`: `member(list_id, idx, value)` keyed (1,2) |
| emitted mint DDL | `__gen__list_<t>_<hash>("__id" INTEGER PRIMARY KEY, "id" INTEGER, UNIQUE("id"))` + `__member("__id", list_id, idx, value, UNIQUE(list_id, idx))` |
| text interning precedent | `__str("__id" INTEGER PRIMARY KEY, content TEXT UNIQUE)`; intern machinery in lower.pl (grep `__str` / `default_intern_mode`) |
| list arrivals already decompose elements | `v6/prolog/conformance/fixtures/10_list_elements.pl` `rel_element_list_round_trips` |
| no compile_time declaration row for builtin constructors — INVARIANT, pinned | plunit expansion_order unit; absence keeps mints out of catalog row/11 |
| interned graph is a DAG (content id computed from children) | `v6/prolog/0_type_plane.pl:253` comment, verdict `interned_graph_is_a_dag` |

## 2. Contract

Type plane:
```prolog
% registry.pl — the only row change in slice 1
expression(split/2, typed_scalar, 3, split_list_intern, typed([text, text], list(text))).
```

Lowering, producer side (`Parts := split(Text, Sep)`):
```prolog
% lower.pl — pseudo
% 1. compute: json array SQL, the existing split_json_array rendering, unchanged
% 2. intern:  INSERT OR IGNORE INTO <entity>(content) SELECT <array_text>;
%             list_id = (SELECT __id FROM <entity> WHERE content = <array_text>)
%             INSERT rows into <member> (list_id, idx, value)
%             SELECT list_id, key, value FROM json_each(<array_text>)
%             ON CONFLICT(list_id, idx) DO NOTHING   -- content-interned: rows are immutable
% 3. bind:    the column value IS list_id (INTEGER)
```
Entity table gains a `content TEXT UNIQUE` column (the rendered canonical json
text) — this is the dictionary-table law applied to lists; the natural key
(content) lives ONCE, the id travels. One insert per rule evaluation for the
member set, never per element row (N+1 law: build the set, one insert).

Consumer side (`decode(Parts, [... Element])` where `Parts : list(T)`):
```prolog
% when the source column's type is list(T): join, do not re-parse
%   FROM <member> m WHERE m.list_id = <Parts> ORDER BY m.idx
%   Element binds m.value, typed T
% when the source is untyped json: existing json_each path, unchanged
```

rx lowering (law: every construct states it): producer = map (compute parts) →
scan (interning map: content → id, emit member rows once on miss) → the bound
id flows as a plain column value. Consumer spread = mergeMap over the member
join. Equality of two list values = `=` on ids (content interning makes ids
canonical).

Instance lifetimes: interned list rows are immutable and permanent within a db
(same lifecycle as `__str` rows); no refcount in this arc — garbage lists are
dead dictionary rows, same as dead `__str` rows.

Storage layout: entity `(__id INTEGER PK, content TEXT UNIQUE)`; member
`(__id INTEGER PK, list_id INT, idx INT, value <T-lowered>, UNIQUE(list_id, idx))`.
Reads: consumer joins member by list_id (SEARCH via the UNIQUE index, never
SCAN). Writes: intern-once per distinct content. Uniqueness: content UNIQUE on
entity; (list_id, idx) UNIQUE on member.

## 3. Slices (one commit each, fail-pre-fix plunit test first per slice)

1. **Entity content column + intern statements.** Extend the `list(T)` mint
   with `content TEXT UNIQUE` and emit intern statements in lowering. Existing
   fixtures must stay byte-identical where they do not use expression-position
   lists (expect the mint DDL diff to touch list fixtures only; report the
   exact fixture set).
2. **Registry + producer lowering.** Flip the split row to
   `typed([text,text], list(text))`, add `split_list_intern` rendering. The
   old `split_json_array` rendering stays (other json producers may use it).
3. **Typed spread consumer.** `decode` over a `list(T)`-typed source joins the
   member rel. Add an EXPLAIN-based COUNT test showing SEARCH (index on
   list_id, idx), never SCAN, on the consumer path.
4. **Oracle parity.** `v6/prolog/conformance/body.pl`: oracle split answers a
   list value whose identity matches the interned id semantics (oracle may
   model ids opaquely; final-state equality is on decomposed rows).
   Fixtures: `v6/prolog/conformance/fixtures/19_list_value_position.pl` —
   split-into-typed-list round trip, two rows with the same parts sharing one
   list_id (assert via the member rel: one list, two referrers), element type
   flows through spread (initcap over parts still compiles), empty separator
   edge unchanged.

## 4. Validation (run all, per slice; never two grade.sh in one shell line)
```
cd v6/prolog/conformance && swipl -g go -t halt go.pl      # 421+N PASS / 0 FAIL
cd v6/tsv2 && bash scripts/sweep.sh                         # RUN wrong=0
cd v6/prolog && swipl -q -l compile/test/plunit_tests.pl -g run_tests -g halt
                                                            # failing set EXACTLY .github/CI-KNOWN-RED.md (5 names)
bash v6/sprefa-engine-rs/grade.sh                           # graded>=421, byte-clean: report any regression below 313 with the fixture names
```
Baselines measured 2026-08-14 on 91da6781: conformance 421/0, sweep RUN
total=317 identical=314 wrong=0 rejection=3, plunit 5 known-red, RUST-GRADE
graded=421 byte-clean=313.

## 5. Ownership and laws
- First action: `git merge --ff-only 91da67816f00255d18d12094ea7cb11a9a896c70`;
  failure = STOP AND REPORT.
- Files owned: `v6/prolog/compile/registry.pl`, `v6/prolog/lower.pl`,
  `v6/prolog/0_generic_expand.pl`, `v6/prolog/conformance/body.pl`,
  `v6/prolog/conformance/fixtures/19_list_value_position.pl` (new),
  `v6/prolog/compile/test/plunit_tests.pl`, plus gate-regenerated files
  (`compile/out/**`, `graded.tsv`).
- FORBIDDEN: `v6/prolog/compile/parse_dl_dcg.pl` (no surface change exists in
  this arc), `conformance/fixtures/17_*` `18_*` (another lane owns them),
  everything under `v6/sprefa-engine-rs/src/**`, `v6/tsv2/**` sources.
- A construct you cannot lower is a REPORT line with the throw site, never a
  silent scope cut. No new `eprintln!`. Comments only for constraints code
  cannot show. Banned words incl. in identifiers: provenance, substrate,
  load-bearing, regime, refusal.
- dl variable names descriptive in every snippet and fixture.
