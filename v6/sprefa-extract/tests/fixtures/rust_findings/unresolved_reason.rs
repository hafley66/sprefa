// unresolved_reason.rs: the rust arm never emits the `unresolved` record, so a
// call site that resolves to nothing is silent and a caller cannot tell an
// external symbol from an ambiguous name from a missed edge.
//
// The record is in the contract (src/schema.rs:45, family=call, reason + detail)
// and only src/lang/ts.rs:1194 pushes one.
//
// EXPECTED, `extract --family call`: one `unresolved` row per site below that
// this file's universe cannot name, each carrying a reason slug:
//   Vec::new   -> external, no corpus def
//   compute    -> ambiguous, two defs of the name in this file
// OBSERVED at cec3d5c1d: zero unresolved rows over 941 rust files and 138,223
// call sites in the rust-analyzer corpus, of which 89,500 resolve to no edge.
//
// Owner: no rust emitter exists; the ts one is src/lang/ts.rs:1194.
//
// SECOND FACT in this file. The arm header states that an ambiguous name yields
// no row, and that guard is cross-blob only. Both `compute` sites here mint an
// edge and both name the same def, so one of the two is wrong. The site rows
// carry callee_path "first::compute" and "second::compute" and the arm reads
// neither; see qualified_path/main.rs for the cross-file form of the same bug.

mod first {
    pub fn compute() -> u32 {
        1
    }
}

mod second {
    pub fn compute() -> u32 {
        2
    }
}

fn caller() -> Vec<u32> {
    let total = first::compute() + second::compute();
    let _ = total;
    Vec::new()
}
