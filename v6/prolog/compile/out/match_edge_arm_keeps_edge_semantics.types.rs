#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Cache {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PollResult {
    pub key: String,
    pub value: String,
}
