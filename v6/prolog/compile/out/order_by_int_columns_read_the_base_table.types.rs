#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Score {
    pub player: i64,
    pub points: i64,
}
