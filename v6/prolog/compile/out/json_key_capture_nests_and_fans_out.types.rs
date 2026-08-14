#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Operation {
    pub path: String,
    pub method: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Spec {
    pub body: serde_json::Value,
}
