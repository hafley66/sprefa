# Rust emitter: two delivery modes (parked, do not build yet)

User word 2026-08-06, recorded during the dl6 perf arc. Nothing here is
started; this file exists so the requirement is not lost.

## TOC
- The two modes
- What each mode implies
- Where it attaches to what exists today
- Open questions

## The two modes

| mode | shape | the thing being asked for |
|---|---|---|
| standalone | `.dl6` compiles to rust, rust compiles to ONE binary | ship a single artifact that runs the program with no toolchain and no host process |
| dynamic | host process loads a compiled rust artifact at runtime | swap the program without restarting; a program change recompiles one artifact, the host picks it up |

## What each mode implies

| axis | standalone | dynamic |
|---|---|---|
| artifact | executable | `cdylib` / `dylib` (or wasm, undecided) |
| program identity | baked at compile time | resolved at load time |
| ABI | none needed | needs a stable C ABI or an agreed serialization at the boundary |
| rebuild cost | full link per program change | one artifact per program change |
| host | none | needs a loader, symbol lookup, lifetime and unload rules |

## Where it attaches to what exists today

- `v6/prolog/labs/emit_rust_shootout/emit_rust.pl` already lowers the
  reachability program to `mono/src/main.rs`, which builds to a standalone
  binary. That is mode 1 in prototype form, for one hardcoded program.
- The 2026-08-05 finding stands: the dedup structure should be chosen by the
  emitter from a cardinality estimate, not fixed in a runtime
  (`v6/findings/INSIGHTS.md`, the bitmap layout row).
- The 2026-08-05 session already mapped the dynamic-loading candidates
  (interpret / dylib / wasm) and concluded interpret wins for arbitrary user
  code; mode 2 revisits that with the emitter in hand.

## Open questions

- Does dynamic mode load a `dylib` (fast, unsafe, no unload on macOS without
  care) or wasm (safe, slower, sandboxed)?
- Does the standalone binary embed SQLite at all, or is the rust path
  SQLite-free the way `mono` is?
- One artifact per program, or one artifact holding many programs selected by
  name at load?
