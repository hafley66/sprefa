# Payload-safe Rust raw-string delimiters

## Context

`v6/prolog/emit_rust.pl` serialized `ProgramJson` inside a fixed `r#"..."#`
literal. The generated cache fixture
`v6/tsv2/gen_served/ea699faefe33603f03451984a1f13665.dl6` contains
`color="#1d4ed8"` at lines 1157 and 1158. Its emitted JSON therefore contains
a quote followed by a hash, which closes that literal before the JSON body
ends.

The fixture is 107,856 bytes and emits 2,372,903 bytes of Rust before the
correction. A direct `rustc --crate-type=lib` invocation exits 1 and reports
`unknown start of token: \\` at the first color value. Section 2 of
`plans/2026-08-12-rust-dl6-reload.RESEARCH.md` records the same parser failure
in three earlier runs.

`v6/sprefa-engine-rs/grade.sh` previously compiled the emitted
`door-handwritten.dl6` module. The other emitted programs were loaded as text
by `emit_rust_harness`, so their Rust syntax was outside the gate.

## Decisions

| question | selected path | rejected alternative |
|---|---|---|
| Delimiter selection | Scan the JSON payload for quote-plus-hash runs and use one more hash than the longest run. | A fixed hash count can occur in a later payload. |
| Scan implementation | Fold PCRE matches as ranges, retaining only the maximum hash length. | Converting the 2.37 MB atom to a code list allocates one list cell per byte. |
| Gate input | Generate a small scratch fixture containing the measured `color="#1d4ed8"` collision. | The measured `gen_served` cache is ignored and may be absent. |
| Cargo shape | Include the door and delimiter modules under separate Rust modules in the existing compile-check crate. | A second Cargo package duplicates dependency compilation. |

## Verification

| check | measured result |
|---|---|
| Large generated-cache fixture before correction | `rustc` exit 1 at `color=\"#1d4ed8\"` |
| Large generated-cache fixture after correction | delimiter `##`; `rustc` exit 0 |
| `grade.sh`, three final runs | 280 byte-clean each; 9.84 s, 9.80 s, 10.09 s wall |
| New scratch source compilation | probe 9-10 ms; generated adversary 9 ms |
| `just conformance`, three runs | 392 PASS / 0 FAIL each |
| `cargo test --no-fail-fast`, three sequential runs | exit 0 each; 984 passed, 37 ignored |

The 10.09-second warm result crosses the 10-second law. Split the emitted-Rust
syntax check into its own gate if that result repeats after integration, while
keeping `grade.sh` as the byte-clean ratchet. The syntax gate should retain one
Cargo crate containing the door, measured collision probe, and generated
adversary modules.

`dl examples/gen-plans-index.dl --check` exits 2 on eight pre-existing drift
diagnostics in other plan documents. This plan contains no open-item comment
and requires no index row.

## Staffing

Codex implements the lane in
`.boop-worktrees/fix/emit-rust-rawstring-delimiter`, starting from `e70417d9`
and fast-forwarding `origin/feature/emit-rust-climb-3` at `dcc9bb1b`. The warm
`grade.sh` suite budget is 10 seconds. The three requested gate runs are kept
separate so cold and warm measurements remain visible.
