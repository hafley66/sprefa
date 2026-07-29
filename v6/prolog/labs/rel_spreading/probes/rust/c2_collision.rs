// C2 collision in Rust: what happens when two sources would contribute the
// same field name?

#[derive(Clone)]
struct ARow {
    shared: i64,
    only_a: i64,
}

#[derive(Clone)]
struct BRow {
    shared: i64,
    only_b: i64,
}

struct Merged {
    shared: i64,
    only_a: i64,
    only_b: i64,
}

fn main() {
    let a = ARow { shared: 1, only_a: 2 };
    let b = BRow { shared: 3, only_b: 4 };

    // two functional updates in one literal: refused
    let m = Merged { ..a, ..b };
    let _ = m.shared;

    // duplicate field name inside one struct declaration: refused
    let _ = Duplicated { shared: 1, shared: 2 };
}

struct Duplicated {
    shared: i64,
    shared: i64,
}
