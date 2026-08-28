#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tally {
    pub player: i64,
    pub points: i64,
}
