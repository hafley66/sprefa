#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EmailState {
    pub user_id: i64,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UserProfile {
    pub user_id: i64,
    pub email: Option<String>,
}
