#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Copied {
    pub items: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Source {
    pub items: Vec<String>,
}
