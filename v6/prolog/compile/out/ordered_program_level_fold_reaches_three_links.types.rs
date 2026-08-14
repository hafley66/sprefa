#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DispatchLeg {
    pub leg_id: i64,
    pub dispatch_id: i64,
    pub previous_leg: i64,
    pub kilos: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LegTotal {
    pub leg_id: i64,
    pub dispatch_id: i64,
    pub kilos: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ping {
    pub partition: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PingOrdinal {
    pub partition: String,
    pub col2: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SeqPingOrdinal2 {
    pub partition: String,
    pub at: i64,
}
