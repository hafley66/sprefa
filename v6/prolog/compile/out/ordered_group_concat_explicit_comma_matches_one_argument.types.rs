#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExplicitJoined {
    pub group: String,
    pub col2: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Item {
    pub group: String,
    pub value: String,
}
