mod a;
mod b;

use crate::a::f;

pub fn call() -> u32 {
    f() + b::legacy::two()
}
