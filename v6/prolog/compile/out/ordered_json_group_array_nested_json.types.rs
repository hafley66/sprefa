#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Child {
    pub group: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Nested {
    pub group: String,
    pub col2: serde_json::Value,
}
