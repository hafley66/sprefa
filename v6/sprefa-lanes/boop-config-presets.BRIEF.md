# BRIEF: boop config, finish the model-preset surface

## Base
- Branch: `feature/boop-config-presets`.
- Base sha: `aeec4e1f` (head of `refactor/boop-config-one-type`, NOT main).
  Verify with `git log --oneline -1` FIRST. Any other base = STOP AND REPORT.
- Your FIRST action after the worktree exists: `git merge --ff-only aeec4e1f`.
  Failure = STOP AND REPORT. Do not work around it.

## What already exists. Do not rebuild it.
`v6/boop/src/config.rs` on your base already has:

| item | line | behavior |
|---|---|---|
| `Config` struct | 10-19 | `default_model_preset`, `model_presets`, `model_harness`, `opencode_banned` |
| `loaded()` | 26-31 | process-wide, `OnceLock`, loaded exactly once |
| `default_path()` | 38-41 | `<platform config dir>/boop/config.json` |
| `load()` | 43+ | missing file returns `Config::default()`; unreadable or unparseable is a loud error |
| `resolve_model()` | 51-58 | preset name -> model string |
| unit test | 65+ | `parses_named_provider_model_presets` |

`v6/boop/src/main.rs` already wires `--preset` on `lane create` (field at 1530,
resolution at 1548-1552, default fallback at 1573-1581) and documents it at
line 36.

## The four gaps you close. Nothing else.

### 1. `default-model-preset` is opencode-only
`main.rs:1573-1581` reads:
```rust
let model = match (requested_model, harness_id.as_str(), config.default_model_preset.as_deref()) {
    (Some(model), _, _) => Some(model),
    (None, "opencode", Some(preset)) => Some(config::resolve_model(preset, &config_path)?),
    (None, _, _) => None,
};
```
The literal `"opencode"` means every other harness ignores the configured
default. Make the default apply to any harness. Keep the precedence order
exactly: explicit `--model` beats `--preset` beats `default-model-preset`.

### 2. Double resolution
`requested_model` at 1548-1552 already resolves `--preset`. The second match at
1573 resolves again. Collapse to one resolution. Behavior must not change; the
existing tests are your proof.

### 3. `resolve_model` gives a dead-end error
Its `with_context` says the preset "is absent from <path>" and stops. Add the
available preset names to the message, sorted. `model_presets` is a `BTreeMap`,
so iteration order is already sorted; do not sort again.

### 4. No way to see the loaded config
Add one verb, `boop config`, with two subcommands:
- `boop config path` -> prints the resolved config path, nothing else.
- `boop config show` -> prints the loaded `Config` as pretty JSON, including
  the defaults that a missing file produces.

No new query DSL. No flags beyond those two subcommands. Follow the existing
clap structure in `main.rs`; match the file's own style for subcommand
declaration rather than inventing a new shape.

## Files you own
| path | permission |
|---|---|
| `v6/boop/src/config.rs` | full |
| `v6/boop/src/main.rs` | full |
| `v6/boop/src/lane.rs` | only if gap 1 requires it |

Touch nothing else in the repo. Explicitly forbidden: everything under
`v6/prolog/`, `v6/labs/`, `chat_log/`, `plans/`. Two other lanes are live in
those trees.

## Tests. Every gap gets one.
Unit tests go in `config.rs` beside `parses_named_provider_model_presets`,
following that test's exact shape.

| gap | test |
|---|---|
| 1 | default preset resolves for a non-opencode harness id |
| 1 | explicit `--model` still beats `--preset` beats the default |
| 2 | one resolution path; the collapsed match yields the same model as before for all three precedence cases |
| 3 | a missing preset name errors with the available names in the message |
| 4 | `config show` on a missing file prints the default config |

## Gates
```bash
cd v6/boop && cargo build --release
cd v6/boop && cargo test
cd v6/boop && cargo clippy -- -D warnings
cd v6/boop && cargo fmt --check
```
All four green before you report done. If `clippy` or `fmt` was already red on
your base, say so with the verbatim output and do not fix unrelated files.

## Known fatal
- Do NOT change what the flat-rate-plan ban does. `opencode_banned` maps a
  model-family prefix to its owning harness and exists to stop a metered-credit
  spawn. Widening or bypassing it is a defect.
- Do NOT add a config-writing verb. The config is load-only by decision; the
  user edits the JSON.
- Do NOT reinvent config parsing. `serde` + `serde_json` are already in the
  dependency tree and already used by this file.
- A missing config file is NOT an error. It returns `Config::default()`. Keep
  that.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime` in prose
  or identifiers.
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no restating the next line.
- Type names say what the thing is on first reading.
- Follow the existing style of each file you edit, even where it differs from
  anything stated here.

## Deliverable
Commits on `feature/boop-config-presets`, and a final report containing: one
row per gap with the file:line you changed, the verbatim output of all four
gate commands, and the exact text of the new `resolve_model` error message.
