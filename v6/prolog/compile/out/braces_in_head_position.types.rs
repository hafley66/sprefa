#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DocOut {
    pub col1: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Seed {
    pub name: String,
}
