// The def file: a macro_rules! whose body mints fns. Cross-file M1 shape
// (interned_slice! in the corpus): the invocation file carries none of the
// minted caller names.
macro_rules! mint_helpers {
    ($name:ident) => {
        impl $name {
            pub fn alpha(&self) -> u32 {
                helper_one()
            }
            pub fn beta(&self) -> u32 {
                helper_two()
            }
        }
    };
}
pub(crate) use mint_helpers;

// A second, single-fn macro: the simplest foreign shape.
macro_rules! mint_single {
    ($name:ident) => {
        impl $name {
            pub fn gamma(&self) -> u32 {
                helper_one()
            }
        }
    };
}
pub(crate) use mint_single;
