#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Arrive {
    pub id: i64,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Thing {
    pub id: i64,
    pub payload: String,
    pub born: i64,
    pub tick: i64,
}
