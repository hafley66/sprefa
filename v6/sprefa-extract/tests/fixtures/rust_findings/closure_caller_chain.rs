// closure_caller_chain.rs: a call inside a closure is attributed to the closure,
// which nothing ever names as a callee, so the caller chain stops there.
//
// EXPECTED, `extract --resolve --family call`: a path from `entry` to `worker`
// exists in the resolved_edge relation, by whatever spelling keeps the chain
// walkable (an edge entry -> worker, or an edge naming the closure as a callee
// so the two hops join).
// OBSERVED at cec3d5c1d:
//   resolved_edge caller_name=entry        callee_name=spawn
//   resolved_edge caller_name=closure@<n>  callee_name=worker
// No row has callee_name=closure@<n>, so a walk from `entry` never reaches
// `worker`. Owner: caller_name, src/project.rs:1012 (a nameless def becomes
// closure@<span.start>) reached through covering_def at src/lang/rust.rs:1031.
//
// Corpus: crates/rust-analyzer/src/bin/main.rs:68 (`move || run_server(None)`),
// which is why the whole LSP server spine is unreachable from `fn main`.
// 6,973 of 48,723 corpus edges have a closure caller and 934 callees are
// reachable only through one.

fn worker() -> u32 {
    1
}

fn spawn<F: Fn() -> u32>(f: F) -> u32 {
    f()
}

fn entry() -> u32 {
    spawn(|| worker())
}
