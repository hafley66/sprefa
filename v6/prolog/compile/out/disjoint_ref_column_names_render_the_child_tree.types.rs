#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Carrier {
    pub id: i64,
    pub nested: ShellPair,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LeafPair {
    pub left: i64,
    pub right: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Seen {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShellPair {
    pub head: LeafPair,
    pub tail: LeafPair,
}
