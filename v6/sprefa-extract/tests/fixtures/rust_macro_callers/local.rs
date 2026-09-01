// A file-local def of the SAME macro name: the local body must win over the
// foreign table (its alpha calls helper_two, the foreign one helper_one), and
// no foreign expansion may double-mint the invocation.
macro_rules! mint_helpers {
    ($name:ident) => {
        impl $name {
            pub fn alpha(&self) -> u32 {
                helper_two()
            }
        }
    };
}

pub struct Local;

mint_helpers!(Local);

pub fn helper_two() -> u32 {
    2
}
