#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlphaStoreTicket {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BetaServiceTicket {
    pub id: i64,
}
