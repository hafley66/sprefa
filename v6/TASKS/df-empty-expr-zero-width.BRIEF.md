# df-empty-expr-zero-width (issue: df-empty-expr-zero-width, size:small)

FIRST ACTION: `git merge --ff-only e23893b2ef8d3e4c5f60f0a98f015b95dea23128`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root.

GOAL: empty Rust statements (bare `;`) stop minting zero-width (0,0) df expr
nodes. Full RCA in issues/df-empty-expr-zero-width/item.md.

MECHANISM (already traced, do not re-derive): syn parses a bare `;` as
`Stmt::Expr(Expr::Verbatim(<empty TokenStream>), Some(semi))`. The block walker
at v6/sprefa-extract/src/lang/rust.rs:1261 (`syn::Stmt::Expr(expr, semi)` arm)
passes it to flow_expr; the `_ =>` fallback arm at rust.rs:1958 mints
DfNodeKind::Expr with the expr's span, and an empty TokenStream's span resolves
to (0,0). Every empty statement in a file collapses to the same (0,0,'expr')
key: invariant 2 (zero-width) + invariant 3 (dup key) violations. No edges
reference these nodes (invariant 4 was clean over the corpus), so suppression
loses no flow.

THE FIX (decided, implement exactly this): in the `syn::Stmt::Expr(expr, semi)`
arm at rust.rs:1261, skip the statement when
`matches!(expr, syn::Expr::Verbatim(tokens) if tokens.is_empty())` — no
flow_expr call, no node, no tail candidate (a bare `;` carries `Some(semi)` and
can never be the tail expression). Do NOT touch the `_ =>` arm at rust.rs:1958:
non-empty Verbatim (unparsable exprs, macros) has a real span and keeps its
conservative node.

TEST (fail-first, prove it): add a test to v6/sprefa-extract/tests/12_df_identity.rs
following that file's existing style, source
`fn foo(){ ;;;some_expr();;;;{;;;};;;;Ok(()) }` (the smallest offender from
rust-analyzer, per the issue: 12 copies of (0,0,'expr') pre-fix). Assert zero
zero-width df nodes and no duplicate (span,kind) keys. Run it BEFORE the fix
and paste the failing output, then after with it passing.

FILES YOU OWN: v6/sprefa-extract/src/lang/rust.rs (ONLY the Stmt::Expr arm),
v6/sprefa-extract/tests/12_df_identity.rs (additive).
FORBIDDEN: every other file. Especially src/types.rs, other lang/ extractors,
scripts/scale-invariants.sh (run it, never edit it).

VALIDATION (all three, paste outputs):
1. `cd v6/sprefa-extract && cargo test` green.
2. Shallow-clone tokio and rust-analyzer into a scratch dir OUTSIDE the repo
   (`git clone --depth 1 https://github.com/tokio-rs/tokio` and
   `https://github.com/rust-lang/rust-analyzer`), then
   `cargo build --release --features cli --bin extract` and
   `bash scripts/scale-invariants.sh <release binary> <tokio> <rust-analyzer>`.
   Known pre-fix tally: tokio 0,1,0,0,0; rust-analyzer 0,26,3,0,0. Required
   post-fix: all zeros, RESULT: PASS. Any NEW nonzero invariant = STOP AND
   REPORT, do not chase it.
3. Run the sweep leg twice; same numbers both runs.

COMMIT plain. Close: `issuectl --json close df-empty-expr-zero-width --commit <sha>:<summary>`.
Report: fail-first test output, both sweep tallies, files touched.
