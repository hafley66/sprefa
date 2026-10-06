# Findings: lowering census, IR nodes per rule by cause

Brief: `plans/v8/2026-10-05-lowering-census.brief.md`. sprefa `9c572a14f` (main plus the brief),
sqlite_ivm `80d416d`, dd engine. Counts only, no clock. No code change on main; the instrumentation
lived in scratch worktrees of both repos and was removed after the runs.

## Method

- Programs: hello = `(: Hello (* (: msg str)))` + `(Hello "hello world")`, `dl8 compile`; registry =
  `dl8 compile std/tsi_registry_emit.dl7`. Installs: #1 macro library comptime, #2 macrotime cone,
  #3 comptime (hello: prelude 115 rules, 141 after the demand rewrite; registry: 172 lowered rules).
  `DL8_IR_DUMP` sizes reproduce the brief: 1,057 / 1,069 / 2,137 (registry #3: 3,640).
- Attribution (scratch `_9a_ir_lower.rs`): `Builder::node` records, per node, the rule index, goal
  index, goal kind (positive, negative, `kernel:<Kernel>`, head, aggregate), the helper on the stack
  (`mapped_ground`, `join_constructor`, `const_text`, `text_const`, `keep_live`, `compile_kernel`,
  `served_effect`), the `lower_declared` section (prewarm, rule, strata, waves, touch, requests, ...)
  and the caller line. Wave substitution copies inherit their original's record. The dumped IR of the
  instrumented build has the same fingerprints as main's (`5e3173897ff8b91b`, `c9a6bee3b829422b`,
  `8250ccbff6f6ad18`, `912ba0e347b24ffa`).
- Rule source: a lowered rule's variables carry `reader_node(<path>, N)` of the rule form; `dl8 read`
  over the joined prelude / macrotime texts (`DL8_READ_PATH=prelude|macrotime`) maps N to file:line.
  Demand copies (`dmd__`, `adn__`) carry the original rule's variables and group with it.
- dd operators per node (scratch `ivm-engine::lower_node` + `ivm-dd`): the worker's
  `peek_identifier()` before and after each `build_node` + `observe`, in the default
  (unmeasured, join-project fused) mode. A node lowered in two scopes (hoisted and inside a LetRec)
  counts both builds. Operators no node owns (source and constructor inputs, LetRec scopes and
  variables, scope-output partition, output inspect/probe) are the remainder row.
- Rows per node: a second run with `DL8_CENSUS_TAPS=1` forces the measured mode (every node built
  unfused and tapped) and accumulates, over every settle of the process, the consolidated records per
  node. "Zero rows" = built in that run and no nonzero record in any settle.

Cause rules (first match): section prewarm -> prewarm/term map; touch -> all-sources sentinel;
strata Get of a Source / Union / Threshold -> seed source / union / threshold per relation; helper
`mapped_ground` -> constant term-map read; Join with a `mapped_ground` input -> constant join (split by
goal kind: head cross join, kernel argument cross join, aggregate cross join, body goal keyed join);
helper `join_constructor` / `const_text` -> constructor-term decode; Mint -> head Mint (all term
construction: tuple, intern, edge_ref, int/str results, aggregates' int results); helper `keep_live`
-> projection after join; the final head `Mfp` -> projection after join (head); kind aggregate ->
aggregate internals; negative goal / negated kernel Antijoin -> negation internals; other kernel
nodes -> kernel op; positive goal Join -> join between goals; positive goal Get / narrowing or
repeated-variable Mfp -> body goal read; the `Get(__ir_unit)` every `compile_rule` starts with ->
unit read per rule. No node fell outside these.

## 1. Nodes by cause per install (and dd operators they build)

Hello world, three installs, plus the registry's comptime install. dd ops: default mode; a Join fused
with its projecting Mfp is built (and counted) at the Mfp.

| cause | hello #1 nodes | #1 dd ops | hello #2 nodes | #2 dd ops | hello #3 nodes | #3 dd ops | registry #3 nodes | registry dd ops |
|---|---|---|---|---|---|---|---|---|
| body goal read | 160 | 30 | 160 | 30 | 237 | 72 | 277 | 78 |
| join between goals | 92 | 459 | 92 | 459 | 98 | 199 | 108 | 199 |
| constructor-term decode | 87 | 272 | 87 | 272 | 236 | 548 | 469 | 1127 |
| head Mint (term construction) | 46 | 172 | 46 | 172 | 160 | 376 | 541 | 1218 |
| constant filter: aggregate constant cross join | 0 | 0 | 0 | 0 | 0 | 0 | 45 | 580 |
| constant filter: body goal constant keyed join | 115 | 274 | 115 | 274 | 48 | 22 | 62 | 48 |
| constant filter: head constant cross join | 49 | 106 | 49 | 106 | 18 | 35 | 42 | 151 |
| constant filter: kernel argument cross join | 31 | 408 | 31 | 408 | 134 | 1076 | 361 | 2745 |
| constant filter: term-map read (Get+Mfp per constant) | 38 | 38 | 38 | 38 | 48 | 48 | 116 | 116 |
| kernel op | 18 | 36 | 18 | 36 | 89 | 150 | 139 | 250 |
| projection after join | 116 | 795 | 116 | 795 | 191 | 1104 | 316 | 1468 |
| projection after join (head) | 66 | 335 | 66 | 335 | 135 | 379 | 160 | 509 |
| seed source per relation key | 25 | 0 | 25 | 0 | 99 | 0 | 120 | 0 |
| union per relation | 20 | 106 | 20 | 106 | 84 | 335 | 99 | 443 |
| threshold per relation | 3 | 24 | 3 | 24 | 73 | 584 | 91 | 728 |
| negation internals | 0 | 0 | 0 | 0 | 48 | 672 | 56 | 758 |
| aggregate internals | 0 | 0 | 0 | 0 | 12 | 72 | 134 | 1608 |
| prewarm/term map | 32 | 32 | 38 | 38 | 36 | 36 | 38 | 38 |
| other: all-sources sentinel (touch) | 91 | 136 | 97 | 145 | 243 | 364 | 287 | 430 |
| other: requests | 2 | 13 | 2 | 13 | 7 | 0 | 7 | 0 |
| other: unit read per rule | 66 | 0 | 66 | 0 | 141 | 0 | 172 | 0 |
| ops outside any node (sources, constructor inputs, LetRec scopes, outputs, probes) |  | 661 |  | 685 |  | 888 |  | 1276 |
| total | 1057 | 3897 | 1069 | 3936 | 2137 | 6960 | 3640 | 13770 |

Per rule: #1 1,057 / 66 rules = 16.0, #3 2,137 / 141 lowered rules = 15.2. Nodes no rule owns:
#1 173 (sentinel 91, prewarm 32, seed sources 25, unions 20, thresholds 3, requests 2), #3 542
(sentinel 243, seed sources 99, unions 84, thresholds 73, prewarm 36, requests 7). Rule-owned
nodes: #1 884 / 66 = 13.4 per rule, #3 1,595 / 141 = 11.3. A term-map read (Get + Mfp, shared per
constant) is owned by the first rule that uses the constant.

