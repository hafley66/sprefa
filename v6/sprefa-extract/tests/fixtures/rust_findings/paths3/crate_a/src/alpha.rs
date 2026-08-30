pub enum Alpha {
    First(u32),
    Second,
}

pub fn alpha_user() -> u32 {
    let value = Alpha::First(3);
    match value {
        Alpha::First(count) => count,
        Alpha::Second => 0,
    }
}
