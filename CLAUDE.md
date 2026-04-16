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
