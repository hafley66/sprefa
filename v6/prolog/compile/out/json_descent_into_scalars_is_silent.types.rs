#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Doc {
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Found {
    pub value: String,
}
