# sprefa

Single line: `v3/` only. v1 and v2 archived to `~/projects/sprefa-archive-20260428/`. Treat that path as out-of-tree historical reference; do not import from it, do not link to chat_log there.

## What sprefa is (post-clearing)

A streaming pattern-flow language. Cursors flow through op chains. Captures are logic vars. Patterns (re/glob/ast) bind captures. Tags are observable relations (subscribe-and-park). Rules are callable parametric pipelines (lazy, pure, memoized). Effects (read/write/list) are pure descriptions batched and cached by the runtime. Writes target a `(repo, rev, fs)` tuple, aggregated per target. Cross-rev writes auto-materialize a worktree.

The point: program LSP actions and incremental analyses across repos, with the language as the configuration.

## Pinned semantics

- `${X}` Read mode (must be already bound). `${X?}` Unbound mode (introducer). Brace-mandatory.
- `>` sequence. `;` fork (or top-level pipe separator).
- Tags by SQL shape:
  - `tag(:r, ${A}, ${B})` no `?` = INSERT
  - `tag(:r, ${A?}, ${B?})` = SELECT * (drain + subscribe)
  - `tag(:r, ${A}, ${B?})` = SELECT B WHERE col0=A (semi-join subscribe)
  - `tag?(:r, ${A}, ${B})` = predicate, fully-bound disambiguator only
- Rules are call-only. No predicate path. Bodied rules error if any param is bound at declaration.
- No snapshotting anywhere. Every read is drain+subscribe.
- Source ops (repo / rev / fs): cursor-field filter or capture-bind today; future: source from cached effect (ListReposEffect / ListRevsEffect mirroring FsListFilesEffect).
- Writes: `(repo, rev, fs)` tuple. `:wt` = working tree. `:HEAD` = working tree + warning diag. branch / sha / tag = auto-worktree under `~/.cache/sprefa/wt/<repo>/<rev>/`, leave dirty + emit `write/orphan-worktree` diag. Aggregate per target before splicing (right-to-left ranges).

## Project structure (current)

- `v3/` -- the engine. tree-sitter host grammar + injected sub-grammars, pipeline crate, effect_runtime crate, server crate (LSP + sprefa-run CLI + sprefa-lsp), tests/smoke/.
- `editors/vscode/` -- VS Code client + `install.sh`.
- `.beads/` -- bd issue tracker state. Committed.
- `.claude/skills/` -- empty placeholder; v2-era skills archived.
- `AGENTS.md` -- bd workflow guidance.
- `justfile` -- `just regen-all` regenerates tree-sitter grammars; `just test` / `just build` run cargo against v3.

## Major v3 files

Read these in order when picking up work:

- `v3/README.md` -- op inventory, fixtures, run/install instructions.
- `v3/crates/sprefa_parse/parse.md` -- language spec.
- `v3/crates/pipeline/src/_0_cursor.rs` -- `Cursor`.
- `v3/crates/pipeline/src/_1_op.rs` -- `Op` trait.
- `v3/crates/pipeline/src/lib.rs` -- `Pipeline`, runner.
- `v3/crates/pipeline/src/effects.rs` -- effects + batchers.
- `v3/crates/pipeline/src/registry.rs` -- op factory map.
- `v3/crates/pipeline/src/ops/` -- per-op directory.
- `v3/crates/effect_runtime/` -- `RtCtx`, `PureEffect`, `Batcher`.
- `v3/crates/tree-sitter-sprefa/grammar.js` -- host grammar.
- `v3/crates/sprefa_parse/src/parse.rs` -- host CST + injected-tree parse.
- `v3/crates/server/src/{backend.rs,session.rs,bin/}` -- LSP + CLI.
- `v3/tests/smoke/` -- end-to-end shell scripts. `_l_*.sh` through `_r_*.sh` are write-architecture DRAFTS that don't pass yet.

## Build and test

```bash
cd v3 && cargo build --tests
cd v3 && cargo test
cd v3 && cargo test --lib                    # fast
just regen-all                                # regen tree-sitter grammars
v3/tests/smoke/_j_write_cursor_replace.sh     # e2e shell tests
```

After modifying the pipeline crate, the long-lived `sprefa-server` daemon at `~/.cache/sprefa/server.json` runs the OLD code. Always:
```bash
pkill -f sprefa-server || true
rm -f ~/.cache/sprefa/server.json
```

## Conventions

- Tests use scaffolded helpers: `OpCtx::for_test`, `RuntimeConfig::test_default`, `Config::test_default`. Add a field to ctx → update the helper. Never hand-roll a 14-field literal in tests.
- Ops own their diagnostics, patterns, hover. No central enum across ops.
- Content contract: byte-reading ops parse `cursor.content[byte_range]` first, fall back to reader only when content is None.
- v3 forward: no numeric file prefixes (`_0_x.rs` is legacy from v2 carry-over and being phased out).

## Beads

`bd ready --json --quiet` → `bd show <id>` → `bd update <id> --claim` → work → `bd close <id> -r "..."`. State lives in `.beads/`, committed. Root epic is `sprefa-4m7`. Cards labeled `spike` need a design session before code.

`bd remember "<insight>"` for persistent knowledge across sessions. No MEMORY.md files at the repo level.

## Author preferences (load-bearing)

- Withhold opinions until asked. Return data, not recommendations.
- No snapshotting paths.
- No prose preambles in tool reports. File paths and line ranges first.
- No "not X / Y" parallelism, no rhetorical closes, no narrative steering.
- Drafts and aspirational tests must say so in big banners.
