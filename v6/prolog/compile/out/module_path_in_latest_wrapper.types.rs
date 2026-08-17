#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Bell {
    pub tree_id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Called {
    pub tree_id: i64,
    pub owner: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Roster {
    pub tree_id: i64,
    pub owner: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Orchard {
}
