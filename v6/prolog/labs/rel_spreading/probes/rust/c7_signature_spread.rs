// C7 spread inside a function signature in Rust (the host-decl analog).
// EXPECTED: no variadic-tuple parameter form exists.

type CommonInputs = (String, String);

fn fetch_row(..CommonInputs, endpoint: String) -> (i64, String) {
    (200, endpoint)
}

fn main() {
    let _ = fetch_row(String::from("r"), String::from("abc"), String::from("/stars"));
}
