#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Bundle {
    pub session: String,
    pub ai_run: i64,
    pub user_run: i64,
    pub ai_text: String,
    pub user_text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Handled {
    pub session: String,
    pub user_run: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LaterStartBetween {
    pub session: String,
    pub run_turn: i64,
    pub turn_number: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PrevSameRole {
    pub session: String,
    pub turn_number: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Resident {
    pub session: String,
    pub user_run: i64,
    pub col3: i64,
    pub col4: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResidentAsk {
    pub session: String,
    pub user_run: i64,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Run {
    pub session: String,
    pub run_turn: i64,
    pub role: String,
    pub ai_text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunBetween {
    pub session: String,
    pub ai_run: i64,
    pub user_run: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunMember {
    pub session: String,
    pub run_turn: i64,
    pub turn_number: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunSaid {
    pub session: String,
    pub run_turn: i64,
    pub role: String,
    pub turn_number: i64,
    pub said: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunStart {
    pub session: String,
    pub turn_number: i64,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Turn {
    pub session: String,
    pub turn_number: i64,
    pub col3: i64,
    pub role: String,
    pub said: String,
}
