#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Link {
    pub from_node: i64,
    pub to_node: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Reach {
    pub from_node: i64,
    pub to_node: i64,
}
