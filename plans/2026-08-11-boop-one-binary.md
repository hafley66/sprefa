# boop + sprefa-extract: one binary

## Context

The user wants a single rust-linked binary that drives tmux (`boop`) and runs
the v6 extraction leaf (`sprefa-extract`) in one process, instead of two
installed executables. Both crates are standalone today:

- `v6/boop/Cargo.toml:1-3` declares its own `[workspace]` so `cargo test` stays
  isolated from the v5 root workspace.
- `v6/sprefa-extract/Cargo.toml:1-4` declares its own `[workspace]` for the same
  reason, stated as "prove the v6 extraction leaf with no v5 tree in the build
  graph".

The boop half of this arc was built in `refactor/boop-mux-trait-features`
(trait `Multiplexer` in `v6/boop-mux`, `agent-read` feature in `v6/boop`). This
doc reports the one-binary link only; it does not build it.

## Research facts

### Q1. What breaks if both crates join one workspace

Reproduced in a scratch copy outside the worktree: a parent `[workspace]` with
`members = ["boop", "boop-mux", "sprefa-extract"]` fails with:

```
error: multiple workspace roots found in the same workspace:
  .../ws/boop
  .../ws/boop-mux
  .../ws/sprefa-extract
  .../ws
```

Each nested `[workspace]` table declares itself a workspace root, so the parent
root and the three crate roots collide. Stripping the three nested `[workspace]`
tables makes `cargo metadata` resolve; the only remaining complaint is that
`sprefa-extract`'s `[profile.dist]` is a non-root profile and is ignored until
moved to the workspace root.

### Q2. Is a multicall binary the shape, and what exists

Yes: the repo already ships this shape. `dl` (v5, root `Cargo.toml`) is one
binary with many subcommands, and `boop` itself is one binary dispatching on
`Subcommand` (`beep`, `db`, `config`). A single executable that dispatches on a
subcommand (busybox model) is the established pattern.

Candidate dispatch crates (build-vs-buy, two named):
- `clap` `#[command(multicall = true)]` on the top `Subcommand`: dispatches on
  the first non-flag argument, busybox-style. Already the CLI framework boop
  uses, so no new dependency class.
- `bpaf`: zero-cost derive parser with the same subcommand dispatch; drops in if
  `clap` is rejected, but adds a second CLI framework to the graph for no gain
  here.

Recommendation: extend `clap`, not a new crate. The concurrent arc is "one
binary" not "one argv[0] multiplexer", so `multicall` is optional polish.

### Q3. Dependency collision surface

Direct shared deps, resolved versions in each manifest's `Cargo.lock`:

| crate | clap | serde | serde_json |
|---|---|---|---|
| `boop` | 4.6.6 | 1.0.229 | 1.0.151 |
| `sprefa-extract` | 4.6.4 | 1.0.229 | 1.0.151 |
| merged workspace | 4.6.6 | 1.0.229 | 1.0.151 |

No version disagreement: all three unify to one copy each in a merged lock.
`cargo tree -d` in the merged workspace shows duplicates only inside
`sprefa-extract`'s own graph (`phf`, `proc-macro2`, `quote`, `syn` 2/3 from the
oxc tree), none crossing from `boop`. `boop`'s `anyhow` (1.0.104) is not used by
`extract`; `boop-mux` brings only `anyhow` + `tmux_interface`, which have no
overlap with `extract`.

Dependency feasibility: the collision surface is clean. The workspace-table
conflict, not versions, is the blocker.

## Decisions

- One binary, dispatched by one `clap` `Subcommand` enum; no argv[0]
  symlink multiplexer. Matches `dl` and `boop` today.
- Keep each crate's lib (`boop`, `sprefa_extract`) linkable so the combined
  binary calls `sprefa_extract` functions in-process.
- Move `[profile.dist]` from `sprefa-extract` to the shared workspace root when
  merging.
- Rejected: a separate `extract` process shelled out to. The user's stated goal
  is one rust-linked binary.

## Verification

- Both manifest `Cargo.lock`s collapse to one shared lock, no cross-crate dup
  above the oxc-internal set.
- `./boop extract ...` and `./boop beep lane ...` both dispatch in the one
  binary.
- `cargo build` and `cargo build --no-default-features` green for the merged
  workspace.
- `cargo test` in the merged workspace keeps the v6 counts.

## Staffing

- Implementer: mechanical Rust lane, flash4 model, worktree yes.
- Base SHA: `91c5ea6e`. Same suite budget as this lane (161 boop + 8 boop-mux +
  extract's own).
