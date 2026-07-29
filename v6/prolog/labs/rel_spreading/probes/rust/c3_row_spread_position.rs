// C3 row spread in a call/head position: can a Rust tuple be spliced into an
// argument list with trailing explicit arguments?

fn takes_three(_id: i64, _name: &str, _extra: i64) {}

fn main() {
    let a: (i64, &str) = (1, "n");

    // (1) splice the tuple then append one argument: no such syntax
    takes_three(..a, 5);

    // (2) the tuple itself is one value, not three slots
    takes_three(a, 5);

    // (3) the only spread-shaped thing is `..` in a PATTERN, which discards
    // rather than binds
    let (id, ..) = (1i64, "n", 5i64);
    println!("{}", id);
}
