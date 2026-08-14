#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MetricDoc {
    pub session: String,
    pub snapshot: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MetricSample {
    pub session: String,
    pub patch: serde_json::Value,
}
