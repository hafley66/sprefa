#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MermaidLine {
    pub file_name: String,
    pub line_ordinal: i64,
    pub line_text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MermaidText {
    pub file_name: String,
    pub col2: String,
}
