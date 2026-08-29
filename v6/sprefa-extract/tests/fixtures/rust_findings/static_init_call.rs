// static_init_call.rs: a call in a static or const initializer sits outside
// every call-plane def, so it has no caller and mints no edge.
//
// EXPECTED, `extract --resolve --family call`:
//   resolved_edge for the `helper()` inside TABLE's initializer
//   resolved_edge for the `helper()` inside the array element expression
// OBSERVED at cec3d5c1d: both sites are minted, neither edge is. The arm bails
// at src/lang/rust.rs:1031 when covering_def finds no enclosing def, which is
// stated on the arm header but leaves a module-level initializer with no
// call-graph representation at all.
//
// Corpus: crates/ide-diagnostics/src/lib.rs:646 (build_lints_map inside a
// LazyLock static), crates/span/src/ast_id.rs:43 (pack_hash_index_and_kind
// inside a const), crates/ide-completion/src/completions/attribute.rs:317
// (prefer_inner inside a static array), 15 sites in the corpus.

fn helper() -> u32 {
    1
}

static TABLE: u32 = helper();

static ROW: [u32; 1] = [helper()];
