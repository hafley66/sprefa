#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Latest {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SetValue {
    pub key: String,
    pub value: String,
}
