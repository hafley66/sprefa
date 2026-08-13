#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Measurement {
    pub sensor_id: i64,
    pub label: Option<String>,
    pub reading: Option<i64>,
}
