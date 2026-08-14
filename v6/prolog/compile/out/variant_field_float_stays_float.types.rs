#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReadingPeak {
    pub id: i64,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReadingTag {
    pub id: i64,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReadingTrough {
    pub id: i64,
    pub value: f64,
}
