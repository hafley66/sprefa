macro_rules! mkfn {
    ($name:ident) => {
        fn $name() { inner_call(); }
    };
}
mkfn! { generated }
fn inner_call() {}
fn main() { generated(); }
