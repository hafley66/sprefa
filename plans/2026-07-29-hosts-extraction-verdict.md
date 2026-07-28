# HOSTS + EXTRACTION VERDICT

Contract: `plans/2026-07-29-hosts-extraction-lab-header.md`.

Lab entry:
`v6/prolog/labs/hosts_extraction/lab.pl`.

Run from `v6/prolog`:

```text
swipl -q -l labs/hosts_extraction/lab.pl -g go -g halt
```

Receipt: 41 PASS lines, exit 0, PASS-only stdout, repeated twice. The
conformance and roundtrip receipts are recorded under Verification.

## Verdict line

The follow-up compiler term inventory is:

```prolog
sh_decl(Name, InputColumns, OutputColumns, template(Text))
probe(Name, InputValues, OutputValues, SaltColumns)
bind_decl(RelName, Columns)
query(RelAtom)
ts_query(Patterns)
sg_pattern(language(Language), source(Text), captures(Names))
```

Extraction code patterns use link-registered host relations. Their rows arrive
as EDB deltas addressed by `(file_digest, query_digest)`. JSON remains the
decode-class term precedent. The `ts_query/1` term compiles losslessly to
tree-sitter query text. Ast-grep retains `sg_pattern/3` because its
metavariables carry target-language pattern semantics absent from tree-sitter
query captures.

The selected source spellings are:

```dl
sh fetch(ep: text, prev: text) ->
  (status: int, tag: text, body: text) =
  `template with {ep} and $prev`.

bind interval(period: int, bucket: int).

? change_log(ep, kind, value).
```

Their term forms are the first four terms in the inventory. Their rx
lowerings are RX-H1, RX-B1, and RX-Q1 in the lowering table below.

## Lowering law

Every proposed spelling used in this verdict has a row here. A row with no
direct rx expression would be refused. All selected rows have expressions.

| id | spelling or term | rx lowering |
|---|---|---|
| RX-H1 | `sh_decl(fetch, Inputs, Outputs, template(Text))` | `request$.pipe(groupBy(r => r.witnessDigest), mergeMap(g => g.pipe(take(1), mergeMap(runShell))), mergeMap(decodeDeclaredOutputs), mergeMap(commitEdbArrival))` |
| RX-H2 | `probe(fetch, Inputs, Outputs, Salts)` | `inputRows$.pipe(map(mintIdentityAndWitness), distinct(r => r.witnessDigest), mergeMap(demandHost))`; identity digest covers host plus inputs, witness digest additionally covers salts |
| RX-B1 | `bind_decl(interval, Columns)` | `defer(readBindDecls).pipe(switchMap(ds => ds.has("interval") ? interval(periodMs, scheduler).pipe(map(toBucketRow), mergeMap(commitEdbArrival)) : EMPTY), takeUntil(programSwap$))` |
| RX-B2 | `bind_decl(clock, Columns)` | same graph as RX-B1 with registry key `clock`; honest lowering, refused by the rx-name vocabulary law |
| RX-B0 | zero-declaration rel-name activation | `defer(readProgramEdbRels).pipe(switchMap(rs => rs.has(bind.rel) ? bind.source$ : EMPTY))`; honest lowering, refused because an ordinary rel name grants world-feed activation |
| RX-Q1 | `query(RelAtom)` | `defer(() => from(store.scanQuery(queryPlan)))`; the reader receives rows from SQLite and does not retain the table |
| RX-Q2 | `query(Name, Args)` | `defer(() => from(store.scanQuery(queryPlan(Name, Args))))`; honest lowering, extra term split |
| RX-Q0 | omitted query | refused: no observable supplies the requested result rows |
| RX-J1 | `decode(Body, {stargazers_count: N})` | `currentBody$.pipe(map(({ep, body}) => ({ep, n: body.stargazers_count})))` |
| RX-J2 | `json_each(Body, Item), decode(Item, Pattern)` | `currentBody$.pipe(mergeMap(({ep, body}) => from(body).pipe(map(item => ({ep, num: item.number, title: item.title, state: item.state, author: item.user.login})))))` |
| RX-EH1 | host extraction `probe(sg, [FileDigest, QueryDigest], Outputs, [])` | `demand$.pipe(groupBy(r => r.fileDigest + ":" + r.queryDigest), mergeMap(g => g.pipe(take(1), mergeMap(runHostExtract))), mergeMap(commitEdbArrival))` |
| RX-ET1 | term extraction `extract(Content, Query, Match)` | `contentDelta$.pipe(mergeMap(({content, query}) => from(extract(content, query))), map(mintRuleRow))` |
| RX-TS1 | `ts_query(Patterns)` used as a host query value | `fileDemand$.pipe(groupBy(r => r.fileDigest + ":" + r.queryDigest), mergeMap(runTreeSitterQuery), mergeMap(commitEdbArrival))` |
| RX-SG1 | `sg_pattern(language(rust), source("$RECEIVER.unwrap()"), captures([receiver]))` | `fileDemand$.pipe(groupBy(r => r.fileDigest + ":" + r.patternDigest), mergeMap(runAstGrepPattern), mergeMap(commitEdbArrival))` |

