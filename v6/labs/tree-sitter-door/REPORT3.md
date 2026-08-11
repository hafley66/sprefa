# Tree-sitter emitter probe, round 2

## Result

The three requested techniques ran. Registry-driven precedence, the four-row
character-class bridge, and concrete separator specialization all produced
Tree-sitter grammar fragments. The resulting `emitted-grammar.js` passes
`pnpm exec tree-sitter generate emitted-grammar.js`.

The checked-in `grammar.js` has 43 rule entries. The brief asks for 44. The
table below covers every rule found by `^    [a-z_]+: \$ =>` in the file.

## Inputs

`emit_grammar.pl` reads these files:

1. `v6/prolog/compile/parse_dl_dcg.pl`, the DCG and its call sites.
2. `v6/prolog/compile/registry.pl`, loaded as a Prolog module and queried with
   the same `surface/5` and `expression/5` goal shapes used by the parser.
3. `v6/labs/tree-sitter-door/grammar.js`, the remaining hand overlay. Generated
   fragments replace checked locations in this file. A missing location makes
   the emitter exit 3.

Command and receipt:

```text
$ swipl -q -f emit_grammar.pl -- ../../prolog/compile/parse_dl_dcg.pl emitted-grammar.js
DCG_EMIT_V2 specializations=14 unbound_args=2 output=.../emitted-grammar.js
```

The emitted header records the absolute paths, registry inventory,
specializations, and unbound forms used for that run.

## Technique results

| Technique | Verdict | Receipt | Hand overlay removed |
|---|---|---|---:|
| Registry precedence | WORKED | `surface/5` returns bind and comparison operators. `expression/5` returns arithmetic levels `1-[+,-]` and `2-[mod,*,/]`. The emitter derives Tree-sitter levels 3 and 4 from those levels, followed by unary 5 and call 6, and emits the operator choices used by the three infix rules. | 508 chars |
| Character predicates | WORKED | The DCG inventory contains only `space`, `alnum`, `alpha`, and `digit`. One four-row mapping emits the integer, float, variable, and identifier regexes. | 169 chars |
| Parameterized nonterminals | WORKED | Current source yields 14 rules: `args_*` and `sep_*` for `atom_arg`, `decl_a_column`, `enum_field`, `expr`, both `typed_col(...)` shapes, `int_lit`, and `rel_atom_term` where applicable. The generic definitions contribute one unbound `args` form and one unbound `sep` form after variant normalization. | 37 chars |

Direct source search finds 12 textual `sep(...)` or `args(...)` occurrences,
including definitions and recursive calls. This differs from the brief's
24-call-site receipt. The emitter walks parsed terms rather than relying on the
stated count.

## Classification

`EMITTED-IDENTICAL` means the generated fragment has the same Tree-sitter
shape as the hand rule. `EMITTED-NEEDS-OVERLAY` means terminals or a repeated
shape are generated while named nodes, fields, or alternatives remain hand
input. `HAND-ONLY` means neither the DCG nor registry determines the rule.

