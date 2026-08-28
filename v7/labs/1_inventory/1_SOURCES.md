# Inventory Sources

Research date: 2026-08-28.

Metadata below was pulled on 2026-08-28 from the GitHub REST API (`api.github.com`), the Quicklisp 2026-01-01 dist index (queried with a project-local Quicklisp install under `/tmp`, since `www.quicklisp.org/dist/*.txt` returned 404 over direct fetch), and each repository's README or LICENSE file. Commit SHAs and dates are the latest commit observed at research time.

Quicklisp 2026-01-01 dist membership (verified by walking `ql-dist:provided-releases`): `cl-kanren` (2024-10-12 archive), `cl-kanren-trs` (2012-03-05 archive), `cl-prolog2` (2021-12-09 archive), `paiprolog` (2018-02-28 archive), `si-kanren` (2026-01-01 archive). `screamer`, `gambol`, `cl-datalog`, `logadat`, `cl-grph`, `reazon-cl`, and `vivace-graph` are absent from the current dist and install from source.

## Prolog interpreters and compilers (Common Lisp)

- Gambol family — https://github.com/wmannis/cl-gambol
  - Extraction of the logic-programming portion of the Frolic system (University of Utah). Pure CL, no dependencies. Package `:gambol`; facts/rules via `*-` macro. README states it is nowhere near ISO Prolog and that some operator semantics (e.g. `retract`) differ from compliant Prolog.
  - Default branch `master`, latest push 2018-02-12T20:41:34Z, 22 stars, license: none on file (GitHub `NOASSERTION`).
  - Sibling copy: https://github.com/olewhalehunter/cl-gambol (created 2018-08-12, latest push 2020-05-11T00:29:02Z, 2 stars, no license file). Same system, treated as one family.
- PAIP Prolog family — https://github.com/quek/paiprolog
  - Peter Norvig's PAIP Prolog interpreter and compiler (`prolog1/2/3`, `prologc*`) packaged with an ASD (`paiprolog.asd`, `unifgram.asd`). License file present in repo (`license.html`, Norvig's PAIP terms).
  - Default branch `master`, latest push 2018-02-24T07:51:55Z, 20 stars.
  - Quicklisp: release `paiprolog` 2018-02-28, systems `paiprolog`, `unifgram`.
  - Family note: the source of truth is Norvig's PAIP book code; other PAIP copies on GitHub are book-typing copies and collapse into this family.
- WAM compiler family — https://github.com/matsud224/wamcompiler
  - Prolog compiler written in Common Lisp emitting WAM bytecode. README recommends SBCL explicitly.
  - Default branch `master`, latest push 2019-12-18T08:55:32Z, 44 stars, license Unlicense (GitHub API).
  - Sibling: https://github.com/guitarvydas/wam (a WAM in Common Lisp, no license on file, latest push 2019-03-24T13:25:34Z, 13 stars). Distinct implementation, kept as its own row in the inventory but the same WAM-in-CL category.
- Allegro Prolog (commercial, research-only) — https://franz.com/products/prolog/index.lhtml
  - Embedded Prolog in Allegro CL. Proprietary; no source access. Documentation page is the authoritative reference.
- LispWorks Common Prolog (commercial, research-only) — https://www.lispworks.com/documentation/lw80/kw-w/kw-prolog-1.htm
  - KnowledgeWorks Prolog inside LispWorks, includes DCG support. Proprietary; documentation page is the authoritative reference.

## Datalog engines

- cl-datalog — https://github.com/thephoeron/cl-datalog
  - A Common Lisp DSL for Datalog. MIT license on file.
  - Default branch `master`, latest push 2015-09-03T15:34:46Z, 9 stars.
  - Quicklisp badge on the README but absent from the 2026-01-01 dist index; installs from source.
- logadat — https://github.com/taarotman/logadat
  - Single-file Datalog implementation (`logadat.lisp`, one file plus a writeup). MIT license on file.
  - Default branch `main`, latest commit `23fc43cc91` 2025-12-20T15:18:25Z, 1 star. Active in 2025; not in Quicklisp.
- cl-grph — https://github.com/inconvergent/cl-grph
  - In-memory immutable graph structure (built on fset) with a Datalog query language; rules and fixed-point iteration supported in `grph:rqry`; subsets of `and`, `or`, `not`, `or-join`, `not-join`. MIT license on file.
  - Default branch `master`, latest push 2026-01-13T21:43:07Z, 73 stars. Active in 2026; not in Quicklisp.
  - Author essays: https://inconvergent.net/2022/graph-data-structure-with-datalog-ql/ and https://inconvergent.net/2023/datalog-to-svg/

## miniKanren and relational programming

- cl-kanren — https://codeberg.org/cage/cl-kanren
  - miniKanren wrapped around a microKanren core. Copyright (c) 2016, cage; BSD-style redistribution terms in `COPYING`. Upstream per Quicklisp `source.txt`: `git https://codeberg.org/cage/cl-kanren.git`.
  - Last activity 2024-12-03T05:21:24+01:00 (Codeberg API), 2 stars on Codeberg.
  - Quicklisp: release `cl-kanren` 2024-10-12, systems `cl-kanren`, `cl-kanren-test`.
