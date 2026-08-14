#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AppendLine {
    pub channel: String,
    pub piece: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LogText {
    pub channel: String,
    pub next: String,
}