dd operators per IR node kind (default mode). `builds` counts a node once per scope it is lowered in;
`Get Constructor` inside a decode join is never built (the join lowers to a dictionary decode).

| IR node kind | h1 nodes / builds / dd ops | h2 nodes / builds / dd ops | h3 nodes / builds / dd ops | reg nodes / builds / dd ops | dd ops per build (all installs) |
|---|---|---|---|---|---|
| Antijoin | 0 / 0 / 0 | 0 / 0 / 0 | 24 / 24 / 672 | 31 / 35 / 980 | 28.00 |
| Get Constructor | 3 / 0 / 0 | 3 / 0 / 0 | 5 / 0 / 0 | 6 / 0 / 0 | - |
| Get Derived | 148 / 155 / 0 | 148 / 155 / 0 | 240 / 243 / 0 | 313 / 319 / 0 | 0.00 |
| Get Source | 171 / 105 / 0 | 177 / 111 / 0 | 403 / 274 / 0 | 512 / 353 / 0 | 0.00 |
| Join (constructor decode) | 72 / 100 / 200 | 72 / 100 / 200 | 195 / 211 / 422 | 380 / 424 / 848 | 2.00 |
| Join (cross) | 83 / 70 / 550 | 83 / 70 / 550 | 168 / 169 / 1211 | 464 / 484 / 3576 | 7.42 |
| Join (keyed) | 204 / 107 / 697 | 204 / 107 / 697 | 137 / 15 / 121 | 177 / 37 / 323 | 6.91 |
| Mfp | 144 / 151 / 302 | 147 / 154 / 308 | 448 / 453 / 906 | 724 / 741 / 1482 | 2.00 |
| Mfp fused with its Join | 132 / 134 / 1022 | 132 / 134 / 1022 | 139 / 139 / 1101 | 174 / 174 / 1410 | 7.84 |
| Mint | 62 / 102 / 204 | 65 / 105 / 210 | 178 / 206 / 412 | 560 / 628 / 1256 | 2.00 |
| Reduce | 0 / 0 / 0 | 0 / 0 / 0 | 6 / 6 / 60 | 15 / 15 / 965 | 48.81 |
| Str | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 | 2 / 2 / 4 | 2.00 |
| Threshold | 4 / 4 / 32 | 4 / 4 / 32 | 73 / 73 / 584 | 91 / 91 / 728 | 8.00 |
| Union | 34 / 46 / 229 | 34 / 46 / 232 | 121 / 127 / 583 | 187 / 197 / 882 | 4.63 |
| Window | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 | 4 / 4 / 40 | 10.00 |

## 2. Top 20 rules by node count, hello world

