pub fn helper() -> u32 {
    7
}

pub fn caller() -> u32 {
    assert_eq!(helper(), helper())
}

pub fn direct() -> u32 {
    helper()
}
