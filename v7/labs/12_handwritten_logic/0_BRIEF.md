# Handwritten Common Lisp Logic Kernel Brief

Read the complete `common-lisp-logic` skill and its references. Own only this folder. Do not commit.

Implement the smallest readable first-order relational kernel in portable Common Lisp with:

- unique logic variables
- `walk`, occurs check, and nested unification
- persistent substitutions
- lazy answer streams
- fair disjunction and conjunction
- fresh variables
- bounded answer reification
- facts and Horn-style rule helpers sufficient for the shared fixture

Keep the kernel under 300 nonblank, noncomment Common Lisp lines. Add no parser, macro language, CLP solver, WAM, or general tabling system. The cyclic fixture must either use an explicit bounded adapter or report that the kernel lacks completion semantics. Make that boundary visible in output.

Build an SBCL executable and measure it under the shared report contract. Write source files in dependency order and include `4_RESULTS.md`.