## Q1. Shell host declaration and probe

### Term verdict

Selected term:

```prolog
sh_decl(
  fetch,
  [col(ep, text), col(prev, text)],
  [col(status, int), col(tag, text), col(body, text)],
  template("template with {ep} and $prev"))
```

Lowering: RX-H1.

Selected probe:

```prolog
probe(
  fetch,
  [Ep, Prev],
  [Status, Tag, Body],
  [salt(bucket, Bucket)])
```

Lowering: RX-H2.

### Explicit versus inferred pricing

Both candidates compile to the same `host_plan/4` in the lab.

| criterion | explicit split | inferred from template |
|---|---|---|
| term | `sh_decl(Name, Inputs, Outputs, Template)` | `sh_decl_inferred(Name, AllColumns, Template)` |
| list boundaries | 2 column lists | 1 column list |
| mode stability when template text changes | declared mode remains stable; mismatch is refused | mode changes with the template reference set |
| unknown `{column}` | refused | the referenced declared column becomes an inferred input |
| declared input absent from template | refused | column becomes an output |
| output referenced as `{name}` or `$name` | refused | column becomes an input |
| compiler work | validate references against explicit sets | scan references, partition columns, then validate |
| rx lowering | RX-H1 | RX-H1 after inferred plan construction |
| executable checks | explicit plan, three mismatch classes, overlap | same-plan equivalence |

Selection criterion: a template edit must not silently alter the host function
signature. The explicit split adds one list boundary and turns all such changes
into declaration errors.

### Content address and salts

For:

```text
probe(fetch, ["repo", "etag"], [Status, Tag, Body], [salt(bucket, 9)])
```

Lowering: RX-H2.

| field | minted value |
|---|---|
| request identity | `identity_digest(host(fetch, [ep="repo", prev="etag"]))` |
| witness identity | `witness_digest(host(fetch, [ep="repo", prev="etag"], [bucket=9]))` |
| same input and same bucket | one in-flight request and one cache row |
| same input and bucket 10 | same request identity, fresh witness identity |
| response columns | exact declared order `status, tag, body` |

The salt is a column value. It carries no subscription identity and no arrival
tick.

### Refusal shapes

| malformed declaration or probe | refusal |
|---|---|
| declared input absent from template | `template_mismatch(unreferenced_input(Name))` |
| declared output referenced by template | `template_mismatch(output_used_as_input(Name))` |
| unknown brace reference | `template_mismatch(unknown_column(Name))` |
| input and output reuse one name | `column_mismatch(input_output_overlap(Name))` |
| duplicate column name | `column_mismatch(Role, duplicate(Name))` |
| probe input or output arity differs from declaration | `probe_mismatch(Probe)` |

### Ghcacher worked term

`0_terms.pl` carries the complete `program(Decls, Rules, Queries)` value. It
contains the selected `sh_decl/4`, salted `probe/4`, `bind_decl/2`,
`decode/2`, `json_each/2`, and final `query/1`. The lab compiles the whole
program to:

```prolog
compiled(
  [host_plan(fetch, Inputs, Outputs, Template)],
  [interval],
  [query_plan(change_log/3, Columns, snapshot(current))])
```

Lowerings: RX-H1, RX-H2, RX-B1, RX-J1, RX-J2, RX-Q1.

## Q2. Bind declaration

### Spelling price

