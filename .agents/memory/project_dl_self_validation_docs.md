---
name: project_dl_self_validation_docs
description: dl_diag self-lint built-in (dl validates dl like rust-analyzer) + op_catalog + dogfood doc generators; agent relations are git-free
metadata: 
  node_type: memory
  type: project
  originSessionId: bd430550-432d-4c86-aa40-8828c5757705
---

Arc landed 2026-06-30 (main, local/uncommitted): make sprefa reusable-with-an-AI
via self-documentation + self-validation. See [[project_v5_dl_engine]],
[[feedback_never_edit_autogen_zones]].

**dl_diag built-in** (`relkind.rs` `DlDiagKind`, registered in `rel_kinds()`):
`dl_diag(path, line, col, end_line, end_col, severity, code, msg)` runs the
engine's OWN lex+parse+typecheck (`check_and_normalize`) over every scanned
`.dl` file and emits one row per diagnostic — same pass as `dl --check`,
relocated into a relation so dl lints dl. Reads the WORK copy from disk
(`eng.root.join(path)`) because `file.content` is a content HASH not text
(engine.rs:3877). Byte span -> 1-based line / 0-based col via `offset_to_line_col`.
Validated FILE-LOCALLY (no `use` include resolution); parse/lex errors are
whole-file (span 0,0). Dogfood found a real bug: `examples/poll-head.dl` uses
`str` (invalid) instead of `text` — still broken, NOT fixed.

**Rail** `examples/lint-dl-self.dl`: `agent_changed(path) <- agent_touch(_,_,path)`
(git-free alias) + `diag(...) <- agent_changed(p), p =~ /\.dl$/, dl_diag(p,...)`.
Lint-on-edit for .dl, `--check`/`--lsp`. Tests: `tests/it/dl_diag.rs` (4, incl.
no-git + reserved-name + --check-exit-2).

**Git-free answer** (the question that kicked this off): `agent_edit`/`agent_touch`
read the harness session store (`~/.claude/projects/<slug>/*.jsonl`,
`opencode.db`) keyed by `--root` DIRECTORY — zero git, file need not be tracked
or committed. `scan`/`file` walk the FS (`ignore::WalkBuilder`). Only
`changed`/`changed_line`/`created` are git (empty outside a repo).

**Docs dogfooded** (extends `examples/gen-reference.dl`, run with root=v5; gen
paths + `scan` globs resolve relative to ROOT): new `op_catalog(op, kind,
syntax, doc)` built-in (`op_docs()` static table in engine.rs, emitted by
`CatalogKind` alongside rel_catalog/fn_catalog) -> `docs/reference/syntax.md`;
`example_doc` scan+match of every `examples/*.dl` first `#` line ->
`docs/reference/examples.md` + a spliced README `<!-- BEGIN: examples-index -->`
block (full corpus, hand-curated highlights table kept above it). All converge.

Adding a builtin rel today = RelKind impl in `relkind.rs` + add to `rel_kinds()`
+ a `builtin_rel_docs()` entry (the skill's old `*_RELS`/`refresh_*_rel`
engine.rs pattern is stale). Reserved-name guard, both tick paths, decls all
wire automatically off `rel_kinds()`.

**Turnkey install (2026-06-30 follow-on):** `v5/install-dl.sh` — `--bin-only`
(cargo install dl), default=bin + wire the umbrella skill into detected agents,
`--project DIR` bootstraps a repo (starter `.dl/dl-self-lint.dl` rail +
AGENTS.md/CLAUDE.md `<!-- BEGIN: sprefa-dl -->` section). Both Claude Code and
opencode read the same skills source (opencode via `opencode.json`
`skills.paths`, Claude Code via the `~/.claude/skills`→plugin symlink). Umbrella
skill = `~/projects/claude-research/skills/sprefa-dl/SKILL.md`.

**Dynamic checked-reference skills:** `examples/gen-skill.dl` — generate a
SKILL page whose code anchors come from `type_entity` (SEMANTIC, resolved decl)
/ `ast` (STRUCTURAL grammar node) / `sg`+`ast_yaml` (ast-grep), NEVER `match`
(match = line/term parsing, not a checked reference). Staleness gate: a
`skill_claims(name)` set + `!resolves(name)` over `type_entity` → a `diag`, so a
skill citing a renamed/deleted symbol fails `--check` (exit 2). Proven on v5.
`ast_yaml` IS a builtin op (ast-grep RuleCore YAML, `inside:`/`has:` relational;
parse.rs:561 → sg.rs:80 `run_ast_yaml`; same span outputs as `sg`).

**type_entity column order (keep getting wrong):** 7 cols =
`type_entity(repo, sym, name, kind, parent, file, line)`. NOT the 6-col
`(sym,name,kind,parent,file,line)` written in CLAUDE.md, and there is no `ty`
col (that was a `TypeEntity` struct field, not the relation). Verify schema via
`rel_catalog(name, _, cols, _)`.

**`dl examples` + embedded corpus (2026-06-30):** `build.rs::embed_corpus`
bakes `examples/*.dl` + `std/**/*.dl` into the binary via `include_str!`
(generated `$OUT_DIR/embedded_corpus.rs`, two arrays). `src/corpus.rs` =
`dl examples` subcommand (intercepted in main like `setup`): bare=list 80,
`<query>`=semantic search (cosine over `embed::make(None)` — the `stub`
token-overlap floor, offline/deterministic; `--features embed-fastembed` swaps a
real ONNX model), `--show NAME`=print body (read/load w/o disk; pipe via
`dl <(dl examples --show X)`), `--std`=list use-able libs. EMBEDDED `use`
FALLBACK: `frontend.rs` Use arm — when `loader.resolve` fails on all disk roots,
`corpus::std_lib(path)` parses the binary-embedded copy (deduped via
`loaded_embedded`). Proven: hid `std/callgraph.dl` on disk, `use "std/callgraph.dl".`
still loaded from binary (3646 call_edge rows). Binary ~96 MB (corpus ~500K).
Reusable-tools mechanism ready; std/ has callgraph.dl + parsers/openapi.dl,
more topic libs (flow, lsp-def) can be factored from examples.

**op-reference "see more" column (2026-06-30):** `gen-reference.dl` syntax.md
op-table gained an `example` column = `dl examples --show <name>` per op. Built
by joining `op_catalog(op,_,_,_)` against op-call heads scanned from the corpus:
`ex_call(op,ex,l) <- scan("v5/examples/*.dl",...), match(.../(?<op>[a-z_][a-z0-9_]*)\(/,l)`,
then `op_use = op_catalog ⋈ ex_call`, `op_example(op, min(ex))` (lexicographic
rep), `op_show(op, split(ex,"/",-1))` for the basename. LEFT JOIN via two gen
rules: with-example (`op_show(op,name)`) + without (`!op_used(op)` → empty cell =
visible drift, e.g. `cmd`/`arith`/`comparison`). Terse row + syntax snippet +
see-more command = skill-reference shape. Self-maintaining (new example lights up
its op next gen). Dynamic regex per-op is impossible (`/${x}/` fails) so scan ALL
heads + join by name equality, not a per-op pattern.

scip perf reality (warm/cold): `rust-analyzer scip` CLI is ~11s EVERY run (no
salsa reuse across launches; cold 11.37 vs "warm" 10.59 on v5, byte-identical
same-state). Warm RA = the LSP SERVER, which emits no SCIP. Two checkouts differ
only by the embedded abs root → OID-shareable modulo path; dl ingest 0.64s ≪ 11s
gen. Harness: `bench/scip_perf.sh` + `bench/scip_perf_results.md`.
