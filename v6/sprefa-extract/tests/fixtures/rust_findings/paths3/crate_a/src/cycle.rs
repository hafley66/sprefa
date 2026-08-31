use alpha as beta;
use beta as alpha;
use alpha::deep;

pub fn cycle_user() -> u32 {
    deep()
}
