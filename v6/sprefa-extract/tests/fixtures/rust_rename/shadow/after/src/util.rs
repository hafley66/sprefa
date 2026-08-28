pub struct Tool;

pub fn scratch() -> u32 {
    struct Helper(u32);
    let inner = Helper(3);
    inner.0
}

mod nested {
    pub struct Helper;
}

pub fn both() -> (Tool, nested::Helper) {
    (Tool, nested::Helper)
}