- cl-kanren-trs — https://gitlab.common-lisp.net/cl-kanren-trs/cl-kanren-trs (GitHub mirror: https://github.com/inaimathi/cl-kanren-trs)
  - "The Reasoned Schemer" miniKanren port. No license file on the GitHub mirror.
  - GitHub mirror latest push 2016-11-01T18:25:59Z, 7 stars.
  - Quicklisp: release `cl-kanren-trs` 2012-03-05 (svn), systems `kanren-trs`, `kanren-trs-test`.
- Reazon family — https://github.com/fiddlerwoaroof/reazon-cl
  - Common Lisp port of Reazon, itself an Emacs Lisp miniKanren: https://github.com/nickdrozd/reazon (GPL-3.0, Emacs Lisp, latest push 2026-01-01T19:31:22Z, 118 stars). reazon-cl inherits GPL-3.0.
  - Default branch `main`, latest push 2022-09-28T05:48:47Z, 4 stars. Not in Quicklisp.
- si-kanren — https://github.com/rgc69/si-kanren
  - microKanren in Common Lisp with disequality, `numbero`, `symbolo`, and `absento` constraints (constraint store `cs = '(((s) . c) (d) (t) (a))`). Pure CL without external libraries. MIT license on file.
  - Default branch `main`, latest push 2025-12-30T08:56:14Z, 11 stars.
  - Quicklisp: release `si-kanren` 2026-01-01, system `si-kanren`.

## Nondeterministic and constraint programming

- Screamer — https://github.com/nikodemus/screamer
  - Nondeterministic Common Lisp with finite-domain constraint search. License on file: MIT-style permission statement (Copyright 1991 MIT, 1992/1993 UPenn, 1993 U. Toronto). This is the maintained fork of the original CMU distribution.
  - Default branch `master`, latest push 2024-04-15T02:26:08Z, 256 stars, no release tags.
  - Not in the current Quicklisp dist; installs from source.
- Screamer successors: no maintained successor found. A `screamer-plus` repo search on 2026-08-28 returned nothing (404 on the candidate handle; GitHub search listed unrelated projects).

## Common Lisp bridges to external Prolog runtimes

- cl-prolog2 — https://github.com/cl-model-languages/cl-prolog2
  - S-expression-to-ISO-Prolog transpiler with backends invoking SWI-Prolog, XSB, YAP, GNU Prolog, and B-Prolog in batch mode. Systems `cl-prolog2.swi`, `.gprolog`, `.xsb`, `.yap`, `.bprolog` plus test systems. No license file on GitHub.
  - Default branch `master`, latest push 2021-11-21T17:34:09Z, 39 stars.
  - Quicklisp: release `cl-prolog2` 2021-12-09.
  - Related work named in the README: https://github.com/keithj/cl-prolog (FFI-based, per-backend bindings).
  - Local runtime check 2026-08-28: `swipl` 10.0.2 present on PATH (`/opt/homebrew/bin/swipl`), so the SWI backend is runnable on this machine.

## Graph database plus Prolog

- VivaceGraph family — https://github.com/kraison/vivace-graph
  - Pure-CL ACID graph database with map-reduce views, MVCC versioned nodes, spatial extension, graph-algorithms add-on exposing streaming algorithms as Prolog predicates, and Prolog inference. MIT license text on file (Copyright (c) 2026 Kevin Thomas Raison).
  - Default branch `master`, latest commit `68230b3879` 2026-08-09T09:17:49Z ("Merge experiment into master: VivaceGraph 3.0.0"), pushed 2026-08-28, 199 stars. Active in 2026.
  - Older family members (collapse, not separate candidates): https://github.com/kraison/vivace-graph-v2 (2012-05-07), https://github.com/kraison/vivace-graph-v1 (2010-10-12).

## Rejected noise (checked and excluded)

Checked on 2026-08-28 and excluded from the inventory for the reasons given:

- https://github.com/namin/clpset-miniKanren — Scheme, not Common Lisp (repo contents are `.scm` files; GitHub language field: Scheme).
- https://github.com/namin/clpsmt-miniKanren — Scheme (GitHub language field: Scheme).
- https://github.com/rgc69/si-kanren — kept (CL); listed here only to note the sibling search results were checked.
- https://github.com/reazon-research/ReazonSpeech — speech corpus project, unrelated to logic programming despite the name.
- https://github.com/acharal/wam — Haskell, not CL (GPL-2.0).
- https://github.com/addisonu/wamcc — Prolog-to-C compiler in C, not CL.
- https://github.com/rupertlssmith/hak_wambook — Java implementation of the WAM book, not CL.
- https://github.com/patrocloschris/WAM-Prolog-Compiler — C, not CL.
- https://github.com/CKalt/prolog_wam_compiler and https://github.com/CKalt/pwam — Rust, not CL.
- WAM book resources (https://github.com/a-yiorgos/wambook and wambook.sourceforge.net) — documentation, not systems.
