# Brief: two book parts, "Modules and the prelude" and "Hosting", plus the namespacing inspection

## 1. Job
Write two new parts of the dl8 book under `v8/book/src/`, each a directory of pages, and one inspection doc. Every page follows the Docs law in section 8, verbatim. Nothing is invented: every claim is a `path:line`, a fixture, or a command's pasted output run through the real `dl8` binary. Chris asked five questions; the pages answer them in the order they depend on each other, not the order asked:

1. how do modules work
2. who hosts `fetch_json` (and every other served relation)
3. how easy is it to add `boop` as a hosted relation, namespaced
4. how easy is it to make `soopy` continuous with its file and git watchers
5. how ghcacher gets implemented on dl8

## 2. Base and first action
- Base sha: `256a140edd2da7a1c71e4443be56ef679a3c8763` (`origin/docs/v8-book`, PR #767). Branch `docs/v8-hosting-modules`. PR base `docs/v8-book`.
- FIRST command: `git merge --ff-only 256a140edd2da7a1c71e4443be56ef679a3c8763`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Commands run from `v8/`. `cargo build --release` first; `./target/release/dl8` is the probe binary.
- Sibling link `hafley-rs` at the worktree root (boop-start makes it; else `ln -s /Users/chrishafley/projects/hafley-rs hafley-rs`). `soopy`, `boop-store`, `sprefa-extract` are read there.

## 3. Ownership
You own, all new: `v8/book/src/modules/*.md`, `v8/book/src/hosting/*.md`, `v8/book/src/probes/*.dl7` (probe programs the pages cite; each compiled by the test), `plans/v8/2026-09-15-v8-namespacing.inspection.md`, and two appended lines in `v8/book/src/SUMMARY.md` (a `# Modules and the prelude` part and a `# Hosting` part, after the existing list). `v8/tests/_22_book.rs`: add one test `probes_compile_as_their_page_says` (each `probes/*.dl7` opens with `; expect: rc=0` or `; expect: <diagnostic functor>`; run `dl8 compile`, assert). Forbidden: every existing chapter file `0_why.md` .. `16_not_built.md` (another lane owns them), `v8/src/**`, `v8/fixtures/**`, `v8/oracle/**`, `v8/prelude/**`, `book.toml`, `hafley-rs/**`. Never spawn subagents. Never `--no-verify`.

## 4. Where the truth is
| for | read |
|---|---|
| a module is a file; project load order; the `module` term | `src/_4_comptime/_0_load/_2_project.rs` (`:112-141` diagnostics, `:267-272` the term), `oracle/compile/sources/test/fixtures/modules/{0_accounts,1_consumer,2_plus_consumer}.dl7`, `oracle/compile/cases/project-modules-*.json` |
| cross-module reference is a graph goal `(: module Name ?Type ?Index)` | `modules/1_consumer.dl7:5`; grep `src/_2_lower/_1_slots.rs`, `_4_promote.rs`, `_8_express.rs` for how a bare name in a goal resolves; `oracle/check/cases/0_unresolved_name.dl7` |
| lexical binding, nearest wins, shadowing | `oracle/compile/sources/test/fixtures/lexical_binding/*.dl7` (7 files), `binding_symmetry/*.dl7` (12 files) |
| the kernel: 20 flat names | `src/_3_check/_5_kernel.rs:16-60`; `src/_2_lower/_9_kernel.rs:19-65` (arity, `return` positions, keys) |
| the `return` column makes a relation an expression | `src/_2_lower/_2_declare.rs:278-303`; probe: a product named `Wrap` with `(: return type)` and an intern rule compiles as a compound-label target (Chris's probe, reproduce it as `probes/0_return_column.dl7`) |
| the prelude, six files | `prelude/0_constructors.dl7` .. `5_tsi_primitives.dl7` (1099 lines); `macrotime/0_standard.dl7` (`<+` at `:101-102`) |
| a `:` form is never an expression | `src/_2_lower/_8_express.rs:232` `expression_without_return`; probe `(: Holder (* (: (: a 5) text)))` |
| hosted relations are bare user declarations matched by string | `src/_9_runtime/_3_executors/mod.rs:46-100` `executors_for`, the `_ => Extract` arm at `:94`; `soopy_refs::RELATION` etc.; probe: a program declaring its own `(: timer ...)` compiles rc=0 |
| the seam every executor implements | `src/_9_runtime/_2_reconcile.rs:13-32` `Cadence`, `IExecutor` |
| the six executors | `src/_9_runtime/_3_executors/{timer,fetch_json,soopy_refs,soopy_history,repo_at,extract}.rs` (1080 lines total); `fetch_json.rs:21-38` is `ureq`, in process; `soopy_refs.rs:34` holds `soopy::RepositoryWatcher` |
| the roster table, cadence, error relations | `v8/README.md` "Executor roster" |
| soopy's watchers | `hafley-rs/crates/soopy/src/_8_watch.rs:120` `RepositoryWatcher`, `lib.rs:50` exports `DirectoryWatcher`, `SourceWatcher`, `git_dirs`; say which of the three any executor uses today (grep `src/_9_runtime`) and which are unexposed |
| out-of-process executor costs | PR #765 (dylib lab, `plans/v8/2026-09-15-v8-dylib-executor-lab.brief.md`), PR #766 doc `plans/v8/2026-09-15-v8-plugin-abi-cost.md` (call p99 ns, cold build s per shape) |
| boop's store | `hafley-rs/crates/boop-store/src/lib.rs:1-67`, SQLite at `~/.agent/boop.db`; `boop db "<sql>"` |
| ghcacher's contract | `v6/tsv2/goldens/ghcacher_tick_golden/README.md`, `1_schedule.json`, `0_ghcacher_clock_golden.dl6`; `ghcacher_304_golden`, `ghcacher_checkout_golden`; `plans/v8/2026-09-14-v8-reconciler.brief.md:37`; `hafley-rs/crates/ghcache` (the Rust client that exists today) |
| doc comments as rows, for the citation rail | `hafley-rs/crates/sprefa-extract/src/lang/rust_docs.rs:75` `push_doc`, `src/types.rs:357-364` `DocFact`; `plans/v8/2026-09-15-extract-cst-astgrep-survey.md` |
| what is not built | `plans/v8/2026-09-13-v8-tour.md` section 10, existing `16_not_built.md` |

## 5. Pages, in reading order (each file opens with a one-line TOC of its own sections)
```
modules/0_a_module_is_a_file.md      one file, one module; --project; load order; the module term; worked example: accounts + consumer, pasted compile output
modules/1_names.md                   how a bare name in a goal resolves: the file, the project, the prelude, the kernel; lexical binding; shadowing probes with pasted diagnostics; the 20 kernel names as a table with arity and return position
modules/2_the_return_column.md       a product with a column named return is callable; Option, Key, Partial are ordinary relations; Chris's Wrap probe; why a : form is never a label
modules/3_the_prelude.md             the six files, what each declares, in dependency order; one example per file; macrotime and <+
modules/4_namespacing.md             one page, tables only, summarizing plans/v8/2026-09-15-v8-namespacing.inspection.md: where kernel names, prelude names, hosted names and user names live today (all one flat space), the collision probes, the forks (spellings) with what each touches. No decision. Ends with "decision: Chris".
hosting/0_the_seam.md                IExecutor, Cadence, the effect row, the error relation, the one match arm; one sequence diagram (mermaid) of one Once request from rule to row
hosting/1_who_hosts_what.md          the roster: served name, process (in dl8 via ureq / soopy in process / extract child process), cadence, wake source, error relation; fetch_json answered here
hosting/2_adding_boop.md             worked page: a boop_lane relation over ~/.agent/boop.db; the exact files and arms to add, with line numbers of the analogous lines in repo_at.rs; the cost of in-process vs dylib vs process from #766's table; what "namespaced" would need (link to modules/4). No code lands; the page is the plan.
hosting/3_soopy_continuous.md        RepositoryWatcher, DirectoryWatcher, SourceWatcher: what each fires on; which one soopy_refs uses; what a file-content relation (git or not) needs; retraction as the missing half
hosting/4_ghcacher.md                the v6 golden contract as a tick table; what dl8 has (timer, fetch_json, store); the gap list with the line each gap sits at (etag/headers on fetch_json, 304, retraction, pre/latest); the program sketch as .dl7 with its rx lowering beside it
hosting/5_citations_both_ways.md     not built: the rail that checks every path:line in the book exists in extract rows and every /// # Book tag has a page; as a .dl7 sketch over DocFact rows plus its rx lowering; the v3 to v5 lineage in one line with the archive path
```
Each page: intuition first (one worked example, real values, pasted output), then the rule, then a receipts table (claim, path, command). Under 120 lines per page. `.dl7` blocks come from `probes/` via `{{#include ../probes/<file>.dl7}}` and nowhere else.

## 6. The inspection doc
`plans/v8/2026-09-15-v8-namespacing.inspection.md`, tables only, one mermaid at most. Sections: (1) every name space that exists today with the code that reads it, (2) collision probes run through the binary with rc and diagnostic pasted (user product named `intern`; user product named `timer` with and without `--serve timer`; two modules declaring `User`), (3) candidate spellings for a qualified name, one row each, with the files a spelling touches (reader, lowerer, checker, executors_for, prelude) and the fixture that would pin it, (4) what other systems do in one row each (Prolog modules `accounts:user/2`, SQL `schema.table`, rxjs has none, Datalog engines: Soufflé components, CozoDB, Logica), (5) open decisions for Chris, one line each. No recommendation.

## 7. Validation
```bash
cd v8 && cargo test --locked --offline --test _22_book
mdbook build book && ls book/book/index.html
bash book/check_blocks.sh
git diff --stat 256a140edd2da7a1c71e4443be56ef679a3c8763...HEAD    # section 3 files only
grep -rniE 'provenance|substrate|load-bearing|regime|ground truth|refusal|honest|distill|here is|below is|the following|let.s|we ' book/src/modules book/src/hosting | wc -l   # 0
```
Batteries in the background, 10 s per-test cap; over the cap is reported by name, never waited out.

## 8. Docs law (user-set 2026-09-15; copied from `CLAUDE.md`, binding)
- Order is ontological, then topological: sections nest and follow the way the things they describe nest and depend. A name appears only after the page defines it. Siblings are peers of one kind; a subsection is a part of its parent.
- No stray numbers: a number lives only in a table, a step trace, or pasted output, and only when it changes what the reader does. Never in a sentence.
- Every claim carries a `path:line`, a fixture, or a command that prints it. No fixture: write "no fixture; not shown" with the source line. Code beats plan; say so in one line with both paths.
- Textbook register: short sentences, present tense, no chat, no "we", no "let's", no narration of the writer's process, no hedging, no praise. Intuition first, then the rule, then receipts.
- Generated where possible: `{{#include}}` for programs, pasted output labeled with its command. A hand-copied block is a defect.
- Two files for one fact is a defect; cite, do not restate.
- Style: no em dashes; banned words provenance, substrate, load-bearing, regime, ground truth, refusal, support (say refCount), honest, distill; vocabulary rxjs, prolog, SQL only; dl variables descriptive.

## 9. Reporting
PR title `docs(v8): modules, prelude and hosting parts of the book; namespacing inspection`, base `docs/v8-book`. Body: page table (page, lines, probes cited, receipts rows), the validation output, the list of places code contradicted a plan. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "hosting+modules docs: PR #<n>, <k> pages, <p> probes all as expected, _22_book <pass>/<total>, mdbook builds, inspection <lines> lines, forks <f>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
