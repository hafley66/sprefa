#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Counter {
    pub name: String,
    pub next: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Decrement {
    pub name: String,
    pub col2: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Increment {
    pub name: String,
    pub col2: String,
}
