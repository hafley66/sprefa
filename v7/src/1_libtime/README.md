# DL7 evaluation-time graph transforms

Dependency order:

```text
0_evaluator.pl
0a_syntax_macro_program.pl
0b_syntax_rewriter.pl
1_syntax_expander.pl
```

`0_evaluator.pl` closes a checked stratified Datalog program.
`0a_syntax_macro_program.pl` resolves the syntax protocol relations from an
already-checked DL7 program, converts syntax graph rows to evaluator calls, and
reads macro claims and `expansion` edges from the closure.
`0b_syntax_rewriter.pl` applies those claims to the active top-level and nested
item sequences. `1_syntax_expander.pl` owns repeated rounds and termination.

The protocol consists of the reader graph relations `syntax_frontier/2`,
`syntax_form/1`, `syntax_atom/2`, `syntax_literal/2`, `syntax_variable/3`, and
`syntax_source/8`, plus `syntax_claim/2`. A macro output is the ordinary edge:

```text
:(Invocation, expansion, OutputNode, Ordinal)
```

A claim with no expansion edges deletes the invocation. One edge replaces it.
Several dense edges splice their targets in ordinal order. Unclaimed forms keep
their identity while rewritten child edges are reindexed. Rows unreachable from
the resulting frontier are absent from the next round. Provenance is retained
separately as `expansion_claim/3` and `expansion_output/5` rows.

`expand_syntax/5` currently accepts a checked program supplied by the caller.
The compiler still uses `0_reader/1_expander.pl` while constructing
`dl7_unit/5`; automatic macrotime phase selection is a later integration cut.