Nodes per install with the rule's own nodes (section `rule`, its demand copies included). Strata
nodes (seed source, union, threshold) belong to relations and are excluded here. Breakdown keys:
read = body goal read, join = join between goals, decode = constructor-term decode, mint = term Mint,
constK / constH / constX / constA = constant keyed join / head cross join / kernel argument cross
join / aggregate cross join, constMap = term-map read for a constant first used by this rule,
kernel = kernel op, proj = projection after join, headproj = head projection, neg = negation,
agg = aggregate internals, unit = unreachable `Get(__ir_unit)`.

| # | rule (file:line) | lowered rules | #1 nodes | #2 nodes | #3 nodes | dd ops #1+#2+#3 | breakdown (one install) |
|---|---|---|---|---|---|---|---|
| 1 | `macrotime/1_caret.dl7:264` | 1 | 48 | 48 | 0 | 298 | read 10, proj 10, join 9, constK 9, decode 4, constMap 2, unit 1, constX 1, kernel 1, headproj 1 |
| 2 | `macrotime/1_caret.dl7:163` | 1 | 48 | 48 | 0 | 334 | read 12, constK 12, join 11, proj 11, unit 1, headproj 1 |
| 3 | `macrotime/0_standard.dl7:84` | 1 | 41 | 41 | 0 | 472 | decode 11, mint 10, constX 6, constMap 6, proj 3, unit 1, read 1, constK 1, kernel 1, headproj 1 |
| 4 | `prelude/2_constructor_rules.dl7:11` | 3 | 0 | 0 | 80 | 238 | proj 15, read 13, decode 12, join 11, constK 9, constX 6, kernel 6, unit 3, headproj 3, constMap 2 |
| 5 | `prelude/4_type_algebra.dl7:311` | 4 | 0 | 0 | 76 | 203 | decode 16, proj 12, read 11, constX 8, kernel 8, join 7, unit 4, constK 4, headproj 4, constMap 2 |
| 6 | `macrotime/0_standard.dl7:88` | 1 | 34 | 34 | 0 | 388 | mint 10, decode 10, constX 6, proj 3, unit 1, read 1, constK 1, kernel 1, headproj 1 |
| 7 | `macrotime/1_caret.dl7:330` | 1 | 34 | 34 | 0 | 206 | decode 8, read 4, proj 4, join 3, constK 3, kernel 3, constX 2, constMap 2, mint 2, unit 1, constH 1, headproj 1 |
| 8 | `macrotime/1_caret.dl7:144` | 1 | 33 | 33 | 0 | 206 | proj 7, read 6, constK 5, join 5, decode 4, constMap 2, unit 1, kernel 1, constH 1, headproj 1 |
| 9 | `macrotime/2_list.dl7:40` | 1 | 33 | 33 | 0 | 368 | mint 10, decode 10, constX 5, read 2, proj 2, unit 1, join 1, kernel 1, headproj 1 |
| 10 | `macrotime/1_caret.dl7:211` | 1 | 30 | 30 | 0 | 170 | read 6, proj 5, join 4, constK 4, decode 4, kernel 2, mint 2, unit 1, constX 1, headproj 1 |
| 11 | `macrotime/1_caret.dl7:114` | 1 | 30 | 30 | 0 | 304 | mint 10, decode 10, constX 4, proj 2, unit 1, read 1, kernel 1, headproj 1 |
| 12 | `macrotime/1_caret.dl7:249` | 1 | 29 | 29 | 0 | 150 | decode 8, proj 4, read 3, kernel 3, join 2, constK 2, constX 2, mint 2, unit 1, constH 1, headproj 1 |
| 13 | `prelude/2_constructor_rules.dl7:5` | 3 | 0 | 0 | 55 | 178 | proj 12, read 11, kernel 9, join 8, constK 6, unit 3, constX 3, headproj 3 |
| 14 | `prelude/3_derived_rules.dl7:264` | 1 | 0 | 0 | 52 | 156 | mint 18, decode 17, constX 9, proj 4, kernel 2, unit 1, headproj 1 |
| 15 | `prelude/3_derived_rules.dl7:164` | 1 | 0 | 0 | 52 | 152 | mint 18, decode 17, constX 9, proj 4, kernel 2, unit 1, headproj 1 |
| 16 | `prelude/3_derived_rules.dl7:150` | 1 | 0 | 0 | 46 | 130 | mint 16, decode 14, constX 8, proj 4, kernel 2, unit 1, headproj 1 |
| 17 | `prelude/3_derived_rules.dl7:244` | 1 | 0 | 0 | 46 | 130 | mint 16, decode 14, constX 8, proj 4, kernel 2, unit 1, headproj 1 |
| 18 | `macrotime/1_caret.dl7:106` | 1 | 22 | 22 | 0 | 190 | read 5, constK 5, join 4, proj 4, constMap 2, unit 1, headproj 1 |
| 19 | `macrotime/1_caret.dl7:177` | 1 | 21 | 21 | 0 | 150 | read 5, constK 5, join 3, proj 3, constMap 2, unit 1, constH 1, headproj 1 |
| 20 | `macrotime/1_caret.dl7:324` | 1 | 20 | 20 | 0 | 114 | decode 4, read 3, constK 3, proj 3, join 2, unit 1, constX 1, kernel 1, constH 1, headproj 1 |

