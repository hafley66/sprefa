// macro_body_calls.rs: a call inside a macro invocation's token stream mints no
// site, so it is invisible to the whole call plane.
//
// EXPECTED, `extract --family call`: four site rows with callee=helper, one per
// fn below, and four resolved_edge rows under --resolve.
// OBSERVED at cec3d5c1d: one site row (from `plain`) and one edge. The three
// calls inside format!, assert_eq! and vec! are absent.
//
// syn parses a macro invocation as an opaque token stream and the site walker
// never descends into it. Owner: project_call, src/lang/rust.rs:1215 (the
// expression walk has no syn::Macro arm).
//
// Corpus: 17,184 macro invocations across 941 rust-analyzer src files, led by
// expect! (3,001), format! (1,376), vec! (995), matches! (875), assert_eq!
// (820). Test bodies in this corpus are written almost entirely inside them.

fn helper() -> u32 {
    1
}

fn plain() -> u32 {
    helper()
}

fn in_format() -> String {
    format!("{}", helper())
}

fn in_assert() {
    assert_eq!(helper(), 1);
}

fn in_vec() -> Vec<u32> {
    vec![helper()]
}
