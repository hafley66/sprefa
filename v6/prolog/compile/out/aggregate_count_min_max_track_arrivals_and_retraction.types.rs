#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StarRow {
    pub repo: String,
    pub stars: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stat {
    pub repo: String,
    pub col2: i64,
    pub col3: i64,
    pub col4: i64,
}
