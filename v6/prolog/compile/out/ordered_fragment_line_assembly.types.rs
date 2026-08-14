#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FragmentLine {
    pub fragment_name: String,
    pub line_ordinal: i64,
    pub line_text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FragmentText {
    pub fragment_name: String,
    pub col2: String,
}
