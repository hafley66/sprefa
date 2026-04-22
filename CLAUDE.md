# sprefa

Read `v3/README.md` first. It covers the current op inventory, the `.sprf` usage surface, CLI/LSP/VS Code setup, and fixtures. Language spec lives at `v3/crates/sprefa_parse/parse.md`.

## Project structure

- `v3/` -- active line. Cursor-pipeline engine on `effect_runtime`, tree-sitter host grammar + injected sub-grammars, LSP + CLI + VS Code extension.
- `v2/` -- previous line. Pipeline engine, LSP. Still functional and referenced during v3 ports; not where new features land.
- `crates/` -- v1 system. SQLite-backed extraction, watcher, CLI. Reference only.
- `chat_log/` -- session logs. Numbered by `date.session`. Source of truth for prior design decisions.
- `editors/vscode/` -- VS Code client + `install.sh` end-to-end packager.
- `.claude/skills/` -- project-local skills covering v2 architecture (op trait family, cursor slots, pipeline tree, cursor_ref, content contract).
- `.beads/` -- bd issue tracker state. Committed. See "Beads in this repo" below.

## Major files (v3)

Read these in order when picking up work:

- `v3/README.md` -- op inventory, fixtures, how to run and install.
- `v3/crates/sprefa_parse/parse.md` -- language spec. §14.5 marks each op's sub-grammar contract. §14.5e-l list what's landed.
- `v3/crates/pipeline/src/_0_cursor.rs` -- `Cursor` (content, byte_range, captures, fs/repo/rev, last_bound).
- `v3/crates/pipeline/src/_1_op.rs` -- `Op` trait (object-safe). Every op's surface lives here or on `pattern_op::PatternOp`.
- `v3/crates/pipeline/src/lib.rs` -- `Pipeline` enum (`Op`/`Seq`/`Fork`), runner.
- `v3/crates/pipeline/src/effects.rs` -- `ReadBytesEffect`, `PrintEffect`, `FsListFilesEffect`, `ensure_content_loaded`.
- `v3/crates/pipeline/src/registry.rs` -- op factory map (string-shape → `Box<dyn Op>`).
- `v3/crates/pipeline/src/ops/` -- one directory or file per op.
- `v3/crates/effect_runtime/` -- `RtCtx`, `PureEffect`, `Batcher`, `RtCtxBuilder`.
- `v3/crates/tree-sitter-sprefa/grammar.js` -- host grammar (pipe, fork, op_invocation, paren_slot).
- `v3/crates/sprefa_parse/src/parse.rs` -- host CST + injected-tree parse.
- `v3/crates/server/src/{backend.rs,session.rs,bin/}` -- tower-lsp `LanguageServer`, `DocSession`, `sprefa-run` CLI, `sprefa-lsp` binary.
- `v3/tests/smoke/` -- end-to-end shell scripts per feature.

## Build and test

```bash
cd v2 && cargo build --tests        # compile
cd v2 && cargo test -p v2            # full suite
cd v2 && cargo test -p v2 --lib      # lib tests only (fast)
cd v2 && cargo test -p v2 --test hover_render   # targeted
```

Known pre-existing failure: `doc_session_completion_fs_filter_glob_double_star` (wildcard-glob completion path, out of scope).

## Key conventions

- Numeric file prefixes indicate dependency/reading order: `_0_types.rs` before `_5_op.rs` before `_8_parse.rs`
- Ops own their diagnostics, patterns, and hover rendering. No central enum across ops.
- `Op::pipe()` streams `BoxStream<Arc<[Cursor]>>`. Cursors carry content, captures, slots, byte_range.
- Content contract: every byte-reading op parses `cursor.content[byte_range]` first, falls back to `reader.bytes()` only when content is None.
- Walker `Leaf` = scalar match. `CaptureAny` = any node kind. Cross-refs keep `Leaf` for constraint matching.

## Test scaffolding — do not inline fixtures

New knobs on `OpCtx` or `RuntimeConfig` must not force a fan-out edit across every test. The scaffold absorbs growth:

- `RuntimeConfig::test_default()` — canonical runtime knobs. One literal.
- `Config::test_default()` — empty config. Override with struct-update: `Config { repos: vec![...], ..Config::test_default() }`.
- `OpCtx::for_test(config, reader, writer)` — full plumbing (no-op diags/events, NoopStore, closed mutation channel, fresh cancel, empty ParseSite). Tests override via struct-update: `OpCtx { diags: my_sink, ..OpCtx::for_test(cfg, reader, writer) }`.

