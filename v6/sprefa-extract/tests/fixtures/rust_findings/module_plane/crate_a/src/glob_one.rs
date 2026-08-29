//! module plane fixture: a glob source. `glob_a_fn` is unique to this glob
//! (lib.rs also shadows it locally); `conflict` collides with `glob_two`.
pub fn glob_a_fn() -> u32 {
    10
}

pub fn conflict() -> u32 {
    1
}
