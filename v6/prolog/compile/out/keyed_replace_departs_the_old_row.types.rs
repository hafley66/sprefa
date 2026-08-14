#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FromPoll {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Latest {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReplacedValue {
    pub key: String,
    pub old_value: String,
}
