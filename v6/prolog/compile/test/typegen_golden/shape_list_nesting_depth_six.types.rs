#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hexcube {
    pub id: i64,
    pub depth_six_values: Vec<Vec<Vec<Vec<Vec<Vec<String>>>>>>,
}