Rules:
- Adding a field to `OpCtx` or `RuntimeConfig` means updating `for_test` / `test_default`. Never patch test-sites. If the compiler points at a test file, the helper is where the fix goes.
- Do not hand-write `RuntimeConfig { ... }` or the full 14-field `OpCtx { ... }` literal in a test. If the existing helper does not fit, extend the helper.
- `cargo fix --tests --allow-dirty` auto-removes imports that go stale when a literal collapses.


## Beads in this repo

All open work is tracked under root epic `sprefa-4m7` (sprefa v3 — active line). bd state lives in `.beads/` and is committed. A full walkthrough of the tracker layout lives in `memory/reference_beads_tracker.md` in the per-project Claude memory; the essentials are here.

### Epic tree

| ID | Epic |
|---|---|
| `sprefa-4m7.11` | Invariant compliance — ops own everything (§2, §14.1) -- P0 |
| `sprefa-4m7.1`  | Host grammar + injected trees (§13, §14.5a) |
| `sprefa-4m7.2`  | Op stdlib (§14.5c-§14.5l + future ops) |
| `sprefa-4m7.3`  | Pattern sub-grammars -- ops own grammar (§14) |
| `sprefa-4m7.4`  | Binding & resolution (§6, §7, §19) |
| `sprefa-4m7.5`  | Rules -- parametric + arg-mode (§17, §18) |
| `sprefa-4m7.6`  | Control flow -- fork, scan-pointers, temporal ops (§11, §12, §20) |
| `sprefa-4m7.7`  | Relations tier + mutation effects + render spikes (§24, §25) |
| `sprefa-4m7.8`  | LSP tooling + VS Code (§14.5l, §14.6) |
| `sprefa-4m7.9`  | CLI + server + config discovery (§14.5d, §14.5g) |
| `sprefa-4m7.10` | Fixtures + smoke + kitchen sink |
| `sprefa-4m7.12` | Diagnostics + hole mechanic (§14.7, §14.8, §26) |
| `sprefa-4m7.13` | Cross-repo analysis -- entity hinting, type-check lite, norm joins |

### Label taxonomy (4 axes, compose freely)

- **Crate**: `crate/pipeline`, `crate/server`, `crate/sprefa-parse`, `crate/tree-sitter-sprefa`, `crate/effect-runtime`, `crate/macros`
- **Arch layer**: `core`, `ops`, `lowering`, `grammar`, `runtime`, `lsp`, `cli`
- **Topic**: `semantics`, `syntax`, `binding`, `effects`, `perf`, `invariant`, `cross-repo`, `safe`, `unsafe`
- **Work type**: `spike` (design session with user required before coding), `future` (long horizon), `port-v2` (straight port from v2 to v3), `close-the-loop`, `test`

Query by label: `bd list --label spike`, `bd list --label cross-repo --label spike`, `bd list --label core`, etc.

### Card anatomy

Every P0/P1 card has reading material appended as NOTES. Expect:
- parse.md §X.Y with concrete line ranges
- Relevant `memory/*.md` pointers
- Relevant code paths (crates, files, functions)
- Dep notes when blocked

Read via `bd show <id>` before picking up work.

### Working protocol (single or multi-agent)

1. `bd ready --json --quiet` -- find unblocked work
2. `bd show <id>` -- read description + NOTES (reading material)
3. `bd update <id> --claim --status in_progress --json --quiet` -- claim FIRST
4. Do the work using the code paths listed in the notes
5. `bd close <id> -r "<one-line summary>"` -- close with reason
6. Commit `.beads/` alongside the code changes

### Multi-agent rules

- Never skip `--claim`. Another agent may pick the same card.
- Before claiming, check `bd list --status in_progress` to see what other agents hold.
- If a card's code paths overlap with an in-progress card (reading material lists the paths), pick a different card.
- Abandoning work: `bd update <id> --status open --assignee ""` to release.
- Fix stale cards in place (`bd update <id> --description "..."`) so the next agent benefits.

### Spikes are design sessions

Cards labeled `spike` require a design session with the user before code lands. Do not start implementation on a `spike` card without first confirming direction. Open questions and the "what we'd discuss" lists are already in the card description.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->
