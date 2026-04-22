# sprefa

Read `v2/README.md` first. It covers the system diagram, .sprf file anatomy, op registry, cursor lifecycle, content contract, and walker DSL.

## Project structure

- `v2/` -- active development. Pipeline engine, LSP, all current work.
- `crates/` -- v1 system. SQLite-backed extraction, watcher, CLI. Still functional but not where new features land.
- `chat_log/` -- session logs. Numbered by date.session. Reference for design decisions.
- `.claude/skills/` -- project-local skills covering v2 architecture (op trait family, cursor slots, pipeline tree, cursor_ref, content contract).

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
