//! module plane fixture (crate_a): every `use` shape RustModuleIndex
//! resolves. Assertions live in tests/57_rust_module_plane.rs.

mod nested;
#[path = "real_target.rs"]
mod path_mod;
mod hop_source;
mod hop_one;
mod hop_two;
mod glob_one;
mod glob_two;
mod renamed_src;

mod inline_holder {
    pub fn inline_fn() -> u32 {
        1
    }
}

use crate::glob_one::*;
use crate::glob_two::*;
use crate::hop_two::reexported_fn_two;
use inline_holder::inline_fn;
use crate::nested::crate_path_fn;
use crate::path_mod::path_fn;
use crate::renamed_src::original_name as renamed_local;
use crate_b::cross_fn;
use std::collections::HashMap as StdMapUnused;

pub fn root_target() -> u32 {
    42
}

// Explicit item shadows the glob-brought `glob_a_fn`: no ambiguity error,
// unlike two explicit `use`s of the same name.
fn glob_a_fn() -> u32 {
    999
}

pub fn crate_path_caller() -> u32 {
    crate_path_fn()
}

pub fn path_mod_caller() -> u32 {
    path_fn()
}

pub fn hop_two_caller() -> u32 {
    reexported_fn_two()
}

pub fn renamed_caller() -> u32 {
    renamed_local()
}

pub fn inline_caller() -> u32 {
    inline_fn()
}

pub fn cross_crate_caller() -> u32 {
    cross_fn()
}

pub fn shadow_caller() -> u32 {
    glob_a_fn()
}

pub fn glob_ambiguous_caller() -> u32 {
    conflict()
}

pub fn external_caller() -> Option<StdMapUnused<u32, u32>> {
    None
}
