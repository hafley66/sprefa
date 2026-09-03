# brief: PLAN for extract and soopy bindings in dl7 comptime, shared with the dbsp emitter

Lane: `plan/comptime-bindings`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.
Deliverables: TWO docs, `plans/2026-09-03-comptime-bindings.PLAN.md` (receipts, citations, for the auditor) and `plans/2026-09-03-comptime-bindings.PLAN.visual.human.unga.md` (plain words, diagrams, zero citations, for Chris). A plan without the second doc is undelivered. No code.

## The ask, Chris 2026-09-03

"can we get sprefa extract and soopy bindings in the swi prolog compiler comptime for dl7 and then that is comptime (for importing types yada). i want to get the prolog comptime setup and then we are able to use same api when outputting a dbsp program (for code relations ivm maintenance)".

Read in this order: `CLAUDE.md` (standing laws; build-vs-buy is mandatory; language design is decided with Chris, so every design choice comes back as a fork with options and a recommendation, never as a settled call), `docs/extract-tsi.md` (the type wire), `docs/comptime-bindings-inventory.md` if it has landed (a sibling lane writes it; if absent, do the retrieval yourself and cite), `v7/src/2_comptime/2_compiler.pl`, `v7/src/2_comptime/0c_extract_loader.pl`, `v7/src/3_emit/3_rust_type_region_mainer.pl` (today's extract call, `process_create` at `:68`), `v6/sprefa-engine-rs/src/executors/mod.rs` (the executor roster engine-rs links, `SoopyCheckoutExecutor` at `:17`), `v6/prolog/compile/6_isolated_compiler_dd.pl` (the `dd_plan` emitter), `docs/ext-dbsp-incremental.md`, `~/projects/hafley-rs/crates/soopy/README.md`, `v6/prolog/conformance/rulings.pl` (grep `executor`, `arrival`, `sh_bind`: the decided executor namespacing and "hosts are arrivals" shape, also `docs/hosts-are-arrivals.md`).

## What the plan must settle (as forks with a recommendation each)

1. Binding mechanism, candidate by candidate, measured where a measurement is cheap: (a) `process_create` + JSONL per call, cached by content id through `1c_compiler_cacher.pl`; (b) swipl `library(ffi)` (check it loads on this machine) over a Rust cdylib that exposes extract and soopy as C functions returning JSONL or a term string; (c) engine-rs embeds swipl (`swipl-rs` or the C API) so comptime runs inside the Rust process; (d) a resident extract daemon spoken to over a socket (`docs/daemon.md` exists for v5). For each: latency per call on `v6/sprefa-extract/tests/fixtures/tsi/rust_probe/src/lib.rs`, build complexity, whether the same call shape serves a dbsp circuit input at runtime.
2. The surface in dl7: a comptime arrival rel per binding (`extract.tsi(File) -> tsi.* rows`, `soopy.files(Glob) -> path rows`, `soopy.read(Path) -> blob rows`), spelled the way executors are already namespaced (rulings `executor_namespacing`, `executor_modules_use_import`); type signatures first, pseudo-code under each, instance lifetimes, storage layout, then reads and writes (the planning protocol in `~/.claude/CLAUDE.md`).
3. Comptime vs runtime: which bindings are comptime only (type import), which are both (file enumeration), and how the dbsp emitter reads the same binding table to print one input handle per binding and soopy `watch --format jsonl` deltas as the change stream. Cite `dd_plan` in `6_isolated_compiler_dd.pl` as the existing emitter shape to extend, not replace.
4. Caching and identity: content-id keyed results (`src/shape.rs` `content_id_of` in extract; soopy content identities), so a comptime import of an unchanged file costs one hash.
5. Sequencing: the smallest first arc that lands a comptime `extract.tsi` binding behind the existing loader with a fixture and a v7 test, then the soopy pair, then the dbsp emitter reading the table. Each arc as an ARCH-row-shaped line (name, deps, receipt).

## Receipts

- Every claim in PLAN.md cites file:line. The visual doc has none.
- A measured latency table for candidate (a) at minimum (three runs), profile stated (debug/release).
- `swipl -g "use_module(library(ffi))" -t halt` output pasted.
- The two docs open with a TOC; diagrams are mermaid.

## Ownership

Owned: the two plan docs. Forbidden: everything else (three implementation lanes are live under `v6/sprefa-extract/src/lang/` and `v7/`).

## Style laws

No em dashes. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground truth". Language vocabulary: rxjs, prolog, SQL words only; "support" is banned. No one-line library dismissals.

## Done

Push, PR against `main`, then:
`boop beep --no-wait --as plan-comptime-bindings sprefa-coordinator "comptime PLAN PR #<n>: forks <n>, recommended mechanism <a|b|c|d>, first arc <name>"`.
