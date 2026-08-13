#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Derived {
    pub seen_value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Seen {
    pub value: String,
}