1. `macrotime/1_caret.dl7:264`

```lisp
(<- (caret_return ?Member ?Target)
    (syntax_form ?Level)
    (: ?Level item ?Star 0)
    (syntax_atom ?Star "*")
    (: ?Level item ?Member ?Index)
    (int.lt 0 ?Index)
    (syntax_form ?Member)
    (: ?Member item ?Colon 0)
    (syntax_atom ?Colon ":")
    (: ?Member item ?Mark 1)
    (caret_mark ?Mark ?Target)
    (syntax_atom ?Target ?TargetText))
```

2. `macrotime/1_caret.dl7:163`

```lisp
(<- (caret_reparent ?Form ?Edge ?Name ?Body)
    (syntax_form ?Form)
    (: ?Form item ?Colon 0)
    (syntax_atom ?Colon ":")
    (: ?Form item ?Edge 1)
    (syntax_form ?Edge)
    (: ?Edge item ?EdgeColon 0)
    (syntax_atom ?EdgeColon ":")
    (: ?Edge item ?Mark 1)
    (caret_mark ?Mark ?Name)
    (syntax_atom ?Name ?NameText)
    (: ?Form item ?Body 2)
    (syntax_form ?Body))
```

3. `macrotime/0_standard.dl7:84`

```lisp
(<- (plus_form ?Form ?Output)
    (syntax_claim ?Form "<+")
    (GeneratedSyntax ?Form 0 0 ?Output))
```

4. `prelude/2_constructor_rules.dl7:11`

```lisp
(<- (HistoryV1 ?Source ?Options ?Result)
    (intern_snapshot HistoryV1 ?Arguments ?Result)
    (tuple ?Source ?Options ?Arguments)
    (default ?Options ?Mode "copy")
    (edge_snapshot ?Options ?ContractName ?Contract 1)
    (Extends ?Source ?Contract ?Source))
```

5. `prelude/4_type_algebra.dl7:311`

```lisp
(<- (Intersect ?Left ?Right ?Result)
    (intern_snapshot Intersect ?Arguments ?Result)
    (tuple ?Left ?Right ?Arguments)
    (valid_intersection ?Left ?Right))
```

6. `macrotime/0_standard.dl7:88`

```lisp
(<- (plus_operator ?Form ?Output)
    (syntax_claim ?Form "<+")
    (GeneratedSyntax ?Form 0 1 ?Output))
```

7. `macrotime/1_caret.dl7:330`

```lisp
(<- (: ?Output item ?Child ?TemplatePath)
    (caret_return ?Member ?Target)
    (caret_mint ?Member 1 0 ?Output)
    (caret_template_path ?Path)
    (int.lt 0 ?Path)
    (caret_mint ?Member 1 ?Path ?Child)
    (int.add ?Path -1 ?TemplatePath))
```

8. `macrotime/1_caret.dl7:144`

```lisp
(<- (syntax_diagnostic ?Level "ambiguous_return")
    (: ?Level item ?First ?FirstIndex)
    (caret_level_mark ?First ?FirstMark)
    (: ?Level item ?Second ?SecondIndex)
    (caret_level_mark ?Second ?SecondMark)
    (int.lt ?FirstIndex ?SecondIndex)
    (: ?FirstMark item ?Caret 0)
    (syntax_atom ?Caret "^"))
```

9. `macrotime/2_list.dl7:40`

```lisp
(<- (list_mint ?Form ?TemplatePath ?Output)
    (list_form ?Form)
    (list_template_path ?TemplatePath)
    (GeneratedSyntax ?Form 0 ?TemplatePath ?Output))
```

10. `macrotime/1_caret.dl7:211`

```lisp
(<- (caret_reparent_member ?Form ?Member ?OutputOrdinal)
    (caret_reparent ?Form ?Edge ?Name ?Body)
    (: ?Body item ?Member ?Index)
    (syntax_form ?Member)
    (: ?Member item ?MemberColon 0)
    (syntax_atom ?MemberColon ":")
    (int.add ?Index 1 ?OutputOrdinal))
```

11. `macrotime/1_caret.dl7:114`

```lisp
(<- (caret_mint ?Invocation ?OutputOrdinal ?TemplatePath ?Output)
    (caret_key ?Invocation ?OutputOrdinal ?TemplatePath)
    (GeneratedSyntax ?Invocation ?OutputOrdinal ?TemplatePath ?Output))
```

12. `macrotime/1_caret.dl7:249`

```lisp
(<- (: ?Output item ?Child ?Shifted)
    (caret_reparent_member ?Form ?Member ?OutputOrdinal)
    (caret_mint ?Form ?OutputOrdinal 0 ?Output)
    (: ?Member item ?Child ?Index)
    (int.lt 0 ?Index)
    (int.add ?Index 1 ?Shifted))
```

13. `prelude/2_constructor_rules.dl7:5`

