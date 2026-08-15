#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Batch {
    pub id: i64,
    pub payloads: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Carry {
    pub id: i64,
    pub payloads: Vec<serde_json::Value>,
}
