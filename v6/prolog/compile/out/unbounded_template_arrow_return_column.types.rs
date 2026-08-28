#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Counter<Node> {
    pub node: Node,
    pub r#return: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GenCounterTextC559074ec2ceaf94 {
    pub node: String,
    pub r#return: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Carry {
    pub id: i64,
    pub pos: GenCounterTextC559074ec2ceaf94,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Cursor {
    pub id: i64,
    pub pos: GenCounterTextC559074ec2ceaf94,
}