| candidate | vocabulary source | activation | EDB classification | rx lowering | result |
|---|---|---|---|---|---|
| `bind interval(period: int, bucket: int).` | rx `interval` | matching registered bind plus exact column shape | explicit bind declaration | RX-B1 | selected |
| `bind clock(secs: int, bucket: int).` | legacy engine name | matching registered bind | explicit bind declaration | RX-B2 | refused by the rx-name law |
| `rel clock_bucket(period: int, bucket: int).` with registry name match | project-specific compound name | any never-headed relation with that name activates | EDB by absence | RX-B0 | reproduced as the magic-rel hazard and refused |

Selected term:

```prolog
bind_decl(interval, [col(period, int), col(bucket, int)])
```

Lowering: RX-B1.

The relation name performs link selection. The declaration performs
authorization. A plain relation declaration with the same name leaves the bind
cold.

### EDB consequence

| declaration and heads | origin |
|---|---|
| `bind_decl(interval, Columns)`, zero rule heads | `edb(bind_declaration)` |
| plain `rel_decl(input, Columns)`, zero rule heads | `edb(never_headed)` |
| plain relation with a rule head | `idb(rule_head)` |
| bind declaration plus a rule head of the same name | `refused(bind_and_rule_head(Name))` |

This extends `edb_definition`: absence still classifies ordinary pure subjects,
and a bind declaration classifies the named relation directly.

Lifecycle:

1. Program load reads bind declarations.
2. Registry match validates the emitted column shape.
3. Subscription to the program graph subscribes the cold bind source.
4. Each bind emission commits one EDB batch.
5. Program replacement unsubscribes the source through `switchMap`.

Storage and uniqueness:

- emitted rows stay in SQLite;
- host code observes commit completion and deltas;
- `(period, bucket)` is the content row;
- recurrence comes from rx `interval`;
- the bucket is witness data;
- no bind cache or subscription salt exists.

## Q3. Query line

Selected term:

```prolog
query(change_log(Ep, Kind, Value))
```

Lowering: RX-Q1.

| criterion | result |
|---|---|
| relation name and arity | retained as `change_log/3` |
| variable identity | retained inside the relation atom |
| snapshot meaning | `snapshot(current)` |
| table residency | SQLite |
| host residency | streamed query rows only |
| ghcacher placement | the sole element of `program/3` Queries |

Pricing:

| candidate | nesting | relation atom retained | rx lowering | result |
|---|---:|---:|---|---|
| `query(RelAtom)` | 1 wrapper | yes | RX-Q1 | selected |
| `query(Name, Args)` | 2 fields | reconstructed | RX-Q2 | adds a term split |
| omit query and retain a parser finding | 0 | discarded | RX-Q0 refusal | cannot compile the worked program |

## Q4. JSON field pull and correlated array explode

The executable `ghcacher_json_normalization` fixture runs against the landed
conformance `decode/2` and `json_each/2`.

Field pull:

```prolog
stars(Ep, N) <-
  current_body(Ep, Body),
  decode(Body, {stargazers_count: N})
```

Lowering: RX-J1.

Array explode:

```prolog
pull_request(Ep, Num, Title, State, Author) <-
  current_body(Ep, Body),
  json_each(Body, Item),
  decode(Item,
         {number: Num, title: Title, state: State,
          user: {login: Author}})
```

Lowering: RX-J2.

The fixture yields:

```text
stars(repo, 17)
pull_request(pulls, 7, "seven", "open", "octo")
pull_request(pulls, 8, "eight", "closed", "hub")
```

`Item` is the correlation boundary. One decode sees the sibling fields and
the nested `user.login` from one array element.

Residue:

| slot | dependency |
|---|---|
| `slot_json_text_to_value` | a typed shell output declared `json` needs a library decoder from stdout text to the canonical JSON value before `decode/2`; parse failure needs the host failure-as-value envelope |

The JSON matching and fan-out shapes are expressible. The typed host output
decoder is follow-up wiring.

## Q5. Extraction fork

### Same worked examples

The canned rows are identical for both shapes.

| example | old digest result | changed digest result | boundary delta |
|---|---|---|---|
| callgraph sg | `call(foo, bar, 0, 3)` | `call(foo, zap, 0, 3)` | `[-call(foo,bar,0,3), +call(foo,zap,0,3)]` |
| span-line scan | `span_line(10, "old")` | `span_line(11, "new")` | `[-span_line(10,"old"), +span_line(11,"new")]` |

