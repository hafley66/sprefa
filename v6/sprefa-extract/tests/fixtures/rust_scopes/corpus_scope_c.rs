pub fn helper() -> u32 {
    1
}

pub mod inner {
    pub fn go() -> u32 {
        helper()
    }
}
