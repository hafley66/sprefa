// C1 decl spread in Rust: is there a compile-time FIELD splice in a struct
// declaration? EXPECTED: no such syntax exists.

struct ARow {
    id: i64,
    name: String,
}

struct BRow {
    ..ARow,
    extra: i64,
}

fn main() {
    let _ = BRow { id: 1, name: String::new(), extra: 7 };
    let _ = ARow { id: 1, name: String::new() };
}
