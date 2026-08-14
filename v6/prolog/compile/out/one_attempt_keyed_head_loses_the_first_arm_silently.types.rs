#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DispatchAck {
    pub dispatch_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DispatchSeal {
    pub sealed_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DispatchWinner {
    pub dispatch_id: i64,
    pub col2: String,
}
