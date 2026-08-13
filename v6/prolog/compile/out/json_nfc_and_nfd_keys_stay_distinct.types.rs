#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct KeySeen {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RawDoc {
    pub body: serde_json::Value,
}
