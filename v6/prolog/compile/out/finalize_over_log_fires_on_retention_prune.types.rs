#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ev {
    pub ordinal: i64,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Gone {
    pub ordinal: i64,
    pub payload: String,
}
