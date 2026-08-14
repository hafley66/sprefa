#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CloseRequest {
    pub session_id: String,
    pub tab_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Closed {
    pub session_id: String,
    pub tab_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Demanded {
    pub col1: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LiveTab {
    pub session_id: String,
    pub tab_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpenRequest {
    pub session_id: String,
    pub tab_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpenTab {
    pub session_id: String,
    pub tab_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TabRow {
    pub tab_id: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TabView {
    pub tab_id: String,
    pub body: String,
}
