#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Tree {
    Leaf { value: i64 },
    Branch { left: Tree, right: Tree },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TreeBranch {
    pub id: i64,
    pub left: Tree,
    pub right: Tree,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TreeKind {
    pub id: i64,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TreeLeaf {
    pub id: i64,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TreeTag {
    pub id: i64,
    pub tag: String,
}
