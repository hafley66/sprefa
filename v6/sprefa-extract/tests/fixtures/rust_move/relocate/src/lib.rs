mod a;
mod b;
mod util;

use crate::a::f;

pub fn call() -> u32 {
    f() + b::shout()
}
