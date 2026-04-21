use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct Diag {
    pub msg: String,
}

impl Diag {
    pub fn new(msg: impl Into<String>) -> Self {
        Self { msg: msg.into() }
    }
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

pub type Diags = Vec<Diag>;
