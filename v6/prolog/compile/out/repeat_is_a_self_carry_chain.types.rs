#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Kick {
    pub col1: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pulse {
    pub next: i64,
}
