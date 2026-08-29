// const_block_defs.rs: a fn declared inside a `const _: () = { .. }` block is
// invisible to the call plane. This shape is what salsa's derive expands to and
// it carries real definitions.
//
// EXPECTED, `extract --family call`:
//   node kind=function name=inner
//   node kind=function name=outer
//   site callee=inner
// EXPECTED, `extract --resolve --family call`:
//   resolved_edge caller_name=outer callee_name=inner
//
// OBSERVED at cec3d5c1d: zero node rows, one site row, zero resolved_edge rows.
// The def walker skips a const or static item body; the site walker does not,
// so the site survives with no caller and the edge is dropped.
// Owner: call_defs_in_items, src/lang/rust.rs:1082.
// Corpus: crates/span/src/hygiene.rs:145 (7 sites of `as_salsa_id`).

const _: () = {
    fn inner() -> u32 {
        1
    }

    fn outer() -> u32 {
        inner()
    }
};
