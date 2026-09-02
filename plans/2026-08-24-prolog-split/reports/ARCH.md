# v6/prolog/ARCH.pl -> v6/prolog/ARCH/

module head keeps lines 1..149 (149 lines): 1 directives, 0 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_species.pl` | 218 | 150-367 | 84 | 8 |
| `1_constructs.pl` | 94 | 368-461 | 48 | 3 |
| `2_covers.pl` | 156 | 462-617 | 117 | 2 |
| `3_forks.pl` | 66 | 618-683 | 16 | 1 |
| `4_tasks.pl` | 288 | 684-971 | 265 | 1 |
| `5_gate.pl` | 56 | 972-1027 | 13 | 5 |
| **total** | **878** | | | |

parts over 700 lines: none

## clauses of one predicate landing in two parts

none

## directives sitting below the first anchor

| line | directive | part it falls in |
|---|---|---|
| 315 | `:- use_module('src/kernel.pl')` | `0_species.pl` |
| 366 | `:- use_module('conformance/rulings.pl')` | `0_species.pl` |
| 602 | `:- dynamic arch_dir/1` | `2_covers.pl` |
| 603 | `:- prolog_load_context(directory,_970874),asserta(arch_dir(_970874))` | `2_covers.pl` |
| 1007 | `:- use_module('src/grader',[run/1])` | `5_gate.pl` |

Each one moves up into the module head file, above the includes.

## cross-part call edges

| from | to | callees |
|---|---|---|
| `5_gate.pl` | `0_species.pl` | `refines/2` |
| `5_gate.pl` | `4_tasks.pl` | `task/3` |

2 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_species.pl` | the graph, refines, species, algorithm, prior_art, capability, tech and technique rows |
| `1_constructs.pl` | the construct roster with its status and tier vocabularies |
| `2_covers.pl` | which construct each endpoint covers, and the endpoint existence check |
| `3_forks.pl` | the open design fork rows |
| `4_tasks.pl` | the task rows |
| `5_gate.pl` | roadmap, topsort, the check rows and go/0 |
