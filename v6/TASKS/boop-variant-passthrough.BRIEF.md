# boop: --variant passthrough to opencode lanes

## Problem
`opencode run` supports `--variant <low|medium|high>`; opencode picks a per-model
default when the flag is absent. Measured 2026-08-14: pro4 lane sessions ran
variant=low while flash4 ran high, and boop had no way to say otherwise. The
`model@effort` suffix in presets maps ONLY to codex reasoning effort
(`crates/boop/src/harness/codex.rs:178`); the opencode channel ignores it.

## Task
1. Locate where the opencode spawn command line is assembled (the `opencode run -m
   '<model>' --auto "$(cat ...)"` string; grep `opencode run` under `crates/boop/src/`).
2. Add variant passthrough with BOTH doors:
   - CLI: `boop beep lane create --variant <v>` threads through to `opencode run
     --variant <v>`.
   - Preset: a variant field in the preset config (`~/Library/Application
     Support/boop/config.json` shape; find the preset struct) used when the CLI flag
     is absent. CLI flag wins over preset.
   - Absent both: emit no `--variant` flag at all (preserve opencode's default),
     byte-identical to today's command line.
3. The codex channel is untouched: `--variant` on a codex-harness lane is either
   rejected with a clear error or documented as opencode-only; pick one and say why.
4. Tests: unit tests on the assembled command line for (flag set, preset set, both
   set, neither set). `cargo test -p boop` — 3 pre-existing failures in `lane::tests`
   (`a_gpt_model_names_the_codex_harness`, `an_unnamed_harness_never_guesses_opencode`,
   `plan_family_models_are_banned_from_opencode`) are known-red; any OTHER failure is
   yours.
5. Receipt: `boop beep lane create --dry-run` output showing the `--variant` in the
   `cmd:` line for a preset that sets it, pasted into the final message. Use --dry-run
   only; do not spawn real lanes.

## Ownership
You own `crates/boop/src/**` EXCEPT `crates/boop/src/supervise.rs` and
`crates/boop/src/channel/tui.rs` (both freshly merged, do not touch).
FORBIDDEN: everything outside crates/boop, all other crates.

## Style laws
- No eprintln! in src/**; tracing only (existing `@eprintln-ok` lines stay).
- Comments state only constraints the code cannot show.
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime.

## Deliverable
Commits on this branch. Final message: files touched with line refs, the dry-run
receipt, cargo test counts vs the 3 known-red.
