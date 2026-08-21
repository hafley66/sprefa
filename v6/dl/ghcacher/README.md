# ghcacher on the Rust door

The six ghcacher goldens, folded by `emit_rust_harness` against scripted
schedules, plus one live conditional-GET smoke. Every host links a Rust
executor; nothing here spawns `sh`.

## Contents

| file | what it is |
|---|---|
| `<golden>.dl6` | the arrival-rel spec (was a byte copy of `v6/tsv2/goldens/<golden>/0_*.dl6`; the tsv2 door is paused and this copy has since diverged on purpose) |
| `<golden>.schedule.json` | the scripted arrival batches the gate feeds it |
| `<golden>.expected.tick.jsonl` | per-tick delta oracle |
| `<golden>.expected.final.jsonl` | final-state oracle |
| `<golden>.adapters.json` | live-executor override, only where one is still needed (executor binding is by rel name now, via registry.pl `arrival_executor/2`, so most goldens carry no file at all) |
| `ghcacher_smoke.dl6` | the live smoke: one endpoint, two clock buckets |
| `gate.sh` | the gate `just ghcacher-rust` runs |

## The two tick-log shapes

The Rust `--final` prints one line per rel in `final_select`, `{"rel", "columns",
"rows"}`, including rels with zero rows. The TS golden's `3_expected.final.jsonl`
is one line, `{"final": {rel: rows}}`, and omits a zero-row rel. `gate.sh`
folds the first shape into the second and drops empty arrays. The TICK log
needs no normalization at all: all six are byte-identical between doors.

`5_expected.statements.jsonl` is a TS-door receipt and does not transfer: the
Rust door folds the clock golden in 353 statements where the TS door takes 698.

## Budget

| leg | budget | command |
|---|---|---|
| the six-golden gate | under 6s once the harness binary is current | `cd v6 && just ghcacher-rust` |
| the live smoke | under 10s, capped at 30s | see below |

`gate.sh` opens with `cargo build --bin emit_rust_harness`, so the first run
after an engine edit pays that build and nothing else does. Read the budget
against a warm run.

```bash
DL_ADAPTERS_DIR=v6/dl/ghcacher RUST_LOG=sprefa_engine_rs=info DL_TRACE_SUMMARY=1 \
  timeout 30 v6/sprefa-engine-rs/target/debug/emit_rust_harness \
  <ghcacher_smoke.rs> v6/dl/ghcacher/ghcacher_smoke.schedule.json \
  --live-hosts --final-only
```

Measured warm, three runs: 2.42s, 3.01s, 1.89s. The cold run that rebuilt the
harness read 17.06s under eight concurrent lanes, and the build is its whole
cost. Smoke: 0.37s, two requests, 5687 body bytes on the 200 and 0 on the 304.

## Executors

Every host below is an arrival rel, `rel <name>(...) -> (...) key(...)`; the
dotted name IS the executor lookup key (`registry.pl arrival_executor/2`,
`hosts.rs executor_for`). No adapter row and no shell template is needed for
any of these.

| host | executor name | file | crate |
|---|---|---|---|
| `/http/fetch` | `http_fetch` | `executors/fetch.rs` | `ureq` |
| `/env/var` | `env` | `executors/env.rs` | std |
| `/gh/repos` | `gh_repos` | `executors/repos.rs` | `ureq` |
| `/soopy/checkout` | `soopy_checkout` | `executors/checkout.rs` | `soopy` |
| `/toml/json` | `toml_json` | `executors/toml.rs` | `basic-toml` |

The gate itself is scripted, so it reads no adapter row; `DL_ADAPTERS_DIR` only
matters for the live smoke below, and even there it is a fallback, never
consulted for a rostered dotted name.

`ghcacher_config_golden` spells its path-existence probe as the `/gh/repos`
host and its config read as `answer` (its own header explains the deviation).
`/gh/repos` IS now a rostered, executor-linked name, but the LIVE `gh_repos`
executor answers a different question (a real GitHub repos lookup) than this
fixture's file-exists probe -- the mismatch is a naming collision, not a
missing binding. `ghcacher_config_golden.adapters.json` stays empty because
this golden never runs `--live-hosts`; `answer` has no linked executor at any
name, and the program is not yet respelled onto `path_exists` /
`read_org_config`, which `compile/registry.pl:472-477` also carries.
