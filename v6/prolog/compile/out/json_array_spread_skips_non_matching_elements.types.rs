#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Numbered {
    pub number: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Resp {
    pub body: serde_json::Value,
}
