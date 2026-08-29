# Selection

Counts as of 2026-08-28. Numbers are exact against `1_SOURCES.md` and `2_INVENTORY.md`.

## Found repositories

17 repositories found and documented:

1. wmannis/cl-gambol
2. olewhalehunter/cl-gambol
3. quek/paiprolog
4. matsud224/wamcompiler
5. guitarvydas/wam
6. thephoeron/cl-datalog
7. taarotman/logadat
8. inconvergent/cl-grph
9. nikodemus/screamer
10. nickdrozd/reazon
11. fiddlerwoaroof/reazon-cl
12. cage/cl-kanren (Codeberg)
13. inaimathi/cl-kanren-trs (GitHub mirror; upstream gitlab.common-lisp.net)
14. rgc69/si-kanren
15. kraison/vivace-graph (plus v1 and v2 archives, same family)
16. cl-model-languages/cl-prolog2
17. keithj/cl-prolog (referenced as related work in cl-prolog2's README; checked, not inventoried separately)

Plus 2 commercial documentation-only systems (Allegro Prolog, LispWorks Common Prolog) that have no repository.

## Distinct families

14 families:

| # | Family | Repos collapsed into it |
| --- | --- | --- |
| 1 | Gambol | wmannis, olewhalehunter |
| 2 | PAIP Prolog | quek/paiprolog (Norvig PAIP source) |
| 3 | WAM in CL | matsud224/wamcompiler, guitarvydas/wam |
| 4 | Commercial CL Prolog | Allegro Prolog, LispWorks Common Prolog |
| 5 | cl-datalog | thephoeron/cl-datalog |
| 6 | logadat | taarotman/logadat |
| 7 | cl-grph | inconvergent/cl-grph |
| 8 | Screamer | nikodemus/screamer (CMU original) |
| 9 | Reazon | nickdrozd/reazon, fiddlerwoaroof/reazon-cl |
| 10 | cl-kanren | cage/cl-kanren |
| 11 | cl-kanren-trs | inaimathi mirror |
| 12 | si-kanren | rgc69/si-kanren |
| 13 | VivaceGraph | vivace-graph v1, v2, v3 |
| 14 | cl-prolog2 | cl-model-languages/cl-prolog2, keithj/cl-prolog (related) |

## Runnable candidates

12 systems have enough source or documentation for a bounded probe and run on the installed toolchain (SBCL 2.6.7, swipl 10.0.2):

cl-gambol, paiprolog, wamcompiler, cl-datalog, logadat, cl-grph, screamer, reazon-cl, cl-kanren, si-kanren, vivace-graph, cl-prolog2 (needs swipl, installed), and LispWorks/Allegro are excluded here as commercial.

Caveats per candidate: `cl-gambol` and `cl-kanren-trs` carry no license file, which gates any vendored use but not a probe. `cl-prolog2` has no license file either.

## Research-only candidates

4 systems are research-only:

1. Allegro Prolog — proprietary, no source access
2. LispWorks Common Prolog — proprietary, no source access
3. guitarvydas/wam — CL but no license and no documentation; source inspection only
4. cl-kanren-trs — Quicklisp-loadable but frozen since 2012 and materially overlapping `cl-kanren`; no separate lab

## Duplicates collapsed

5 collapse operations:

1. olewhalehunter/cl-gambol into wmannis/cl-gambol (same Gambol system)
2. vivace-graph-v1 and vivace-graph-v2 into kraison/vivace-graph
3. nickdrozd/reazon (Emacs Lisp) folded as upstream of reazon-cl
4. keithj/cl-prolog folded as related work of cl-prolog2
5. PAIP book-copy repositories folded into quek/paiprolog

## Rejected noise

9 repositories rejected after checking:

1. namin/clpset-miniKanren — Scheme, not CL
2. namin/clpsmt-miniKanren — Scheme, not CL
3. reazon-research/ReazonSpeech — speech corpus, name collision only
4. acharal/wam — Haskell
5. addisonu/wamcc — C
6. rupertlssmith/hak_wambook — Java
7. patrocloschris/WAM-Prolog-Compiler — C
8. CKalt/prolog_wam_compiler — Rust
9. CKalt/pwam — Rust

## New lab folders

2 candidate folders added under `v7/labs/`, each distinct and probe-ready:

- `16_logadat` — active 2025 Datalog implementation, single file, MIT. Not covered by the existing Datalog labs (`4_cl_datalog` is dormant since 2015, `5_cl_grph` is graph-first).
- `17_si_kanren` — miniKanren with a real constraint store (disequality, `numbero`, `symbolo`, `absento`), active and in Quicklisp 2026-01-01. The existing miniKanren labs (`7_reazon_cl`, `8_cl_kanren`) cover constraint-free cores.

No existing lab was edited. `v7/labs/0_INDEX.md` gained rows 16 and 17.

## Blocked source access

1. `www.quicklisp.org/dist/*.txt` dist metadata endpoints returned 404 on direct fetch on 2026-08-28. Worked around by installing a project-local Quicklisp under `/tmp` and querying the live 2026-01-01 dist index through `ql-dist`. No user-global Quicklisp state was touched (none existed on this machine).
2. Commercial systems (Allegro Prolog, LispWorks Common Prolog) have no source access; documentation pages are pinned in `1_SOURCES.md`.
