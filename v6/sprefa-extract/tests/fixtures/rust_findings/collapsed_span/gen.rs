macro_rules! define_pair {
    ($first:ident, $second:ident) => {
        pub fn $first() -> u32 {
            1
        }
        pub fn $second() -> u32 {
            2
        }
    };
}

define_pair!(alpha, beta);

pub fn plain() -> u32 {
    3
}
