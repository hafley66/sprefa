//! The call in order: `2_compiler.pl:75-700`. `lib.rs::compile` is the chain;
//! every file here is one link of it.

#[path = "_0_read.rs"]
pub mod _0_read;
#[path = "_1_unit.rs"]
pub mod _1_unit;
#[path = "_2_macro.rs"]
pub mod _2_macro;
#[path = "_3_units.rs"]
pub mod _3_units;
#[path = "_4_project.rs"]
pub mod _4_project;

use crate::_1_macrotime::Wave;
use crate::_4_comptime::Round;

/// Where the pipe stops without an answer: v7 `throw`s or fails outright.
#[derive(Debug)]
pub enum Stop {
    Read(crate::_0_read::expand::Stop),
    Lower(crate::_2_lower::Stop),
    Check(crate::_3_check::Stop),
    Load(String),
    Io(String),
}

impl From<crate::_0_read::expand::Stop> for Stop {
    fn from(e: crate::_0_read::expand::Stop) -> Self {
        Stop::Read(e)
    }
}

impl From<crate::_2_lower::Stop> for Stop {
    fn from(e: crate::_2_lower::Stop) -> Self {
        Stop::Lower(e)
    }
}

impl From<crate::_3_check::Stop> for Stop {
    fn from(e: crate::_3_check::Stop) -> Self {
        Stop::Check(e)
    }
}

/// The one sink: `--trace` prints each of these to stderr.
#[derive(Debug)]
pub enum Event {
    Wave(Wave),
    Round(Round),
}