One unchanged file contributes zero deltas. One changed file contributes two
boundary deltas per query in both shapes.

### Host shape

Term skeleton:

```prolog
rule(call_edge(File, Caller, Callee),
     [ file(File, FileDigest),
       probe(sg,
             [FileDigest, QueryDigest],
             [Caller, Callee, Start, End],
             [])
     ])
```

Lowering: RX-EH1.

### Term-extract shape

Term skeleton:

```prolog
rule(call_edge(File, Caller, Callee),
     [ file(File, FileDigest),
       content(FileDigest, Content),
       extract(Content, TsQuery, match(Caller, Callee, Start, End))
     ])
```

Lowering: RX-ET1.

### Criteria table

| criterion | host relation | term-extract op |
|---|---|---|
| row mint point | world boundary, committed as EDB arrival | rule evaluation |
| identity | `(file_digest, query_digest)` | rule occurrence plus `(file_digest, query_digest)` |
| two rules ask for same digest and query | 1 host invocation | 2 op invocations |
| explicit sharing escape | inherent cache identity | author introduces one named derived rel and both consumers join it |
| changed-file computation | one fresh digest/query request | one fresh op evaluation per occurrence |
| callgraph boundary delta | 2 | 2 |
| span-line boundary delta | 2 | 2 |
| direct edge-rule input | EDB delta is an ordinary trigger | materialized level rel feeds the edge rule |
| direct op inside an edge body | unnecessary | refused by the current compile subset |
| rx lowering | RX-EH1 | RX-ET1 |
| spine residency | stdlib rel plus link bind plus content salt | decode-class kernel operation |

Verdict: sg, ast, tree-sitter, and span extraction take the host relation
shape. It satisfies `spine_residency`, gives cross-rule and cross-repo cache
sharing from the existing content salt law, and feeds edge rules through
ordinary EDB deltas. `decode/2` and `json_each/2` retain the term-extract
precedent for values already present in a rule.

## Q6. Native tree-sitter query term

### Term and compiled query

The complete term:

```prolog
ts_query([
  group(
    node(call_expression, [
      field(function, capture(callee, node(identifier, []))),
      field(arguments,
            node(arguments, [
              quant(one_or_more,
                    alternative([
                      capture(arg, named_wildcard),
                      anonymous(",")
                    ]))
            ]))
    ]),
    [ predicate(eq, capture_ref(callee), string("fetch")),
      predicate(match, capture_ref(arg), string("^[a-z]+$"))
    ]),
  quant(optional, node(comment, [])),
  quant(zero_or_more, wildcard)
])
```

Lowering: the term compiler emits query text; execution uses RX-TS1.

Exact output:

```text
((call_expression function: (identifier) @callee arguments: (arguments [(_) @arg ","]+)) (#eq? @callee "fetch") (#match? @arg "^[a-z]+$"))
(comment)?
_*
```

### Fidelity matrix

| required feature | term slot | compiled text | grade |
|---|---|---|---|
| node types | `node(Type, Children)` | `(call_expression ...)` | mapped |
| field names | `field(Name, Pattern)` | `function: ...` | mapped |
| captures | `capture(Name, Pattern)` | `Pattern @name` | mapped |
| anonymous nodes | `anonymous(Text)` | quoted node text | mapped |
| `#eq?` | `predicate(eq, Left, Right)` | `(#eq? ...)` | mapped |
| `#match?` | `predicate(match, Left, Right)` | `(#match? ...)` | mapped |
| `?` | `quant(optional, Pattern)` | `Pattern?` | mapped |
| `*` | `quant(zero_or_more, Pattern)` | `Pattern*` | mapped |
| `+` | `quant(one_or_more, Pattern)` | `Pattern+` | mapped |
| alternation | `alternative(Patterns)` | `[P1 P2 ...]` | mapped |
| wildcard | `wildcard` | `_` | mapped |
| named-node wildcard | `named_wildcard` | `(_)` | mapped |

Unknown query constructs are refused as
`unmapped_feature(slot_ts_pattern_form, Term)`. A non-query top-level term is
refused as `unmapped_feature(slot_ts_query_term, Term)`.

### Ast-grep

Ast-grep term:

```prolog
sg_pattern(
  language(rust),
  source("$RECEIVER.unwrap()"),
  captures([receiver]))
```