```lisp
(<- (HistoryV1 ?Source ?Options ?Result)
    (default ?Options ?Mode "copy")
    (edge_snapshot ?Options ?ContractName ?Contract 1)
    (intern HistoryV1 (tuple ?Source ?Options) ?Result)
    (Extends ?Source ?Contract ?Source))
```

14. `prelude/3_derived_rules.dl7:264`

```lisp
(<- (: ?BodyCall ?Name ?Variable ?SourceIndex)
    (curry_edge ?Source ?Result ?Name ?Target
                ?SourceIndex ?ResultIndex)
    (BodyCall ?Result 0 ?BodyCall)
    (Variable ?Result ?ResultIndex ?Name ?Variable))
```

15. `prelude/3_derived_rules.dl7:164`

```lisp
(<- (: ?BodyCall ?Name ?Variable ?Position)
    (history_v1_edge ?Source ?Options ?Result
                     ?Name ?Target ?Position)
    (BodyCall ?Result 0 ?BodyCall)
    (Variable ?Result ?Position ?Name ?Variable))
```

16. `prelude/3_derived_rules.dl7:150`

```lisp
(<- (: ?HeadCall ?Name ?Variable ?Position)
    (history_v1_edge ?Source ?Options ?Result
                     ?Name ?Target ?Position)
    (HeadCall ?Result ?HeadCall)
    (Variable ?Result ?Position ?Name ?Variable))
```

17. `prelude/3_derived_rules.dl7:244`

```lisp
(<- (: ?HeadCall ?Name ?Variable ?ResultIndex)
    (curry_edge ?Source ?Result ?Name ?Target
                ?SourceIndex ?ResultIndex)
    (HeadCall ?Result ?HeadCall)
    (Variable ?Result ?ResultIndex ?Name ?Variable))
```

18. `macrotime/1_caret.dl7:106`

```lisp
(<- (caret_level_mark ?Edge ?Mark)
    (syntax_form ?Edge)
    (: ?Edge item ?Colon 0)
    (syntax_atom ?Colon ":")
    (: ?Edge item ?Mark 1)
    (caret_mark ?Mark ?Target))
```

19. `macrotime/1_caret.dl7:177`

```lisp
(<- (syntax_claim ?Form "^")
    (caret_reparent ?Form ?Edge ?Name ?Body)
    (: ?Edge item ?Mark 1)
    (: ?Mark item ?Caret 0)
    (syntax_atom ?Caret "^"))
```

20. `macrotime/1_caret.dl7:324`

```lisp
(<- (: ?Output item ?Child ?Index)
    (caret_return ?Member ?Target)
    (caret_mint ?Member 0 0 ?Output)
    (: ?Member item ?Child ?Index)
    (int.lt 1 ?Index))
```


### Registry comptime install, top 10

`unlocated:ref(kernel(':'))` = 6 lowered rules whose variables carry no reader node (macro-generated
`:` rules).

| # | rule (file:line) | lowered rules | nodes | dd ops | breakdown |
|---|---|---|---|---|---|
| 1 | `std/tsi_registry_emit.dl7:45` | 1 | 167 | 536 | mint 57, constX 38, decode 32, proj 18, constMap 8, kernel 8, read 2, unit 1, join 1, constH 1, headproj 1 |
| 2 | `std/tsi_registry_emit.dl7:146` | 1 | 143 | 466 | mint 46, constX 30, decode 24, proj 16, constMap 10, kernel 6, read 5, join 3, unit 1, constH 1, headproj 1 |
| 3 | `std/tsi_registry_emit.dl7:35` | 1 | 130 | 419 | mint 43, constX 29, decode 24, proj 14, constMap 8, kernel 6, read 2, unit 1, join 1, constH 1, headproj 1 |
| 4 | `std/tsi_registry_emit.dl7:27` | 1 | 91 | 288 | mint 29, constX 20, decode 16, proj 10, constMap 6, kernel 4, read 2, unit 1, join 1, constH 1, headproj 1 |
| 5 | `std/tsi_registry_emit.dl7:83` | 1 | 81 | 258 | mint 26, constX 18, decode 16, proj 10, kernel 4, read 2, constMap 2, unit 1, join 1, headproj 1 |
| 6 | `std/tsi_registry_emit.dl7:308` | 1 | 80 | 584 | agg 30, decode 16, constA 12, constMap 10, mint 10, unit 1, read 1 |
| 7 | `std/tsi_registry_emit.dl7:106` | 1 | 80 | 257 | mint 24, decode 20, constX 16, proj 9, kernel 5, constMap 4, unit 1, headproj 1 |
| 8 | `prelude/2_constructor_rules.dl7:11` | 3 | 80 | 238 | proj 15, read 13, decode 12, join 11, constK 9, constX 6, kernel 6, unit 3, headproj 3, constMap 2 |
| 9 | `prelude/4_type_algebra.dl7:311` | 4 | 76 | 203 | decode 16, proj 12, read 11, constX 8, kernel 8, join 7, unit 4, constK 4, headproj 4, constMap 2 |
| 10 | `unlocated:ref(kernel(':'))` | 6 | 74 | 258 | constMap 20, constH 18, constK 12, unit 6, read 6, proj 6, headproj 6 |

