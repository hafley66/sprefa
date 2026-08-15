#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pentacube {
    pub id: i64,
    pub depth_five_values: Vec<Vec<Vec<Vec<Vec<String>>>>>,
}
