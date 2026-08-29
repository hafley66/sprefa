//! module plane fixture: the other glob source; `conflict` collides with
//! `glob_one::conflict` (the ambiguity case, no shadowing local def).
pub fn glob_b_fn() -> u32 {
    20
}

pub fn conflict() -> u32 {
    2
}
