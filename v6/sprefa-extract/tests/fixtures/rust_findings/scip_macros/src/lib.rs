pub fn helper() -> u32 {
    7
}

macro_rules! pair {
    ($a:expr, $b:expr) => {
        $a + $b
    };
}

pub fn caller() -> u32 {
    pair!(helper(), helper())
}

pub fn direct() -> u32 {
    helper()
}
