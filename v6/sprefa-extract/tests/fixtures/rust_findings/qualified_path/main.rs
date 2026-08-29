// qualified_path/main.rs: `wrapper::main()` is a call into the sibling module,
// and the resolve arm keys on the trailing segment only, so the same-file `main`
// wins and a false self-edge is minted.
//
// EXPECTED, `extract --resolve --family call main.rs wrapper.rs util/mod.rs util/deep.rs`:
//   resolved_edge caller_name=main callee_path=.../wrapper.rs callee_name=main
// OBSERVED at cec3d5c1d:
//   resolved_edge caller_name=main callee_path=.../main.rs   callee_name=main
// The `site` row already carries callee_path="wrapper::main"; the arm never
// reads it. Owner: RustSource::call_name_match, src/lang/rust.rs:913-928 (the
// same-file leg runs before any path check).
//
// Corpus: crates/rust-analyzer/src/bin/main.rs:30 (`rustc_wrapper::main()`
// resolves to main.rs::main). 294 edges corpus-wide point at a file other than
// the module the call site names.
//
// The four calls in `spread` pin the rest of the rule:
//   util::helper()        -> util/mod.rs::helper  (a `mod.rs` names its dir)
//   util::deep::helper()  -> util/deep.rs::helper (two same-named defs, one
//                            path each, so only the path tells them apart)
//   Widget::build()       -> main.rs::build       (an UPPERCASE qualifier is a
//                            type, not a module; receiver typing is out of
//                            scope, so the name-match leg still answers)
//   other_crate::helper() -> no edge              (no corpus file sits in a
//                            module named `other_crate`)

mod wrapper;
mod util;

fn main() -> u32 {
    wrapper::main()
}

fn helper() -> u32 {
    2
}

struct Widget;

impl Widget {
    fn build() -> u32 {
        3
    }
}

fn spread() -> u32 {
    util::helper() + util::deep::helper() + Widget::build() + other_crate::helper()
}
