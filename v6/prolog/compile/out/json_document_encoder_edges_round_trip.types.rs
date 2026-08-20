#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Echoed {
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RawDoc {
    pub body: serde_json::Value,
}
