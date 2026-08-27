mod a;
mod other;
mod util;

use crate::a::f;

pub fn call() -> u32 {
    f() + a::g() + other::reach() + util::size()
}
