#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ping {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SeenAt {
    pub name: String,
    pub tick: i64,
}
