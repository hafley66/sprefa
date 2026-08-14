#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Chart {
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Image {
    pub repository: String,
    pub tag: String,
}
