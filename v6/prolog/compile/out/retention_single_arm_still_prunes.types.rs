#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Journal {
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ping {
    pub payload: String,
}
