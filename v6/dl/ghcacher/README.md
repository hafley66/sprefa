# ghcacher on the Rust door

The six ghcacher goldens, folded by `emit_rust_harness` against scripted
schedules, plus one live conditional-GET smoke. Every host links a Rust
executor; nothing here spawns `sh`.

## Contents

| file | what it is |
|---|---|
| `<golden>.dl6` | byte copy of `v6/tsv2/goldens/<golden>/0_*.dl6`, the spec |
| `<golden>.schedule.json` | byte copy of that golden's `1_schedule.json` |
| `<golden>.expected.tick.jsonl` | byte copy of `2_expected.tick.jsonl` |
| `<golden>.expected.final.jsonl` | byte copy of `3_expected.final.jsonl` |
| `<golden>.adapters.json` | which linked executor answers each host, live |
| `ghcacher_smoke.dl6` | the live smoke: one endpoint, two clock buckets |
| `gate.sh` | the gate `just ghcacher-rust` runs |

`gate.sh` diffs each `.dl6` against the tsv2 original, so a drifted copy is a
failure rather than a silent pass.

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

| host | executor name | file | crate |
|---|---|---|---|
| `fetch`, `gh_rest_cond` | `http_fetch` | `executors/fetch.rs` | `ureq` |
| `env_var` | `env` | `executors/env.rs` | std |
| `repos`, `gh_repos` | `gh_repos` | `executors/repos.rs` | `ureq` |
| `repo_checkout` | `soopy_checkout` | `executors/checkout.rs` | `soopy` |
| `toml_json` | `toml_json` | `executors/toml.rs` | `basic-toml` |

The gate itself is scripted, so it reads no adapter row; the sidecars are the
live wiring, read through `DL_ADAPTERS_DIR`.

`ghcacher_config_golden.adapters.json` is EMPTY on purpose. That golden spells
its path-existence probe as the `repos` host and its config read as `answer`
(its own header explains the deviation), and neither question is what the live
`gh_repos` executor answers. Live, those two hosts are a named stop until the
program is respelled onto the registered `path_exists` / `read_org_config`
names, which `compile/registry.pl:425-431` already carries.
