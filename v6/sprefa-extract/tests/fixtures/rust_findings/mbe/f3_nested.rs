macro_rules! inner { ($e:expr) => { $e } }
macro_rules! outer { ($e:expr) => { inner!($e) } }
fn go() {
    outer!(leaf());
}
fn leaf() {}
