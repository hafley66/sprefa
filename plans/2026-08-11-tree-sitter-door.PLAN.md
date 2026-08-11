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
