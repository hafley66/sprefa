# extract-scale-corpus (issue: extract-scale-corpus, size:small)

FIRST ACTION: `git merge --ff-only d0e8340dff067453e08eedbefaacbd6625777b8c`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root.

GOAL: extractor invariants asserted over an external-scale corpus. Clone (shallow) two large Rust repos into a scratch dir OUTSIDE the repo (e.g. tokio, rust-analyzer), run the release extractor (`cargo build --release --features cli --bin extract` in v6/sprefa-extract, then the binary) over every matching source file, and assert per-file invariants on the JSONL wire output: (1) every span start <= end and end <= file byte length; (2) zero zero-width df nodes; (3) no two df node facts share (span, kind); (4) every df edge endpoint (span+kind) resolves to an emitted node fact; (5) process exits 0 on every file. Tally violations per invariant per repo.

FILES YOU OWN: a new test-or-script `v6/sprefa-extract/tests/13_scale_invariants.rs` behind `#[ignore]` (so CI stays fast) OR `v6/sprefa-extract/scripts/scale-invariants.sh` — your pick, say why. Nothing else. Do NOT commit Cargo.lock churn. Do NOT regenerate goldens.
FORBIDDEN: src/** of any crate, v6/prolog/**, v6/tsv2/**.

VALIDATION: run it over both repos, paste the tally table (files scanned, facts, violations per invariant). Known state: rust df spans were just fixed (PR #270), go/kotlin are known len-0 (issue go-kotlin-df-spans) — if your corpus is Rust-only those should be zero; any nonzero Rust violation is a REAL FINDING: file it with `issuectl --json new`, smallest offending file cited, do not fix src.

COMMIT plain. Close: `issuectl --json close extract-scale-corpus --commit <sha>:<summary>`. Report: corpus sizes, tally table, timing, issues filed.
