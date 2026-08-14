#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Heavy {
    pub tree_id: i64,
    pub kilos: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pick {
    pub tree_id: i64,
    pub kilos: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Report {
    pub tree_id: i64,
    pub kilos: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tree {
    pub tree_id: i64,
}
