// C1 alternative: Rust's struct update syntax `..base` is the only spread-
// shaped form. It is a VALUE form, and it requires the SAME type on both
// sides, so it cannot widen a row.

#[derive(Debug)]
struct ARow {
    id: i64,
    name: String,
}

#[derive(Debug)]
struct BRow {
    id: i64,
    name: String,
    extra: i64,
}

fn main() {
    let a = ARow { id: 1, name: String::from("n") };

    // same type: accepted
    let a2 = ARow { id: 2, ..a };
    println!("{:?}", a2);

    // widening into a different struct: refused
    let a3 = ARow { id: 3, name: String::from("m") };
    let b = BRow { extra: 7, ..a3 };
    println!("{:?}", b);
}