1. `std/tsi_registry_emit.dl7:45`

```lisp
(<- (piece "of" ?Index
       (doc.Doc.concat
          [(doc.Doc.text "            Arg::")
           (doc.Doc.text ?Variant)
           (doc.Doc.text "(")
           (doc.Doc.text ?Pattern)
           (doc.Doc.text ") => ArgKind::")
           (doc.Doc.text ?Variant)
           (doc.Doc.text ",")]))
    (tsi.arg_kind ?Index ?Word ?Pattern)
    (variant_of ?Word ?Variant))
```

2. `std/tsi_registry_emit.dl7:146`

```lisp
(<- (piece "entries" ?Index
       (doc.Doc.concat
          [?Notes
           (doc.Doc.text "\n    Relation {\n        name: \"")
           (doc.Doc.text ?Name)
           (doc.Doc.text "\",\n        args: &[")
           ?Args
           (doc.Doc.text "],")
           ?Suffix
           (doc.Doc.text "\n    },")]))
    (tsi.registry ?Index ?Name ?Role)
    (notes_document ?Name ?Notes)
    (args_document ?Name ?Args)
    (role_suffix ?Name ?Suffix))
```

3. `std/tsi_registry_emit.dl7:35`

```lisp
(<- (piece "word" ?Index
       (doc.Doc.concat
          [(doc.Doc.text "            ArgKind::")
           (doc.Doc.text ?Variant)
           (doc.Doc.text " => \"")
           (doc.Doc.text ?Word)
           (doc.Doc.text "\",")]))
    (tsi.arg_kind ?Index ?Word ?Pattern)
    (variant_of ?Word ?Variant))
```

4. `std/tsi_registry_emit.dl7:27`

```lisp
(<- (piece "variant" ?Index
       (doc.Doc.concat
          [(doc.Doc.text "    ")
           (doc.Doc.text ?Variant)
           (doc.Doc.text ",")]))
    (tsi.arg_kind ?Index ?Word ?Pattern)
    (variant_of ?Word ?Variant))
```

5. `std/tsi_registry_emit.dl7:83`

```lisp
(<- (args_item ?Name
       (tuple ?Position
          (doc.Doc.concat
             [(doc.Doc.text ", ")
              (doc.Doc.text ?Variant)])))
    (tsi.registry_arg ?Name ?Position ?Kind)
    (int.ne ?Position 0)
    (variant_of ?Kind ?Variant))
```

6. `std/tsi_registry_emit.dl7:308`

```lisp
(<- (fs.file "target/tsi/registry.rs" (fold doc.join "" ?Item))
    (file_item ?Item))
```

7. `std/tsi_registry_emit.dl7:106`

```lisp
(<- (role_suffix ?Name
       (doc.Doc.concat
          [(doc.Doc.text " // ")
           (doc.Doc.text ?Role)]))
    (tsi.registry ?Index ?Name ?Role)
    (str.len ?Role ?Length)
    (int.gt ?Length 0))
```

8. `prelude/2_constructor_rules.dl7:11`

```lisp
(<- (HistoryV1 ?Source ?Options ?Result)
    (intern_snapshot HistoryV1 ?Arguments ?Result)
    (tuple ?Source ?Options ?Arguments)
    (default ?Options ?Mode "copy")
    (edge_snapshot ?Options ?ContractName ?Contract 1)
    (Extends ?Source ?Contract ?Source))
```

9. `prelude/4_type_algebra.dl7:311`

```lisp
(<- (Intersect ?Left ?Right ?Result)
    (intern_snapshot Intersect ?Arguments ?Result)
    (tuple ?Left ?Right ?Arguments)
    (valid_intersection ?Left ?Right))
```

10. `unlocated:ref(kernel(':'))`

```lisp
None
```


## 3. Nodes that compute nothing for hello world

"Unreachable": no stratum body reaches the node; dd never builds it. "Built, zero rows": built in the
tapped run, no nonzero record in any settle of the process. #1 + #2 + #3 = 4,263 nodes; 3,422
built-and-empty, 261 unreachable, 11 constructor Gets folded into decodes, 569 with rows.

