// sample.rs: a small Rust fixture exercising every facet (type/call/df/const).
// ASCII-only so syn's char column equals the byte column (parity is clean).

pub struct Engine {
    name: String,
}

pub enum Mode {
    Fast,
    Slow,
}

pub const GREETING: &str = "hello";

pub fn trim(value: String) -> String {
    value
}

pub fn make_engine(name: String) -> Engine {
    let trimmed = trim(name);
    let engine = Engine { name: trimmed };
    engine
}

impl Engine {
    pub fn mode(&self) -> Mode {
        let picked = Mode::Fast;
        picked
    }
}

pub fn apply(value: String) -> String {
    let func = |text: String| text;
    func(value)
}
