#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FromPoll {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FromPush {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Latest {
    pub key: String,
    pub value: String,
}
