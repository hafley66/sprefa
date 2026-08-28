pub struct Helper;

pub fn scratch() -> u32 {
    struct Helper(u32);
    let inner = Helper(3);
    inner.0
}

mod nested {
    pub struct Helper;
}

pub fn both() -> (Helper, nested::Helper) {
    (Helper, nested::Helper)
}
