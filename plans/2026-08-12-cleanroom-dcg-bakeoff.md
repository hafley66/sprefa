# Clean-room DCG bakeoff: two agents, one brief, three artifacts each

## TOC
1. [What was asked](#1-what-was-asked)
2. [The result in one table](#2-the-result-in-one-table)
3. [The printer: both hand-wrote it](#3-the-printer-both-hand-wrote-it)
4. [The tree-sitter grammar: the two disagree, and the numbers are not comparable](#4-the-tree-sitter-grammar-the-two-disagree-and-the-numbers-are-not-comparable)
5. [Two term shapes for the same language](#5-two-term-shapes-for-the-same-language)
6. [What this says about our own parser](#6-what-this-says-about-our-own-parser)
7. [Verification the coordinator ran](#7-verification-the-coordinator-ran)

## 1. What was asked

Two flash4 lanes, the SAME brief, no communication, isolated from the codebase. User's word: "same input prompt with much hand holding but in isolation from the codebase so it avoids reading too much and is just told to go hard with swi prolog dcg from scratch to hit pretty printer and tree sitter outputs from same ideas we tried here."

Allowed reads: `v6/prolog/compile/SYNTAX.md` (380 lines) and the 397-file `dl_view/*.dl6` corpus. Forbidden: `parse_dl_dcg.pl`, `parse_dl.pl`, `print_dl.pl`, the whole `v6/labs/tree-sitter-door/` directory, every other `.pl` in the repo.

The brief made each lane try the two hard things in a fixed order, so the answer would be measured rather than assumed:

1. Run the DCG BACKWARDS for the printer before writing any printer code.
2. EMIT `grammar.js` by reading its own `dcg.pl` as Prolog terms before hand-writing any of it.

## 2. The result in one table

| metric | lane A | lane B |
|---|---|---|
| corpus parse | **397/397** | **397/397** |
| round-trip, term equality after re-parse | **397/397** | **397/397** |
| tree-sitter parse | **397/397** | **397/397** |
| `tree-sitter generate` | exit 0 | exit 0 |
| DCG lines | 337 | 296 |
| DCG nonterminals | 129 | 164 |
| printer lines | 161 | 151 |
| printer origin | **hand-written** | **hand-written** |
| emitter lines | 70 | 67 |
| grammar.js lines | 192 | 162 |
| tree-sitter named rules | 38 | 39 |
| rules the emitter reached | 4 of 38 | 21 of 39 |

Both hit a perfect score on all three corpus gates. Neither failed a single construct. The language is not the hard part.

## 3. The printer: both hand-wrote it

This is the finding. Two independent attempts, both built from the start with reverse mode as the stated goal, both tried it first as instructed, and both gave up and wrote the printer by hand.

| blocker | lane A | lane B |
|---|---|---|
| cuts | 6 | 1 |
| `code_type/2` character guards | 3 | 7 |
| if-then control glue (`->`) | 131 lines | 173 lines |
| negation lookaheads | 3 rules, 4 uses | not counted |
| arithmetic / precedence goals | 2 | 3 |
| char-token position reads | not counted | 14 |

Lane A: reverse mode "did not terminate. With an open list, `number` and the dotted `ident` generate arbitrarily long inputs before a later unification fails, so the search never terminates."

Lane B: "`phrase(dcg:program(D, R), Codes)` with `D`/`R` bound and `Codes` free does not terminate (killed after 120 s, no output)."

They wrote DCGs in visibly different styles. Lane A leaned on cuts (6 vs 1); lane B leaned on if-then glue (173 lines vs 131) and character guards (7 vs 3). Neither style is reversible. The non-reversibility does not come from a stylistic choice; it comes from parsing characters with guards, which every character-level DCG does.

## 4. The tree-sitter grammar: the two disagree, and the numbers are not comparable

| | lane A | lane B |
|---|---|---|
| rules emitted | 4 | 21 |
| rules hand-written | 34 | 18 |
| chars emitted vs hand, as each lane counted | 116 / 3413 | 390 / 298 |

Do not read those char columns against each other. They measure different things: lane A counted non-whitespace characters of the whole emitted-versus-hand grammar text, lane B counted only the rules it attributed. Lane B's two numbers sum to 688 while its `grammar.js` is 3608 bytes, so the two accountings are not the same accounting.

The rule counts are comparable, and they genuinely diverge: **4 of 38 versus 21 of 39.** The cause is a definitional difference the brief did not pin down.

| lane | what it counted as emitted |
|---|---|
| A | the emitter produced the rule body; only the four lexical token regexes qualified (`identifier`, `number`, `atom_literal`, `string_literal`) |
| B | the rule's token/seq/choice skeleton traces to a named DCG nonterminal, even where the emitter output needed assembly |

Under lane A's stricter reading, lane B's 21 would shrink. Under lane B's looser reading, lane A's 4 would grow. Neither lane cheated; the brief left the word "emitted" underspecified, which is my error to fix before any rerun.

What both agree on: the **lexer never comes out of the DCG**. Lane A: "exact token regexes cannot be recovered." Lane B: same four token rules in its hand overlay, for the same reason. `code_type/2` and `number_codes/2` describe characters procedurally, and tree-sitter wants a regex.

## 5. Two term shapes for the same language

Same corpus, same spec, two different designs.

Lane A collapsed everything into one expression tree:

```prolog
program(Decls, Rules, Queries)
Expr :: var(Name) | wildcard | int(N) | float(F) | neg(X)
      | atom(C) | string(C) | list([...]) | brace([...])
      | call(Name, [Expr,...])     % relations, wraps, ALL builtins
      | op(Op, Left, Right)        % precedence-climbed infix
```

Its stated finding: "a relation atom, a negation, decode, pre, coalesce, a comparison, and a `:=` bind are all just expr nodes. A body needs no separate grammar."

Lane B kept body items as their own kinds:

```prolog
program(Decls, Rules)
Body = [call(Name, Args), cmp(Op, A, B), bind(:=, LHS, RHS), var('true'), ...]
Expr = ... | plus(A,B) | minus(A,B) | times(A,B) | div(A,B) | mod(A,B)
```

Lane B spells each arithmetic operator as its own functor; lane A carries one `op(Op, L, R)` with a precedence table. Both round-trip 397/397, so both are adequate. Lane A's is the smaller surface for a fourth consumer to walk.

## 6. What this says about our own parser

Our `parse_dl_dcg.pl` has 69 cuts, 8 `code_type` guards, and 19 position reads, and `print_dl.pl` is 724 lines of independent code held to it only by `just roundtrip`. The temptation has been to read that as an accident of how ours grew.

Two agents that had never seen our parser, told explicitly to aim for reverse mode, landed on the same shape and hit the same wall. That is the evidence that the hand-written printer is not our mistake. A character-level DCG that guards its terminals is not run backwards.

The path the session already named stands: a fourth consumer of the term structure, in the invertible-syntax-descriptions family, rather than a reversed grammar.

The tree-sitter half is the opposite result. Both lanes DID get structure out of the DCG by reading it as terms, and lane B got more than half its rules that way. Our own `emit_grammar.pl` already does exactly this. The lexer is the part that will always be hand-written, in any of the three implementations.

## 7. Verification the coordinator ran

Every headline number was re-run in the lane's own worktree rather than read from its report.

| check | lane A | lane B |
|---|---|---|
| `results.tsv` row count | 397 | 397 |
| file set identical to the corpus | yes, `diff` clean | yes, `diff` clean |
| harness re-run by the coordinator | 397 parse / 397 round-trip | 397 parse / 397 round-trip |
| `tree-sitter generate` re-run | exit 0 | exit 0 |
| corpus parsed by the coordinator | 397/397 | 397/397 |
| corpus filename special-cased in any code path | none; the only `.dl6` string in either `dcg.pl` is the header comment | same |

## Process note, my error not theirs

Lane A used `git commit -n`, which its brief forbade, and disclosed it in its own report. Cause is mine: the brief told both lanes to copy the extract binary but not to run the two `pnpm install`s, so the pre-commit rail could not start its server and hard-blocked every commit. The rail never reached its grading step, so nothing was smuggled past a comment-budget finding.
