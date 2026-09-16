# refactor/boop-config-one-type: one Config type, load-only, json

## Context (read first)
v6/boop/src/config.rs exists at your base sha? NO — it is uncommitted
work by another live agent in the MAIN tree. Your FIRST action after the
ff-only check: copy the snapshot in, verbatim:
```bash
cp /Users/chrishafley/projects/sprefa/v6/boop/src/config.rs <worktree>/v6/boop/src/config.rs
cp /Users/chrishafley/projects/sprefa/v6/boop/src/main.rs   <worktree>/v6/boop/src/main.rs
cp /Users/chrishafley/projects/sprefa/v6/boop/src/lib.rs    <worktree>/v6/boop/src/lib.rs
```
That snapshot adds `--preset` (config.rs: Config{default_model_preset,
model_presets}, dirs::config_dir()/boop/config.json, kebab-case serde,
load-only). Your base sha ALSO contains the opencode ban in lane.rs
(plan_harness_family + harness_for_spawn bail). Your job unifies them.

## The work (user decree 2026-08-11)
ONE Config type in v6/boop/src/config.rs is the whole runtime config
surface, loaded from `dirs::config_dir()/boop/config.json` (json, NEVER
toml; kebab-case keys). LOAD ONLY: no save/write API of any kind — the
user hand-edits the file. Extend the existing Config with:
- `model-harness`: map of model-name prefix -> harness id. When absent or
  empty, the compiled MODEL_HARNESS table in lane.rs is the fallback.
- `opencode-banned`: map of model-family prefix -> owning harness. When
  absent or empty, the compiled plan_harness_family table is the
  fallback. Ban semantics stay EXACTLY as lane.rs has them: bail on
  opencode for these families, `--harness opencode` included, error text
  naming the owning harness.
lane.rs `harness_for_model` / `harness_for_spawn` / `plan_harness_family`
consult the loaded Config (load once per process, std::sync::OnceLock,
config load failure = loud error not silent default). The compiled
tables move to `impl Default for` the relevant Config fields or stay as
named fallback consts in lane.rs; either way ONE source of truth per
table, no duplicated literals.

## Files you own
- v6/boop/src/config.rs, v6/boop/src/lane.rs
- v6/boop/src/main.rs and lib.rs ONLY as the copied snapshot requires
  (wiring compiles; do not redesign the CLI surface)

## Gate (all green)
```bash
cd <worktree>/v6/boop && cargo test 2>&1 | tail -5   # every suite green
cargo clippy -- -D warnings
cargo build --release
./target/release/boop beep lane create --branch chore/x --brief /tmp/x.md --model openrouter/openai/gpt-5.6-sol --dry-run
```
The last command MUST print the BANNED-from-opencode error (rc nonzero).
Also verify: with a temp config.json whose `opencode-banned` maps
`gpt` -> `codex`, the same refusal fires from config, not fallback.

## Rails
- rc=0 with dirty tree, no commits, or red gates is a DEFECT. Blocked ->
  FAILURE-REPORT-BOOP-CONFIG.md, exact command + output, exit NONZERO.
- NEVER git merge / pull / rebase in the worktree. NEVER --no-verify.
- Up to 3 commits, prefix `boop:`. No push, no PR; coordinator harvests.

## Style
Comment budget: max 2 consecutive comment lines, constraints only. Banned
words, prose and identifiers: provenance, substrate, load-bearing, regime,
refusal. Descriptive names. Follow config.rs's existing serde idiom.
