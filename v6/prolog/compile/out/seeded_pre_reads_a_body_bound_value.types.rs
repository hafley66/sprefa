#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Counter {
    pub name: String,
    pub next: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Increment {
    pub name: String,
    pub start: i64,
}