| cause | h1 unreachable | h1 built, zero rows | h2 unreachable | h2 built, zero rows | h3 unreachable | h3 built, zero rows | reg unreachable | reg built, zero rows |
|---|---|---|---|---|---|---|---|---|
| body goal read | 0 | 124 | 0 | 91 | 0 | 149 | 0 | 145 |
| join between goals | 0 | 92 | 0 | 71 | 0 | 93 | 0 | 91 |
| constructor-term decode | 0 | 84 | 0 | 80 | 0 | 231 | 0 | 278 |
| head Mint (term construction) | 0 | 46 | 0 | 46 | 0 | 160 | 0 | 160 |
| constant filter: aggregate constant cross join | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| constant filter: body goal constant keyed join | 0 | 114 | 0 | 87 | 0 | 44 | 0 | 44 |
| constant filter: head constant cross join | 0 | 49 | 0 | 49 | 0 | 18 | 0 | 18 |
| constant filter: kernel argument cross join | 0 | 31 | 0 | 30 | 0 | 134 | 0 | 134 |
| constant filter: term-map read (Get+Mfp per constant) | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| kernel op | 0 | 18 | 0 | 17 | 0 | 89 | 0 | 85 |
| projection after join | 0 | 115 | 0 | 97 | 0 | 186 | 0 | 184 |
| projection after join (head) | 0 | 66 | 0 | 66 | 0 | 123 | 0 | 121 |
| seed source per relation key | 0 | 18 | 0 | 16 | 0 | 87 | 0 | 101 |
| union per relation | 0 | 18 | 0 | 15 | 0 | 74 | 0 | 66 |
| threshold per relation | 0 | 2 | 0 | 2 | 0 | 63 | 0 | 57 |
| negation internals | 0 | 0 | 0 | 0 | 0 | 44 | 1 | 45 |
| aggregate internals | 0 | 0 | 0 | 0 | 0 | 12 | 0 | 12 |
| prewarm/term map | 0 | 32 | 0 | 38 | 0 | 36 | 0 | 38 |
| other: all-sources sentinel (touch) | 0 | 70 | 0 | 72 | 0 | 212 | 0 | 242 |
| other: requests | 0 | 2 | 0 | 2 | 0 | 7 | 0 | 0 |
| other: unit read per rule | 66 | 0 | 66 | 0 | 129 | 0 | 159 | 0 |
| total | 66 | 881 | 66 | 779 | 129 | 1762 | 160 | 1821 |
| nodes with rows |  | 107 |  | 221 |  | 241 |  | 1653 |

## 4. Ranked lowering changes

Counts per install: nodes removed / dd operators of the removed nodes (default mode). `(+n)`: for
constants, the Mfp nodes the change adds where a constant chain has no adjacent single-consumer Mfp to
absorb its literal; for unseeded sources, the sentinel nodes of those sources (also counted in the
sentinel row). Rows overlap (last line: union of node ids, net of the added Mfps). Ranked by net nodes
removed over the three hello installs.

| change | h1 nodes / dd ops | h2 nodes / dd ops | h3 nodes / dd ops | reg nodes / dd ops |
|---|---|---|---|---|
| unreachable nodes (Get __ir_unit per rule) | 66 / 0 | 66 / 0 | 129 / 0 | 160 / 0 |
| all-sources sentinel | 91 / 136 | 97 / 145 | 243 / 364 | 287 / 430 |
| constants as IR literals | 233 / 826 (+11) | 233 / 826 (+11) | 248 / 1181 (+50) | 626 / 3640 (+178) |
| no seed source for keys with no seed rows | 28 / 30 (+36) | 26 / 30 (+32) | 139 / 156 (+174) | 157 / 168 (+202) |
| threshold only where duplicates can arise | 0 / 0 | 0 / 0 | 14 / 112 | 14 / 112 |
| fused project-after-join in the IR | 132 / 0 | 132 / 0 | 139 / 0 | 174 / 0 |
| hash-cons structurally equal nodes (shared decodes, reads, maps) | 353 / 437 | 356 / 437 | 606 / 665 | 767 / 775 |
| semantic decode as one join on __ir_semantic | 24 / 120 | 24 / 120 | 72 / 210 | 166 / 465 |
| prewarm through Engine::intern_terms | 32 / 32 | 38 / 38 | 36 / 36 | 38 / 38 |
| union of all rows above | 768 (net 757) / 1444 of 1057 / 3897 | 778 (net 767) / 1459 of 1069 / 3936 | 1300 (net 1250) / 2416 of 2137 / 6960 | 1961 (net 1783) / 5305 of 3640 / 13770 |

### Ranked list

