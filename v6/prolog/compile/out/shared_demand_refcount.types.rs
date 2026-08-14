#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Demanded {
    pub target: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EffectCall {
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpenFeed {
    pub session_id: String,
    pub target: String,
}
