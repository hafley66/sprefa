#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Demand {
    pub args: String,
    pub salt: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fill {
    pub args: String,
    pub salt: String,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Response {
    pub args: String,
    pub salt: String,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WatchRequest {
    pub col1: String,
    pub args: String,
    pub salt: String,
}
