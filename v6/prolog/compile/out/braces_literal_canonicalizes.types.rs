#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Doc {
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Seed {
    pub name: String,
}
