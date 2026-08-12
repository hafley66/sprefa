# Tree-sitter, Topiary, and the one-description question

## TOC
- Verdicts
- Phase A: the complete grammar
- Phase B: emitting grammar.js from the DCG (fails, measured)
- Phase C: LSP candidates and the recommended shot
- Forks awaiting the user
- Where the code lives

## Verdicts

| Question | Verdict | Receipt |
|---|---|---|
| Can tree-sitter parse all of dl6? | YES | `golden-flex.dl6 lines=630 errors=0`; `TS_CORPUS total=266 clean=266 errors=0` |
| Can Topiary format dl6 to the decree? | YES | formatting law + idempotence PASS |
| Can the DCG be the ONE hand-written description? | **NO**, settled in round 2 | All three untried techniques WORKED and moved the ratio 2.96 -> 1.68 (PR #178). What remains is editor-only and was never in a compiler's parser: CST node boundaries and field names, immediate tokens, comment retention, escape-preserving tokens, recovery/root choices = 2767 chars |

Landed in PR #177 (main `0cc79ca1`). Lab gate: `v6/labs/tree-sitter-door/run-tests.sh`, rc=0, coordinator-rerun.

## Phase A: the complete grammar

The first lab (PR-less, branch since deleted, commit df6b0c8a) proved a
62-line slice. This one covers the whole surface. Precedence resolved by
explicit declaration, six levels:

| Level | Declaration | Construct |
|---:|---|---|
| 1 | `prec.right` | `:=` and `is` binding |
| 2 | `prec.left` | comparison operators |
| 3 | `prec.left` | additive `+` `-` |
| 4 | `prec.left` | multiplicative `*` `/` `mod` |
| 5 | `prec` | unary `+` `-` |
| 6 | `prec` / `prec.left` | calls and member access |

GLR conflict sets declared for fact-vs-expression, atom-vs-path, and
named-argument-vs-object-pair. Tree-sitter 0.26.9 calls them unnecessary
for the current corpus; they document shared prefixes for incomplete
editor input.

## Phase B: emitting grammar.js from the DCG (fails, measured)

`v6/labs/tree-sitter-door/emit_grammar.pl` reads `parse_dl_dcg.pl` with a
`read_term/3` loop and lowers what it mechanically can.

Measured twice, against two parser revisions. The lab branch forked from
PR #169, so its report cites the pre-opus parser; main's parser is smaller
and slightly LESS emittable, because the moves that shrank it
(parameterized nonterminals, table dispatch, sigil operators) are exactly
the shapes the generator cannot read.

| parser | non-ws chars | clauses | translatable | emitted | overlay ratio |
|---|---:|---:|---:|---:|---:|
| PR #169 (lab report) | 29534 | 108 | 34 | 1407 | 2.74 |
| main, PRs #172+#173 | 26473 | 103 | 32 | 1304 | 2.96 |

```
$ swipl -q -f emit_grammar.pl -- ../../prolog/compile/parse_dl_dcg.pl out.js
DCG_EMIT clauses=103 translatable=32 rule_names=70 output=out.js
```

| DCG shape | Emits as |
|---|---|
| `(A, B)` | `seq(A, B)` |
| `(A ; B)` | `choice(A, B)` |
| fixed-integer code list | literal token |
| call to a discovered DCG head | `$.rule_name` |
| `[]` | `blank()` |
| `{action}`, cut | `blank()` + overlay requirement |

Of 44 hand rules: 0 EMITTED-IDENTICAL, 26 EMITTED-NEEDS-OVERLAY, 18
HAND-ONLY. What defeats mechanical lowering: parameterized combinators
(`call//N`), `code_type/2` character predicates (become regex tokens),
semantic actions, the runtime operator/precedence registry, and
variable-identity bookkeeping.

The failure condition the brief named was "does the overlay stay a small
fixed file, or grow per-construct". Round 1's overlay grows per-construct.

### Why round 1 does not settle the question

Three techniques the round-1 generator never attempted, each verified
against the code by the coordinator 2026-08-11:

| Technique | Round 1 called it | What the code actually shows |
|---|---|---|
| Read the precedence table | "runtime registry data", HAND-ONLY | `infix_op/2` (parse_dl_dcg.pl:967-972) and `tier_operators/2` (:1020) are `findall` goals over `surface/5` and `expression/5` facts; registry.pl holds 76 of them. An emitter that loads registry.pl runs the same query and emits prec declarations |
| Map character classes | every regex token HAND-ONLY | the parser uses four `code_type/2` classes (space :243, alnum :269, alpha :279, digit :290). A four-row mapping is written once and never grows |
| Specialize parameterized nonterminals | `call(P, X)` fatal | `sep/2` and `args/2` have 24 call sites binding P to 9 concrete parsers (`expr`, `atom_arg`, `typed_col`, `head`, `body`, `decl_a_column`, `enum_field`, `int_lit`, `rel_atom_term`). Partial evaluation emits one rule per binding, bounded by call sites |

Round 2 brief: `sprefa-lanes/tree-sitter-emit-v2.BRIEF.md`. Result, PR #178,
`v6/labs/tree-sitter-door/REPORT3.md`:

| Technique | Verdict | Hand overlay removed |
|---|---|---:|
| registry-driven precedence | WORKED | 508 chars |
| four-row character-class bridge | WORKED | 169 chars |
| separator specialization (14 emitted) | WORKED | 37 chars |

`DCG_EMIT_V2 specializations=14 unbound_args=2`; emitted 1648, remaining
overlay 2767, ratio **1.68**; classification 8 EMITTED-IDENTICAL / 27
EMITTED-NEEDS-OVERLAY / 8 HAND-ONLY (round 1 had zero identical). Gates:
`pnpm exec tree-sitter generate emitted-grammar.js` rc=0, `run-tests.sh`
rc=0, TS_CORPUS 272/272/0.

The verdict is now NO for a reason the code supports. Everything left is an
editor concern a compiler's parser never carried: where CST nodes begin and
end, field names, immediate token behavior, comment retention (the parser
discards comments; the editor grammar must keep them), escape-preserving
lexical tokens, and error recovery. Brief-vs-lane correction: the brief cited
24 sep/args call sites from a grep; the lane walked parsed terms and found 12
occurrences yielding 14 specializations. The lane's count stands.

## Phase C: LSP candidates and the recommended shot

| Candidate | Surface | Activity receipt |
|---|---|---|
| Langium 4.3.1 | grammar language generates AST types, parser services, validation/scoping/linking hooks, LSP framework | commit bbaa4b8, 2026-08-11 |
| lsp-tree-sitter | shared Python lib (Termux, Mutt servers); query/schema-driven completion + hover | commit af5ed28, 2026-08-01 |
| SWI prolog_lsp | JSON-RPC/LSP in SWI, stdio + TCP transports | commit 83d8c39, 2026-07-16 |
| treelsp | TS grammar-first generator: tree-sitter grammar, typed AST, highlights, LSP | commit c04e643, 2026-02-22 |
| compiler over stdio | swipl already parses and emits findings; oracle/manifest establish deterministic JSON | this worktree |

Full 12-capability fan-out table (syntax vs semantic diagnostics,
highlight, folding, outline, definition, references, completion, hover,
rename, formatting, code actions) x 5 candidates: REPORT2.md.

Recommendation: Langium as the generated LSP shell, a Phase-B-style
emitter targeting `.langium`, SWI compiler as semantic backend. Price:
(1) second emitter target with its own overlay, which needs its own
measurement before adoption given Phase B's 2.96 ratio; (2) lossless
AST-to-compiler bridge; (3) finding-to-diagnostic conversion with URI,
version, UTF-16 ranges, cancellation, stale suppression; (4)
compiler-backed relation/column indexes; (5) Topiary stays the formatter
and tree-sitter stays the CST provider unless Langium explicitly
replaces those consumers.

## Forks awaiting the user

| Fork | Why it needs a decision |
|---|---|
| Emit both tree-sitter and Langium, or let Langium's parser replace tree-sitter | tree-sitter currently feeds Topiary and incremental CST consumers; Langium generates its own parser |
| Model relation references as Langium cross-references, or let the compiler own all linking | the stale slice uses plain IDs and resolves in a bridge; generated navigation needs cross-reference declarations |
| Invoke the compiler per document version, or keep a resident SWI process | per-version simplifies cancellation; resident needs versioned state and reset rules |
| Publish tree-sitter syntax errors beside compiler findings, or compiler findings only | the two parsers recover differently on incomplete text and can report overlapping ranges |

## Round 3: CST/LSP fact tables in the DCG

User word 2026-08-11 opened the round: adding LSP/CST data to the parser is
allowed. `parse_dl_dcg.pl` gained 48 lines, zero deletions, all inert facts
(28 `cst_shape/2`, 3 `lex_token/2`, 1 `cst_extra/2`, 5 `cst_origin/2`); no
predicate in `v6/prolog/**` calls any of them.

| step | change | commit | overlay | emitted | ratio | strict ratio |
|---|---|---|---:|---:|---:|---:|
| 0 | round 2 baseline | `b580d627` | 2767 | 1648 | 1.6790 | 2.5532 |
| 1 | adjacency tokens + repetition pairs, emitter only | `2f0fd660` | 2503 | 1974 | 1.2680 | 2.5532 |
| 2 | `cst_shape/2` | `4ba09e59` | 2013 | 2580 | 0.7802 | 1.4614 |
| 3 | `lex_token/2` | `91ac792f` | 1888 | 2705 | 0.6980 | 1.3069 |
| 4 | `cst_extra/2` | `91ac792f` | 1856 | 2729 | 0.6801 | 1.2754 |
| 5 | `cst_origin/2` | `a35ef0a3` | 1239 | 3375 | 0.3671 | 0.7339 |
| 6 | `atom` without the call precedence | `7b9caa91` | 1121 | 3525 | 0.3180 | 0.6528 |

23 of the 43 editor rules moved to `EMITTED-IDENTICAL` and every one is
machine-checked: `measure.py` diffs the emitted body against `grammar.js` and
reddens `run-tests.sh` on any drift. The `strict` column counts only those 23;
round 2's 8 `EMITTED-IDENTICAL` verdicts were written judgments, so its strict
ratio equals its loose one.

Round 2's predicted floor of two irreducible rules is wrong on both counts.
`declaration_parameter` is generated: `decl_a_column//1`
(`parse_dl_dcg.pl:486-488`) itself admits the untyped column through a branch
that binds `Type = none`. `source_file` carries no recovery at all; its whole
body is `repeat($.statement)` (35 chars), `grep -n 'ERROR\|recover\|MISSING'`
over `grammar.js` returns nothing, and tree-sitter recovery is built into the
generated parser rather than declared. What blocks it is that
`statements//3` (`:264-274`) puts its recursive call in the `else` branch and
spells "no statement here" as a throwing side condition, which the emitter
reads as an empty alternative.

Remaining 1121 chars split three ways: 101 where the editor is deliberately
wider than the parser (`enum_variant`, `query`), 479 where no DCG nonterminal
names the node (`expression`, `literal`, `unary_expression`,
`parenthesized_expression`, `member_expression`), 541 of emitter gaps
(`source_file`, `relation_declaration`, `shell_declaration`, `column`,
`type`) needing repetition detection, longest-common-prefix factoring, and
resolving `call//3` through the specialization inventory. `column`'s two
concrete type parsers, `decl_b_column_type//3` and `host_col_type//3`, differ
only in a cut and stay unmerged.

Measured negative result: `atom`'s `prec(PREC.call, ...)` wrapper was doing
nothing. Removing it leaves `tree-sitter generate` clean and the 272-file
corpus at 272 clean.

### Round 3 forks awaiting the user

| Fork | Why it needs a decision |
|---|---|
| Field names for the `sh` input and output column lists | `grammar.js` names only `name` and `template`; placing `template` needs both lists named or explicitly skipped. Blocks `shell_declaration`, 171 chars |
| Field names for the `rel` column list and modifier list | the round-3 brief spells these `columns` and `modifiers`; neither string occurs in `grammar.js`. Blocks `relation_declaration`, 166 chars |
| A field name for a type's trailing `?` | `type` has `name` and `element` only. Blocks `type`, 102 chars |
| Enum variant field types: full type expression or bare identifier | `enum_field//1` reads the type with `ident//1`; the editor rule uses `$.type`, so the editor is wider. Narrowing removes `list(int)` from enum fields in the editor only |
| `? name(...)`: keep the `$.atom` node | `query_stmt//1` inlines `ident//1` and `head_args//1` and refuses dotted paths; the editor rule is wider on both counts |
| Show `set` as a relation modifier in the editor | `rel_modifiers//2` parses `~set` then calls `unsupported(removed_word(set))`; the editor grammar now shows it |

Eleven editor-visible corrections landed under a stated policy: where
`grammar.js` and the DCG disagreed, the DCG won provided the corpus stayed at
272/272. Six widened the editor (`member_access` and `capture_key` character
classes, `enum_variants` `repeat1` to `repeat`, `$.string` object keys, the
`set` modifier, `arm ; arm` without a leading semicolon), two narrowed it
(`list` spread only as the whole list, `bind_declaration` name to
`$.identifier`), three were shape-only.

## Where the code lives

`v6/labs/tree-sitter-door/` on main: `grammar.js`, `emit_grammar.pl`,
`emitted-grammar.js`, `measure.py`, `classification.tsv`,
`queries/formatting.scm`, `languages.ncl`, `run-tests.sh`, `REPORT.md` (slice
lab), `REPORT2.md`, `REPORT3.md` (round 2), and the round-3 receipts in this
section. `classification.tsv` is the checked 43-row verdict table that
`measure.py` reads.
Lab retirement waits on the user's go/no-go for the full arc, since the
grammar is the only complete non-DCG description of dl6 that exists.

## Round 4: three field names and the emitter gaps

The user (2026-08-11) confirmed the three field-name slots from round 3's
forks. Round 4 lands them plus four emitter gaps. The emitter gained two
general capabilities: a widened repetition detector (`statements//3`'s
tail-recursive else branch with a throwing guard reads as its empty base) and
`prec`/`repeat1` IR rendering. Four editor bodies (`relation_declaration`,
`shell_declaration`, `type` via `record decl`, and the three Part C nodes)
are emitted by targeted `ir_body/4` clauses that reproduce the editor's
fixed shapes; the three parser-folded Part C nodes use `editor_*` cst_shape
keys that are not parser predicates.

| step | change | overlay | emitted | ratio | command |
|---|---:|---:|---:|---|---|
| 0 | round 3 baseline | 1121 | 3525 | 0.3180 | `python3 measure.py` |
| A | 3 field-name cst_shape rows + grammar.js fields | 1208 | 3525 | 0.3427 | `python3 measure.py` |
| B | source_file detector + rel + shell + type bodies | 647 | 4158 | 0.1556 | `python3 measure.py` |
| C | literal + parenthesized_expression + member_expression | 445 | 4360 | 0.1021 | `python3 measure.py` |

Part A alone worsens the ratio: the field names land in `grammar.js` (and the
emitted bodies) before the emitter can generate the rules, so they sit in the
hand overlay until Part B makes the bodies fall. The field names
`inputs`/`outputs`/`columns`/`modifiers`/`optional` are wired emitters-side
because `place_fields/4` targets leaf nodes only; these slots wrap
`optional` column lists, the modifier `repeat`, and the trailing optional
marker, which the leaf-targeting field placer cannot express. Each name
appears once in `emitted-grammar.js`: `grep -o 'field("(inputs|outputs|
columns|modifiers|optional)"'` returns 1 each.

Classification walk, EMITTED-IDENTICAL 31 → 35 → 38; seven rules moved to
EMITTED-IDENTICAL this round: `source_file`, `relation_declaration`,
`shell_declaration`, `type`, `literal`, `parenthesized_expression`,
`member_expression`.

| rule | verdict | reason |
|---|---|---|
| source_file | EMITTED-IDENTICAL | widened repetition detector; guard reads as empty base |
| relation_declaration | EMITTED-IDENTICAL | ir_body collapses the three-way alternative into the editor seq |
| shell_declaration | EMITTED-IDENTICAL | ir_body lowers the full clause; no-output fallback drops |
| type | EMITTED-IDENTICAL | ir_body: identifier + optional paren element + optional marker |
| literal | EMITTED-IDENTICAL | ir_body: choice of the five literal leaves |
| parenthesized_expression | EMITTED-IDENTICAL | ir_body: seq("(", expression, ")") |
| member_expression | EMITTED-IDENTICAL | ir_body: prec(call) seq(variable, repeat1(member_access)) |

### The new floor: 445 chars, and why each piece cannot fall

| rule | chars | why it cannot fall |
|---|---:|---|
| expression | 206 | editor alternative list is the tier structure plus `factor//1` leaves; `expr//1`'s `tier_expr//2` names no tier. Needs editor precedence tiering, not a fact row |
| unary_expression | 71 | no DCG clause at all; the parser reads a leading minus inside `int_lit//1`/`float_lit//1`. Nothing to hang a node on |
| column | 67 | `typed_col//2` defers its type parser through `call(TypeP, Col, Type)` with `TypeP` unbound at parse time; its two concrete bindings `decl_b_column_type//3` and `host_col_type//3` differ only in a cut. Merging passes parity while silently widening the language; left unmerged (four-agent precedent) |
| enum_variant | 72 | editor wider than the parser: `enum_field//1` types a field with `ident//1`, the editor rule uses `$.type`. DECIDED keep-editor-wider, uniform with query |
| query | 29 | editor wider than the parser: keeps the `$.atom` node; `query_stmt//1` inlines `ident//1`/`head_args//1` and refuses dotted paths. DECIDED keep-editor-wider, uniform with enum_variant |

206 + 71 + 67 + 72 + 29 = 445. `column` stays hand-written; the other four
block because the DCG does not preserve the editor node (two) or the editor
is deliberately wider (two).

### The two user calls, decided

The delegated reading was uniform: an editor grammar wider than the parser is
correct and better, because the editor highlights half-typed text the parser
rejects. Both rows took that reading; neither was narrowed to the parser.

| rule | decision | reasoning |
|---|---|---|
| enum_variant | keep-editor-wider | `enum_field//1` types a field with `ident//1` (`parse_dl_dcg.pl:521`); the editor rule types with `$.type`, so `list(int)` and any other type expression highlights while typing. Narrowing the editor only shrinks highlight coverage so the emitter can win a 72-char row; that trade favors the emitter, not the editor |
| query | keep-editor-wider | `query_stmt//1` (`parse_dl_dcg.pl:750`) inlines `ident//1`/`head_args//1` and reads a plain `Name`, so a DCG-true body would drop the `$.atom` node and lose dotted names; the editor keeps `$.atom`, highlighting `? a.b(x)` and half-typed query names. Same trade, same call |

Consequence: both rows stay `EMITTED-NEEDS-OVERLAY` and the 101 chars (72 + 29)
remain in the floor. That is the intended floor, not a deficit.

### Two language questions, answered; one open

1. Enum variant field types: full type expression or bare identifier? The
   DCG `enum_field//1` reads the type with `ident//1`; the editor uses
   `$.type`, so the editor is wider. ANSWERED keep-editor-wider: full type
   expression stays in the editor.
2. `? name(...)`: keep the `$.atom` node? `query_stmt//1` inlines
   `ident//1` and `head_args//1` and refuses dotted paths; the editor rule is
   wider on both counts. ANSWERED keep-editor-wider: the `$.atom` node stays.
3. Show `set` as a relation modifier in the editor? `rel_modifiers//2`
   parses `~set` then calls `unsupported(removed_word(set))`; the editor
   grammar now shows it. These are the user's calls.

### Gates, verbatim

```
$ cd v6 && just parse-parity
PARSE_PARITY mode=classic-vs-dcg total=687 parity=687 skips=0 diffs=0   rc=0

$ cd v6/labs/tree-sitter-door && ./run-tests.sh
DCG_EMIT_V3 specializations=14 unbound_args=2 rule_bodies=30 output=<tmp>
PASS generate: emitted-grammar.js
PASS parse: golden-flex.dl6 lines=630 errors=0
TS_CORPUS total=272 clean=272 errors=0
PASS format: formatting law and idempotence
rc=0

$ cd v6 && just text-door
TEXT_DOOR compiled=272 byte_identical=272 failures=0
rc=0

$ cd v6 && just green-all
GREEN ALL FAILED after 195s, exit=1
```

`green-all` reports the same legs as the round's base measurement: 12 red
(`compile-speed`, `flagship`, `getting-started`, `golden-flex`, `leak-soak`,
`lsp-diags`, `memory-soak`, `plunit`, `rtkq-golden`, `scale-floor`,
`serve-leak-soak`, `tsv2-test`) and the remainder green. Zero legs turned red
versus the base PASS/FAIL set; `extraction-live` passes (the prebuilt
`v6/sprefa-extract/target/release/extract` binary is in place), and the
soak/scale/compile legs are the known-red and environment list
(`.github/CI-KNOWN-RED.md` plus the temp-file soak failures). Fact tables
changed no parsing: all three parity runs are parity==total, skips=0, diffs=0.
