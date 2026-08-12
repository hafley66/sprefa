# One DCG, three artifacts

## TOC

1. [The claim, corrected](#the-claim-corrected)
2. [The trick: monomorphization](#the-trick-monomorphization)
3. [Why the 445-char floor exists](#why-the-445-char-floor-exists)
4. [Reading order](#reading-order)
5. [Could the pretty printer come from the DCG](#could-the-pretty-printer-come-from-the-dcg)

## The claim, corrected

The working belief was "parser, pretty printer, and tree-sitter grammar all
come from one DCG". Two of the three do. The third is held to it by a test.

| artifact | file | relationship to the DCG |
|---|---|---|
| parser | `v6/prolog/compile/parse_dl_dcg.pl` | IS the DCG, run forward through `phrase/3` |
| pretty printer | `v6/prolog/print_dl.pl`, 724 lines | independent code; imports `analyze`, `0_rel_record`, `registry`, `0_cst_query`, never the DCG |
| tree-sitter grammar | `v6/labs/tree-sitter-door/emit_grammar.pl` | reads the DCG AS TERMS and compiles it |

The pretty printer is kept in agreement by `just roundtrip`: print to `.dl6`
text, reparse through the DCG, compare terms. `ARCH.pl:815` records the one
time they drifted, and it is the argument for keeping that gate: `parse_dl`
kept both declaration forms, `print_dl` printed both, the reparse dropped the
type. 18 red fixtures from one defect.

## The trick: monomorphization

`parse_dl_dcg.pl:447-450`:

```prolog
sep(P, [X | Xs]) -->
    call(P, X), ws,
    ( @`,` -> ws, sep(P, Xs) ; { Xs = [] } ).

args(P, Xs) --> ws, ( peek(0')) -> { Xs = [] } ; sep(P, Xs) ).
```

`P` is a parser passed as an argument, invoked by `call/N`. One `sep//2`
serves every comma-separated list in the language.

tree-sitter's `grammar.js` has no parameterized rules. A rule cannot take
another rule as an argument. So the emitter monomorphizes, the same
transformation Rust applies to generics.

```
step 0  read parse_dl_dcg.pl as TERMS, do not run it
        emit_grammar.pl  read_source_terms(Input, Terms)

step 1  find nonterminals whose argument reaches call/N
        -> sep//2, args//2

step 2  visit every call site, collect the concrete P
        sep(atom_arg, _)                          -> sep-atom_arg
        sep(decl_a_column, _)                     -> sep-decl_a_column
        sep(expr, _)                              -> sep-expr
        sep(int_lit, _)                           -> sep-int_lit
        sep(rel_atom_term, _)                     -> sep-rel_atom_term
        sep(typed_col(decl_b_column_type(A)), _)  -> sep-typed_col(...)
        ...                                          14 pairs

step 3  emit ONE first-order rule per pair
        sep_typed_col_decl_b_column_type___VAR__0___:
          $ => seq($.column, repeat(seq(",", $.column)))

step 4  P UNBOUND at a call site -> cannot monomorphize
        unbound: [args-A, sep-A]
        that pair stays hand-written in the overlay
```

Every one of those steps is recorded in the emitted file's own header.
`emitted-grammar.js:5` lists all 14 specializations verbatim, `:6` states
`unbound: "[args-A,sep-A]"`, `:7` lists the 29 rules generated wholesale, and
`:8` states `blocked: "[]"`.

## Why the 445-char floor exists

The hand-written overlay cannot fall below 445 non-whitespace characters. Five
rules, three distinct causes.

| rule | chars | cause |
|---|---:|---|
| `expression` | 206 | the editor's alternative list is a precedence tier structure; `expr//1`'s `tier_expr//2` names no tier. Needs editor precedence tiering, not a fact row |
| `unary_expression` | 71 | no DCG clause exists at all; the parser reads a leading minus inside `int_lit//1`/`float_lit//1`, so there is no node to hang a rule on |
| `column` | 67 | **the monomorphization gap.** `typed_col//2` defers its type parser through `call(TypeP, Col, Type)` with `TypeP` unbound; its two concrete bindings differ only in a cut. Merging them passes parity while silently widening the language, so they stay unmerged |
| `enum_variant` | 72 | editor deliberately wider than the parser: `enum_field//1` types a field with `ident//1`, the editor rule uses `$.type` |
| `query` | 29 | editor deliberately wider: editor keeps the `$.atom` node, `query_stmt//1` inlines `ident//1`/`head_args//1` and refuses dotted paths |

206 + 71 + 67 + 72 + 29 = 445.

The last two were delegated to a lane on 2026-08-11 ("i got zero opinions
about auto tree sitter outputs from parser") and both were decided
keep-editor-wider, on the reasoning that an editor grammar wider than the
parser is correct because the editor highlights half-typed text the parser
rejects. Those 101 characters are intentional, not debt.

The `column` row is the interesting one: it is the single slot monomorphization
cannot reach, and the emitter prints it in its own header on every run.

## Reading order

| order | file | why |
|---|---|---|
| 1 | `compile/parse_dl_dcg.pl:447-451` | five lines: `sep`, `args`, and `#Cs --> ws, @Cs`. The whole higher-order idea |
| 2 | `labs/tree-sitter-door/emitted-grammar.js:1-8` | the emitter's self-report: inputs, specializations, unbound, generated, blocked |
| 3 | `labs/tree-sitter-door/emit_grammar.pl:17-40` | `main/1`, about 24 lines, the entire pipeline |
| 4 | `compile/parse_dl_dcg.pl:454` | `rel_stmt//1`, a real production using it |

## Could the pretty printer come from the DCG

Not as written. Measured in `parse_dl_dcg.pl`:

| blocker | count | why it blocks reverse mode |
|---|---:|---|
| cuts `!` | 69 | removes the alternatives a generator must backtrack through |
| `code_type(C, alpha)` guards | 8 | with `C` unbound this does not enumerate characters usefully |
| `mark(...)` / `peek(...)` | 19 | read the input position, meaningless when generating |

A textbook DCG with no cuts and no guards runs both directions for free. This
one is written in parser mode deliberately, and 96 sites would have to change.

Not from tree-sitter either: tree-sitter grammars are recognizers, and the
emitted `grammar.js` has already discarded the spacing and precedence
information a printer needs.

The path that does exist is the one already built. `emit_grammar.pl` reads the
DCG as terms and produces a third artifact; a printer would be a fourth
consumer of the same term structure, monomorphizing `sep`/`args` into
"element, comma, element" the same way.

Before anyone writes one: this is the **invertible syntax descriptions**
problem (Rendel and Ostermann, 2010). Haskell's `invertible-syntax` and
`partial-isomorphisms`, and the Racket and Scala equivalents, solve it by
making the combinators bidirectional by construction. That literature is the
first read, not a bespoke design, per the build-vs-buy law.