Lowering: RX-SG1.

It uses its own term family. `$RECEIVER` is a target-language AST
metavariable. A tree-sitter `@receiver` capture labels a subtree selected by a
query and cannot express the metavariable's pattern-hole role. Coercion into
`ts_query/1` is refused as
`unmapped_feature(slot_sg_metavariable_semantics, Term)`.

## Q7. Standing extraction ambiguities

| ambiguity | status | consequence or dependency |
|---|---|---|
| A12, `from world` as nullary `->` | resolved: distinct | `bind_decl` is a push source activated at program subscription. `probe` is demand-driven and content-cached. RX-B1 and RX-H2 have different triggers and teardown. Source syntax can replace `from world` with explicit `bind`; the arrow remains the host demand split |
| A1, glob residency | resolved: demand column | glob belongs in the host enumeration input key. It participates in the content-addressed demand row and deduplicates across rules. Program-text glob unions are absent from the host plan |
| A4, fence escape | open | `ts_query/1` removes raw fences from native tree-sitter queries. Regex, glob, and ast-grep source text still need `slot_general_embedded_text_escape`, dependent on the general embedded-text literal decision |
| A14, trailing `comment_span` | open | host extraction assigns the behavior to a stdlib bind. `slot_comment_span_trailing_bind` requires a bind implementation plus a fixture containing leading, trailing, and block comments. The current lab has no lexer implementation |

## Priced term inventory

| term | new grammar construct | data retained | failure boundary | rx lowering | selected |
|---|---:|---|---|---|---|
| `sh_decl/4` with separate input/output lists | 0 in term form, follow-up parser spelling | mode, types, template | declaration load | RX-H1 | yes |
| `sh_decl_inferred/3` | 0 | types and template | template edits can change mode | RX-H1 after plan inference | no |
| `probe/4` | 0, term form for existing `?` | inputs, outputs, salt columns | rule analysis | RX-H2 | yes |
| `bind_decl(interval, Columns)` | 1 declaration form | world source name and row shape | program link | RX-B1 | yes |
| zero-declaration rel-name bind | 0 | only relation name | runtime registry scan | RX-B0 | no |
| `query/1` | 0, term form for existing query line | complete relation atom | program analysis | RX-Q1 | yes |
| `ts_query/1` structured family | 0 host boundary constructs; term constructors are value grammar | every required query feature | query compile | RX-TS1 | yes |
| reuse `ts_query/1` for ast-grep | 0 | metavariable meaning is unmapped | named refusal slot | refused by `slot_sg_metavariable_semantics` | refused |
| `sg_pattern/3` | 0 host boundary constructs; separate value family | language, source, captures | pattern compile | RX-SG1 | yes |

## Distilled fixture/5 candidates

The lab exports five candidates in the conformance tuple shape.

| fixture | program payload | initial | schedule | intended follow-up grade |
|---|---|---|---|---|
| `ghcacher_json_normalization` | landed `prog/2` with `decode/json_each` | canonical JSON values | empty | directly promotable, already executed by the conformance engine |
| `ghcacher_host_program_term` | full `program/3` with sh, probe, bind, JSON, query | watch row | interval emission plus fetch answer | compiler AST and host wiring |
| `extraction_fork_callgraph` | callgraph sg fork | two file digests | one digest replacement | equal delta size and host salt sharing |
| `extraction_fork_span_line` | span-line fork | one file digest | one digest replacement | equal delta size and named-rel sharing requirement |
| `native_ts_query_term` | complete `ts_query/1` | empty | empty | exact compiler text and feature matrix |

The follow-up arc can convert the four model fixtures to ordinary
`prog(Decls, Rules)` after `compile/*` owns term forms for hosts, probes, binds,
queries, and query values.

## Verification

| command | receipt |
|---|---|
| `swipl -q -l labs/hosts_extraction/lab.pl -g go -g halt` from `v6/prolog` | 41 PASS, exit 0, run twice |
| `swipl -q -l v6/prolog/conformance/go.pl -g go -g halt` | 115 PASS, exit 0 |
| `v6/prolog/compile/scripts/roundtrip.sh` | `ALL GRADES PASS` |
| path fence | only `v6/prolog/labs/hosts_extraction/*` and this verdict |
| git posture | read-only git commands; dirty lab files retained for coordinator |
