// rust_modules/sample.rs: one line per row of the module-specifier mapping.
// ASCII-only so syn's char column equals the byte column (spans stay clean).

use alpha::beta;
use alpha::gamma as gee;
use alpha::{delta, epsilon};
use alpha::zeta::{self};
// rustc rejects a bare `self` leaf outside a brace group; syn parses it, and
// the mapping has to state a row for it either way.
use alpha::eta::self;
use theta::*;
pub use iota::kappa;
pub(crate) use lambda::mu;
pub use nu::*;

extern crate xi;

mod omicron;
pub mod pi;

#[path = "rho.rs"]
mod sigma;

mod tau {
    use upsilon::phi;
}

pub fn chi(count: u32) -> u32 {
    count
}
