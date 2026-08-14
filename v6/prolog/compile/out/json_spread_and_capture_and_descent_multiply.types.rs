#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hit {
    pub item: i64,
    pub name: String,
    pub leaf: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Spec {
    pub body: serde_json::Value,
}
