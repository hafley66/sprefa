// docs.rs: doc-facet parity fixture. Exercises v5's rust_docs_from: the
// doc-attribute (the desugared form of the triple-slash doc comment) on a
// struct/enum/trait/fn item or an impl method becomes a DocFact keyed by the
// entity's sym. Item kinds OUTSIDE that walk (const/static/type-alias) mint no
// doc row, and plain line comments are never docs (`apply` below). The
// documented entities re-exercise the ported facets (type/call/df/const) on
// doc-heavy input. ASCII-only so syn's char column equals the byte column
// (parity is clean).

/// A string-bearing engine.
pub struct Engine {
    name: String,
}

/// Operating modes.
pub enum Mode {
    Fast,
    Slow,
}

// A string const: the const facet fires, but const items get NO doc row.
pub const GREETING: &str = "hello";

/// Trims a value down.
pub fn trim(value: String) -> String {
    value
}

/// Builds an engine from a name.
pub fn make_engine(name: String) -> Engine {
    let trimmed = trim(name);
    let engine = Engine { name: trimmed };
    engine
}

impl Engine {
    /// Picks the fast mode.
    pub fn mode(&self) -> Mode {
        let picked = Mode::Fast;
        picked
    }
}

// Plain comments are not docs: no doc row for this fn.
pub fn apply(value: String) -> String {
    let func = |text: String| text;
    func(value)
}
