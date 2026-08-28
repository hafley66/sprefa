#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Audit {
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EventDoc {
    pub doc: serde_json::Value,
}
