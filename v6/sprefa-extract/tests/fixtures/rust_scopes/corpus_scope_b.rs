pub fn helper() -> u32 {
    2
}

pub mod inner {
    pub fn go() -> u32 {
        helper()
    }
}
