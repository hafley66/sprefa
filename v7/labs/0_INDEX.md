# V7 Common Lisp Logic Labs

Research start: 2026-08-28

Shared skill:

```text
/Users/chrishafley/projects/claude-research/skills/common-lisp-logic/SKILL.md
```

Each folder is one experiment variant. Every worker owns exactly one folder and does not commit because all variants share the checkout.

| Order | Variant | Category | Initial question | Status |
| ---: | --- | --- | --- | --- |
| 1 | `1_inventory` | research | Which Common Lisp logic systems are distinct, available, and lab-worthy? | completed |
| 2 | `2_cl_gambol` | Prolog library | How much Prolog behavior does `cl-gambol` supply? | completed |
| 3 | `3_paiprolog` | Prolog compiler | What does the PAIP-derived Prolog compiler supply? | completed |
| 4 | `4_cl_datalog` | Datalog library | Does `cl-datalog` execute recursive Datalog and terminate on cycles? | completed |
| 5 | `5_cl_grph` | graph plus Datalog | Can `cl-grph` cover compiler graph queries and recursive rules? | completed |
| 6 | `6_screamer` | nondeterminism plus constraints | Which SWI search and constraint facilities map to Screamer? | completed |
| 7 | `7_reazon_cl` | miniKanren | What relational core, fairness, and constraints does `reazon-cl` supply? | completed |
| 8 | `8_cl_kanren` | miniKanren | Is `cl-kanren` runnable and materially distinct from `reazon-cl`? | completed |
| 9 | `9_vivace_graph` | graph database plus Prolog | Can VivaceGraph's query layer cover durable compiler graph queries? | completed |
| 10 | `10_wamcompiler` | WAM Prolog | Can the WAM compiler run and produce a usable CL-hosted Prolog engine? | completed |
| 11 | `11_cl_prolog2` | external Prolog bridge | How short is the CL-to-SWI path through `cl-prolog2`? | completed |
| 12 | `12_handwritten_logic` | controlled implementation | What does the smallest CL unification and fair-search kernel require? | completed |
| 13 | `13_racket_crosswalk` | comparison | Which Racket libraries provide the shortest routes to useful SWI facilities? | queued |
| 14 | `14_binary_packaging` | deployment | What are the measured executable and distribution shapes? | queued |
| 15 | `15_commercial_common_prolog` | research | What do Allegro Prolog and LispWorks Common Prolog cover? | queued |
| 16 | `16_logadat` | Datalog library | Does `logadat` (2025) execute recursive Datalog and terminate on cycles? | queued |
| 17 | `17_si_kanren` | miniKanren plus constraints | Does `si-kanren` supply the constraint store (disequality, `numbero`, `symbolo`, `absento`) on the shared fixture? | queued |

The inventory report owns the final candidate count. A repository search result alone does not establish a distinct or runnable library.
