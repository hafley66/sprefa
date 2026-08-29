//! module plane fixture: `crate::nested::*` and `super::*` targets.
use super::root_target;

pub fn crate_path_fn() -> u32 {
    3
}

pub fn super_caller() -> u32 {
    root_target()
}
