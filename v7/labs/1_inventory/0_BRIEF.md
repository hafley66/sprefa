# Common Lisp Logic Inventory Brief

Read the complete skill at `/Users/chrishafley/projects/claude-research/skills/common-lisp-logic/SKILL.md` and every linked reference.

Own only `v7/labs/1_inventory`. Do not commit.

Find currently accessible Common Lisp systems in these categories:

- Prolog interpreters and compilers
- Datalog engines
- miniKanren and relational programming
- nondeterministic and constraint programming
- Common Lisp bridges to external Prolog runtimes
- commercial Common Lisp Prolog systems

Start with the candidates in `v7/labs/0_INDEX.md`. Search GitHub, Quicklisp metadata, official documentation, releases, source, issues, and tests. Collapse forks and PAIP copies into families while preserving individual repository links.

Write:

- `1_SOURCES.md`: dated authoritative links and exact repository metadata
- `2_INVENTORY.md`: one row per distinct system with category, API shape, algorithm, tabling, constraints, update model, license, latest release or commit, SBCL compatibility, install route, and assigned lab
- `3_SELECTION.md`: exact counts for found repositories, distinct families, runnable candidates, research-only candidates, duplicates, and rejected noise

Add a candidate folder only when it is distinct and has enough source or documentation for a bounded probe. Use the next numeric prefix and update `v7/labs/0_INDEX.md`. Do not edit another existing lab.
