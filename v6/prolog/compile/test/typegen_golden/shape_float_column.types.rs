#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Measurement {
    pub id: i64,
    pub ratio: f64,
    pub label: String,
    pub samples: Vec<f64>,
    pub margin: Option<f64>,
}