| rank | change | net nodes removed h#1 / h#2 / h#3 / registry #3 | dd ops of removed nodes h#1 / h#2 / h#3 / reg | safety condition |
|---|---|---|---|---|
| 1 | Hash-cons the builder: one node per structurally equal (op, canonical inputs). Largest share: a fresh `Get` per goal of the same derived relation (128 in #1), the sentinel's second `Get` of every source, duplicate unit Gets and constant joins | 353 / 356 / 606 / 767 (162 / 162 / 280 / 339 outside rows 2-5) | 437 / 437 / 665 / 775 | Equal op with equal inputs denotes the same collection for every IR op (Mint is a function of functor and args; a `Get` of a LetRec id has one binder). `substitute` (waves) and the narrowing pass keep their maps keyed by node id, so a shared node must stay valid as a memo key in both |
| 2 | Ground terms as IR literals: an IR term table (as `texts`), resolved by the engine at install; `Mfp` filter / map with the literal cell replaces the term-map read and its join | 222 / 222 / 198 / 448 (233 / 233 / 248 / 626 removed, 11 / 11 / 50 / 178 Mfp added where no adjacent single-consumer Mfp absorbs the literal) | 826 / 826 / 1,181 / 3,640 (cross joins: 7.4 ops per build) | A ground term's cell is fixed before the first settle and stable for the install; `__ir_term_map` holds one row per `TermId` (it is filtered on column 0 = TermId today, so the join is a lookup). Terms that arrive after install keep the term-map path |
| 3 | Drop the all-sources sentinel (`touch`: Get + Mfp per Source, one Union) from the dd lowering | 91 / 97 / 243 / 287 | 136 / 145 / 364 / 430 | The lowering's comment names one reader, the SQLite host adapter ("resolves source columns through its first output plan"); the dd engine builds it as an ordinary output. Condition: the SQLite adapter takes source columns from `Program.rels`, or the sentinel is added by the SQLite backend at install; `ivm_ir::compose` stops assuming output 0 reads every source |
| 4 | Fused project-after-join in the IR (`Join` with a `project`) | 132 / 132 / 139 / 174 | 0 (dd already fuses these in unmeasured mode) | The Mfp has no filter and no map and its Join has one consumer. Measured-mode taps then see one node where they saw two |
| 5 | No `Get(__ir_unit)` unless the rule's first goal needs it | 66 / 66 / 129 / 160 (all unreachable today) | 0 | The first goal is a positive relation read (it replaces `current`). Kernel-first or negative-first rules keep the unit read (0 / 0 / 12 / 12 reachable today) |
| 6 | No seed source for relation keys that never receive rows; the relation's Union disappears when one rule body remains | 28 / 26 / 139 / 157, plus their sentinel nodes 36 / 32 / 174 / 202 if rank 3 is not applied, plus one source input collection each in dd | 30 / 30 / 156 / 168 | The key is in none of `program.seeds`, `Declared::sources`, served `.answer` relations, and no comptime round writes it through `IrSession::apply` (edge and intern snapshots, answers). The counts here are empirical: zero rows over every settle of these runs. A static rule needs the session's write set per key |
| 7 | Semantic decode as one join on `__ir_semantic` instead of a decode per wrapper (ref, const) plus a Union | 24 / 24 / 72 / 166 | 120 / 120 / 210 / 465 removed; the replacement is an arranged keyed join (6.9 ops per build on average) where two dictionary decodes cost 2 ops each, so the dd operator change per decode is not measured here | `__ir_semantic` (Union of the `9:1:ref` and `9:1:const` constructors) is a stratum before every reader, as `semantic_stratum` places it today; ref and const ids are disjoint |
| 8 | Prewarm through `Engine::intern_terms` instead of a Get + Mint per constructor shape | 32 / 38 / 36 / 38 | 32 / 38 / 36 / 38 | The session takes term ids from `intern_terms`' return value instead of the `__ir_prewarm_out_*` outputs; shapes named after install go through `declare_constructors` |
| 9 | Threshold only where duplicates can arise | 0 / 0 / 14 / 14 | 0 / 0 / 112 / 112 | Applied criterion: after rank 6 the relation has one rule body, the head holds only variables, and every variable of the rule's positive goals is in the head (no projection drops a variable), so each output row has one derivation. Inputs are sets and body kernels are functional |
| - | all rows, union of node ids | 757 / 767 / 1,250 / 1,783 of 1,057 / 1,069 / 2,137 / 3,640 | 1,444 / 1,459 / 2,416 / 5,305 of 3,897 / 3,936 / 6,960 / 13,770 | |

Notes on the counts:

- dd operators of a removed node are those the default-mode build allocated for it. A Join fused into
  its Mfp has 0 there and its operators sit on the Mfp, so rows 2 and 6 undercount dd operators where
  the removed Join was fused.
- 3,422 of the 4,263 hello-world nodes are built and carry no row in any settle (section 3); the
  three installs together carry rows on 569 nodes (#1 107, #2 221, #3 241).
- Remainder operators (no node owns them): 661 / 685 / 888 / 1,276. Rank 6 also removes one source
  input collection per dropped Source relation from that remainder.

## Instrumentation (removed)

Scratch worktrees `lowering-census-scratch` (sprefa, detached at `9c572a14f`) and
`sqlite_ivm-census-scratch` (sqlite_ivm, detached at `80d416d`):

- `src/_6_eval/_9a_ir_lower.rs`: `#[track_caller]` `Builder::node/get/mfp/get_constructor`
  recording a per-node attribution record; wrappers around the seven helpers setting the helper tag;
  goal kind and section assignments; `DL8_ATTR_DUMP=<dir>` writes `{ir, attrs, rules, hidden}` per
  lowering.
- `ivm-engine::lower_node`: `Rel::op_mark` before/after each node build into a thread-local list;
  `ivm-dd`: `op_mark = scope.worker().peek_identifier()`, `DL8_DD_DUMP=<dir>` writes per-install
  node operators and (with `DL8_CENSUS_TAPS=1`, which forces measured mode) per-node tap record
  counts after every settle.
