#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Demanded {
    pub target: String,
    pub pane_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DetailRow {
    pub item_id: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DetailView {
    pub item_id: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LiveDetail {
    pub pane_id: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpenDetail {
    pub pane_id: String,
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpenPane {
    pub col1: String,
    pub col2: String,
}
