# Brief: one mermaid per chapter of the dl8 book, chained: each diagram's entry nodes are the previous diagram's exit nodes

## 1. Job
Add exactly one mermaid block to every chapter of `v8/book/src/` (chapters `1_run.md` through `16_not_built.md`; `0_why.md` keeps its existing block and becomes diagram 0). The blocks form one chain through the book: the entry node(s) of chapter N+1's diagram are, by node id AND label, the exit node(s) of chapter N's diagram. A reader who lays the 17 blocks end to end sees one continuous pipeline from `.dl7` text to rows in a store, with the kernel in the middle. Every node names a real thing: a `src/` folder, a file, a relation, a verb, a table. No abstract boxes.

Chris, the language's owner, is re-orienting after a long absence from this code. Diagrams that read as lists, or that name things not in the tree, cost him more than no diagram.

## 2. Base and first action
- Base sha: `256a140edd2da7a1c71e4443be56ef679a3c8763` (`origin/docs/v8-book`, PR #767, the book itself). Branch `docs/v8-book-diagrams`. PR base `docs/v8-book`.
- FIRST command: `git merge --ff-only 256a140edd2da7a1c71e4443be56ef679a3c8763`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Commands run from `v8/`.

## 3. Ownership
You own: the 16 chapter files `v8/book/src/1_run.md` .. `16_not_built.md` (ONLY by inserting one fenced `mermaid` block per file directly under the `## What` heading, before its first sentence; `1_run.md` already has one under What: replace it), `v8/book/src/0_why.md` (only to make its existing block's exit node ids match diagram 1's entry), `v8/tests/_22_book.rs` (add tests, change none), `v8/book/check_chain.sh` (new). Forbidden: every other line of every chapter, `v8/src/**`, `v8/fixtures/**`, `v8/oracle/**`, `book.toml`, `SUMMARY.md`, `plans/**`. Never spawn subagents. Never `--no-verify`.

## 4. Where the truth is
| for | read |
|---|---|
| pipeline folders and verbs | `v8/README.md` first table, `src/bin/dl8.rs:138-144`, `plans/v8/2026-09-11-v8-structure.d2` |
| the kernel board already drawn | `plans/v8/2026-09-13-v8-kernel.d2` (convert its shapes to mermaid for chapter 5; drop shapes past the budget) |
| kernel relations, arity, keys | `src/_3_check/_5_kernel.rs:16-60` |
| evaluator rounds | `src/_6_eval/_5_evaluate.rs:727-838`, chapter 3 step trace |
| effect loop | `src/_9_runtime/_2_reconcile.rs`, the mermaid in `v8/README.md` "dl8 run" |
| executors | `v8/README.md` "Executor roster" |
| store tables | `src/_9_runtime/`, chapter 12 |
| what each chapter already says | the chapter's own What and What proves it tables; the diagram draws what the chapter's text already cites, nothing new |

## 5. The chain, fixed
| ch | diagram entry (= previous exit) | diagram exit (= next entry) | body |
|---|---|---|---|
| 0 | | `dl7[file.dl7]` | keep as is |
| 1 | `dl7` | `compile[compile JSON]`, `run[dl8 run]`, `emit[dl8 emit sqlite]` | verbs |
| 2 | `dl7` | `colon[: rows, the type graph]` | read -> lower: declarations to `:` rows |
| 3 | `colon` | `rules[rule rows: head, goals]` | facts, rules, rounds |
| 4 | `rules` | `strata[strata order]` | negation, strict cycle, diagnostics out |
| 5 | `strata` | `kernel[20 kernel relations]` | the kernel board: `ref`/`const`/`var`, `intern`, `edge_ref`, `cons` |
| 6 | `kernel` | `kernel` | comparisons and `int_add`, `term_lt` kind order as a subgraph |
| 7 | `kernel` | `fold[Fold: Partition, Order, Step, Seed]` | aggregates |
| 8 | `dl7` | `colon` | macrotime waves: `<+` to `<-`, the same evaluator |
| 9 | `colon` | `program[reified program JSON]` | modules, partial application, comptime rounds |
| 10 | `program` | `effect[(effect Rel App) rows]` | served relations, loading |
| 11 | `effect` | `answers[answer rows]` | executor roster, cadence |
| 12 | `answers` | `store[(SQLite: pending, settled)]` | `--db`, continuation |
| 13 | `program` | `ddl[sqlite_ivm DDL]` | emitter |
| 14 | `dl7` | `diag[diagnostics JSON, exit 1]` | check cases |
| 15 | `store` | `demo[org.dl7 / extract / ghcacher]` | demos |
| 16 | `store`, `effect` | `nb[not built]` | retraction, pre, latest as dashed nodes |

Node ids in the table are the ids you use, verbatim, so `check_chain.sh` can match them. Chapters 8, 13, 14 branch from an earlier exit; that is allowed, the table says which.

## 6. Diagram laws
- `flowchart LR`. One diagram per chapter. 24 shapes max, count before writing. A shape holds one line: name and role in under 10 words.
- A diagram must carry information a list could not: at least one fork or join. Three boxes in a row is a list; delete it and leave the chapter without a diagram, and say so in the PR.
- Edge labels are verbs or relation names from the tree (`dl8 compile`, `intern`, `--serve`), never prose.
- Every node label names something with a `path:line` in that chapter's What or What proves it table. No node is invented for the diagram.
- Dashed nodes (`-.->`) only in chapter 16, for the not-built rows.

## 7. `check_chain.sh` and the tests
`v8/book/check_chain.sh`: for each chapter N from 1 to 16, extract the mermaid block, list node ids declared with a label (`id[...]`, `id([...])`, `id[(...)]`), and assert the entry ids of the section-5 table appear in chapter N's block AND in its named predecessor's block with identical labels. Exit nonzero naming the chapter and id on the first miss.

Mermaid syntax: validate each block by rendering it. `npx -y @mermaid-js/mermaid-cli -i <block.mmd> -o /tmp/x.svg` (cap 60 s per block; if npx cannot fetch offline, say so in the PR and skip the render, the chain check still runs). `hafley-rxjs/packages/mmd` is Chris's own mermaid package; do not modify it.

`tests/_22_book.rs`: add `every_chapter_has_one_mermaid_block` (count fenced `mermaid` blocks per chapter == 1, `0_why.md` included) and `diagram_chain_is_continuous` (runs `check_chain.sh`, asserts exit 0). 10 s cap each. Follow the file's existing style.

## 8. Style laws
No em dashes. Banned words in labels, prose, identifiers: provenance, substrate, load-bearing, regime, ground truth, refusal, support (say refCount), honest, grounded, distill, "here is", "below is", "the following". Vocabulary rxjs, prolog, SQL only. Comment budget in scripts and tests: constraints only.

## 9. Validation
```bash
cd v8 && bash book/check_chain.sh && bash book/check_blocks.sh
cargo test --locked --offline --test _22_book
mdbook build book && ls book/book/index.html
git diff --stat 256a140edd2da7a1c71e4443be56ef679a3c8763...HEAD    # section 3 files only
grep -c '```mermaid' book/src/*.md                                  # 1 per file
```
Batteries in the background, 10 s per-test cap; over the cap is reported by name, never waited out.

## 10. Reporting
PR title `docs(v8): one chained mermaid per book chapter`, base `docs/v8-book`. Body: the section-5 table with a "shapes" column filled from the blocks you wrote, the validation output, and the list of chapters left without a diagram and why. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "book diagrams: PR #<n>, <k>/17 chapters drawn, chain check <pass|fail>, renders <r>/<k>, _22_book <pass>/<total>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
