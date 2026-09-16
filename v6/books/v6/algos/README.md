# books/v6/algos — one algorithm per file

Every file is self-contained, runnable, and graded:
`swipl -q -l <file> -g go -g halt` prints PASS per check. Duplicated helper
lines between files are deliberate; standalone beats shared imports at
paragraph scale.

| file | algorithm | one-line idea |
|---|---|---|
| unify_hm.pl | Hindley-Milner | 4 clauses; unification is the engine; occurs check rejects self-application |
| clock_calculus.pl | Lustre clocks | same solver, term grammar `base \| on(C, S)`; merge replaces current |
| initialization.pl | init analysis | lattice fold over {d, u}; pre poisons, arrow cures |
| retention.pl | window bound | max pre depth = ticks of history to keep = LAG bound |
| causality.pl | causality check | instantaneous-dep graph + tabled closure; pre cuts edges |
| seminaive.pl | semi-naive fixpoint | known + frontier; subtract, union, repeat; cycles terminate |
| magic_sets.pl | demand transform | subscribe as a fact, demand propagates as a rule, guards make cold parts stay cold |
| marble.pl | bidirectional DCG | one grammar parses "ab--c\|" and prints it back |
| lower_sql.pl | rule -> SELECT | shared holes become join conditions; unsafe heads fail free |
| sexp_cst.pl | CST reflection + matching | tree-sitter S-expr -> terms; metavariable = hole; non-linear = same hole twice |
| json.pl | JSON (ECMA-404) | the full spec as ~45 DCG lines |
| json5.pl | JSON5 | ~35 lines of delta: comments in ws, trailing commas by recursion choice, hex, .5/5., Infinity |

Composed labs (multi-algorithm, one directory up in `books/v6/`):

| file | composition |
|---|---|
| hm.pl | HM + Peano + clock reuse, with full teaching notes |
| lustre.pl | all four Lustre analyses over one AST |
| drills.pl | prolog fundamentals with a grader |
| dl_in_prolog.pl | ops syntax + lowering + semi-naive over a piped sqlite3, tabling as oracle |
| dl_to_ts.pl | the arrow language (`<=` `<+` `<~`) lowered to direct TypeScript, run under node |
| clocked_terms.pl | clock pass beside safety pass + terms-as-column-values |
| rel_island.pl | term_expansion: `<-` datalog island inside a prolog file |
| enum_match.pl | rust enums/match/effect envelopes on the HM kernel |
