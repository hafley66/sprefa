#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FighterSummary {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Squad {
    pub id: i64,
    pub members: i64,
}
