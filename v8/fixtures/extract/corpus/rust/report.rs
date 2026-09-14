use crate::shapes::{TeamRecord, UserRecord};

pub fn format_user(record: &UserRecord) -> String {
    record.name.clone()
}

pub fn format_team(team: &TeamRecord) -> String {
    format_user(&team.owner)
}
