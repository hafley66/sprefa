# go-kotlin-df-spans (issue: go-kotlin-df-spans, size:small)

FIRST ACTION: `git merge --ff-only d0e8340dff067453e08eedbefaacbd6625777b8c`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root.

GOAL: mirror PR #270's rust.rs fix in go.rs and kotlin.rs: df nodes carry full extents, not len 0. Read the landed pattern first: `git log -1 -p --stat` on commit a53aa95f-shaped change to v6/sprefa-extract/src/lang/rust.rs (df_push takes the full span; the existing span helper computes start+end bytes), and the fail-first test shape in v6/sprefa-extract/tests/12_df_identity.rs.

FILES YOU OWN: v6/sprefa-extract/src/lang/go.rs, src/lang/kotlin.rs, a new tests/14_df_identity_go_kotlin.rs, and ONLY the golden fixture files your `cargo test` runs tell you to regenerate (name each in your report). Do NOT commit Cargo.lock churn.
FORBIDDEN: src/lang/rust.rs, src/lang/ts.rs, src/types.rs, src/wire.rs, v6/prolog/**, v6/tsv2/**, v6/dl/**.

STEPS: (1) fail-first: write the per-language test on the ret-covers-tail / same-start shape, run it RED against unmodified code, paste the red output; (2) change each df_push (and its callers if the signature widens) to store real extents; (3) green; (4) full `cargo test` in v6/sprefa-extract — baseline 80 passed / 0 failed, yours adds cases; regenerate only the goldens the diff names and paste which.

VALIDATION: cargo test counts before/after; the red receipt; goldens named. If go/kotlin lack an end-position helper and one must be invented, keep it inside the language file and cite the tree-sitter/parser API you read it from.

COMMIT plain. Close: `issuectl --json close go-kotlin-df-spans --commit <sha>:<summary>`.
