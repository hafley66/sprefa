# Racket Logic Crosswalk Brief

Read the complete `common-lisp-logic` skill and its references. Own only this folder. Do not commit.

Research current official Racket facilities and packages for:

- `datalog` and `datalog/sexp`
- Racklog
- miniKanren and cKanren-family packages
- Rosette
- syntax objects and `#lang` implementation
- `raco exe` and `raco distribute`

Write:

- `1_SOURCES.md` with dated official docs, repositories, versions, and package links
- `2_SWI_CROSSWALK.md` with one row per useful SWI facility, shortest Racket route, semantics, and missing machinery
- `3_DL7_PHASE0.md` describing reader, scope, macro, logic, and packaging boundaries

Use the crosswalk reference as a starting hypothesis and correct it with source receipts. Racket is absent from PATH, so record runnable examples from authoritative docs and mark local execution unavailable unless a project-local route is possible without system mutation.
