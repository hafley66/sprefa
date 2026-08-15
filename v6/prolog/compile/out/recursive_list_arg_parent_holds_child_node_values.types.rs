#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Node {
    pub name: String,
    pub children: Vec<Node>,
}
