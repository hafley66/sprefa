macro_rules! twice {
    ($e:expr) => { { $e; $e } };
}
fn work() {
    twice!(helper());
}
fn helper() {}
