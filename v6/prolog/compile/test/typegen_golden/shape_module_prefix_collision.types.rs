#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlphaStoreTicket {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct M9BetaStoreTicket {
    pub id: i64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Audit {
    pub id: i64,
    pub subject: AlphaStoreTicket,
}