| Hand rule | Classification | Reason |
|---|---|---|
| `source_file` | HAND-ONLY | Root repetition and recovery boundary. |
| `statement` | EMITTED-NEEDS-OVERLAY | Dispatch exists; the retained editor alternatives are hand input. |
| `relation_declaration` | EMITTED-NEEDS-OVERLAY | Tokens and specialized arguments exist; fields and declaration alternatives remain. |
| `shell_declaration` | EMITTED-NEEDS-OVERLAY | Tokens and argument shapes exist; input/output CST fields remain. |
| `bind_declaration` | EMITTED-NEEDS-OVERLAY | Tokens and argument shape exist; declaration node shape remains. |
| `declaration_parameter` | HAND-ONLY | The editor grammar admits typed and untyped generated columns. |
| `column` | EMITTED-NEEDS-OVERLAY | `typed_col(...)` is specialized; field names remain. |
| `type` | EMITTED-NEEDS-OVERLAY | Character classes help its name; wrapper recursion and nullability remain. |
| `enum_variants` | EMITTED-NEEDS-OVERLAY | Variant recursion exists; the semicolon repetition boundary remains. |
| `enum_variant` | EMITTED-NEEDS-OVERLAY | `args_enum_field` is generated; the editor node mapping remains. |
| `relation_modifier` | EMITTED-NEEDS-OVERLAY | Fixed words exist; the three CST alternatives remain. |
| `rule` | EMITTED-NEEDS-OVERLAY | Arrow tokens exist; field boundaries remain. |
| `fact` | HAND-ONLY | The DCG converts a fact into a rule term. |
| `query` | EMITTED-NEEDS-OVERLAY | Query tokens exist; its retained CST boundary remains. |
| `match_statement` | EMITTED-NEEDS-OVERLAY | Block tokens exist; arm repetition and fields remain. |
| `match_arm` | EMITTED-NEEDS-OVERLAY | Arrow tokens exist; guard and head fields remain. |
| `goal_list` | EMITTED-IDENTICAL | `sep_expr` specializes to expression plus repeated comma-expression. |
| `expression` | EMITTED-NEEDS-OVERLAY | Tiers are generated; the complete editor alternative set remains. |
| `binding_expression` | EMITTED-IDENTICAL | Bind operator set and right association are emitted from registry data plus the fixed Tree-sitter association mapping. |
| `comparison_expression` | EMITTED-IDENTICAL | Guard operator set and left association are emitted. |
| `binary_expression` | EMITTED-IDENTICAL | Both arithmetic levels, literal sets, order, and left association are emitted. |
| `unary_expression` | EMITTED-NEEDS-OVERLAY | Its numeric level follows generated tiers; unary syntax remains hand input. |
| `member_expression` | EMITTED-NEEDS-OVERLAY | Dot-chain structure exists; editor precedence and repetition remain. |
| `member_access` | HAND-ONLY | Immediate-token behavior has no DCG representation. |
| `parenthesized_expression` | EMITTED-NEEDS-OVERLAY | Delimiters and expression call exist; the named boundary remains. |
| `atom` | EMITTED-NEEDS-OVERLAY | Specialized arguments exist; fields, named arguments, and call precedence remain. |
| `named_argument` | EMITTED-NEEDS-OVERLAY | The DCG semantic wrapper does not specify fields. |
| `object_pattern` | EMITTED-NEEDS-OVERLAY | Brace parsing exists; editor pair repetition remains. |
| `object_pair` | EMITTED-NEEDS-OVERLAY | Pair parsing exists; key alternatives and fields remain. |
| `capture_key` | HAND-ONLY | Dollar capture capitalization is an editor token choice. |
| `list` | EMITTED-NEEDS-OVERLAY | `sep_expr` is generated; spread and empty-list alternatives remain. |
| `spread_element` | HAND-ONLY | The DCG constructs a semantic term without this CST boundary. |
| `path` | EMITTED-NEEDS-OVERLAY | Recursive dotted parsing exists; the reusable separator declaration remains hand input. |
| `literal` | EMITTED-NEEDS-OVERLAY | Literal parsers exist; named editor alternatives remain. |
| `integer` | EMITTED-IDENTICAL | Generated from the `digit` row. |
| `float` | EMITTED-IDENTICAL | Generated from `digit` plus the finite decimal/exponent DCG shape. |
| `string` | HAND-ONLY | Escape-preserving regex token shape is absent from the semantic decoder. |
| `quoted_atom` | HAND-ONLY | Escape-preserving regex token shape is absent from the semantic decoder. |
| `template` | EMITTED-NEEDS-OVERLAY | Fixed escape clauses exist; the arbitrary-character token remains. |
| `boolean` | EMITTED-NEEDS-OVERLAY | Fixed values are visible through a semantic membership action. |
| `variable` | EMITTED-IDENTICAL | Generated from `alpha` and `alnum` with the DCG capitalization branches. |
| `identifier` | EMITTED-IDENTICAL | Generated from `alpha` and `alnum` with the underscore branches. |
| `comment` | HAND-ONLY | Whitespace parsing discards comments; the editor grammar retains them. |

Totals: 8 `EMITTED-IDENTICAL`, 27 `EMITTED-NEEDS-OVERLAY`, and 8
`HAND-ONLY`.

## Overlay measurement

Whitespace was removed before counting. Rule spans begin at a four-space rule
key and end at the next rule key. This is the same character unit used in
round 1.

```text
identical hand-rule spans                 714
generated specialized helper rules       934
emitted total                            1648
remaining hand-rule overlay              2767
ratio                         2767 / 1648 = 1.6790
```

Round 1's checked-in report records 1,407 emitted chars, 3,862 overlay chars,
and 2.74. The lane brief records an older 2.96 ratio. The round-2 ratio beats
both baselines and remains above 1.0.

Technique attribution within the 714 removed hand-rule chars is 508
precedence, 169 character predicates, and 37 separator specialization. The
934 specialized helper chars are emitted code and occur once per distinct
binding and combinator.

## Gates

```text
$ pnpm exec tree-sitter generate emitted-grammar.js
rc=0
```

`cd v6 && just text-door` completed with:

```text
TEXT_DOOR compiled=272 byte_identical=272 failures=0
```

The hand grammar was regenerated after the emitted-grammar check so its
checked-in `src/grammar.json` and `src/parser.c` remain the Phase A artifacts.
The complete gate passes:

```text
$ ./run-tests.sh
DCG_EMIT_V2 specializations=14 unbound_args=2 output=/tmp/...
PASS parse: golden-flex.dl6 lines=630 errors=0
TS_CORPUS total=272 clean=272 errors=0
PASS format: formatting law and idempotence
$ echo $?
0
```

VERDICT: NO. DCG plus registry derives operator inventories, arithmetic tier
order, the four observed character classes, and bounded higher-order
specializations. It does not derive editor CST node boundaries and field
names, immediate token behavior, comment retention, escape-preserving lexical
tokens, recovery/root choices, or editor-only alternatives. Those remaining
rules account for 2,767 non-whitespace characters.
