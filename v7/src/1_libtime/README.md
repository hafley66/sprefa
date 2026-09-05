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
reads macro claims and `expansion` edges from the closure. It slices that
program to claim writers, syntax constructors, `item` and `expansion` edge
writers, and their non-input helper dependency cone before evaluation.
Predecessor rows are derived from the active syntax `item` edges for that
round. Retained compiler predecessor seeds from the macro program are excluded
from macrotime.
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

Macro-created identities use the existing constructor algebra. The executable
fixture interns `GeneratedSyntax(Invocation, OutputOrdinal, TemplatePath)`,
then derives the generated node, syntax payload, copied source span, and
expansion edge in DL7. Equal runs preserve the identity and separate invocation
occurrences produce separate identities.

The same fixture defines `<+` without a parser or lowerer rule branch. It
constructs a fresh form whose first child is a fresh `<-` atom, copies the
remaining source children through their indexed `item` edges, and expands the
source form to that generated form. The source head remains an explicit action
relation; runtime meaning comes from the selected action and trigger relations.

`expand_syntax/5` accepts a checked program supplied by the caller.
`compile_unit_with_macros/4` retains that explicit test and embedding seam.
Normal `compile_dl7/4`, `compile_dl7_project/5`, and
`compile_dl7_project_rows/6` compile the numbered library under
`v7/macrotime/`, cache it by prelude and macro source content, expand every
source unit, then install the existing file and directory module graph around
the expanded units. `compile_dl7_macro_program/3` provides the raw bootstrap
path for a macro library. Project-authored macro imports remain to be derived
from module graph edges.

The standard-library cache retains the sliced checked program rather than its
complete compiler prelude and compiler rows. Its source defines its ordinal
closure over the kernel `predecessor` relation, so bootstrap compilation needs
only the kernel relation set. Claim rules with the exact shape “form item zero
has literal atom Name” form a dispatch index. When every claim writer has that
shape and the active graph contains none of those names, expansion returns the
input graph exactly without evaluator closure. A general claim rule makes the
dispatch result unknown and retains the full evaluation path.
