#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ClosedInner {
    pub outer_id: String,
    pub inner_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ClosedOuter {
    pub outer_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EndASignal {
    pub outer_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EndBSignal {
    pub outer_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EndCSignal {
    pub inner_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LiveInner {
    pub outer_id: String,
    pub inner_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LiveOuter {
    pub outer_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpenInner {
    pub outer_id: String,
    pub inner_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpenOuter {
    pub outer_id: String,
}
