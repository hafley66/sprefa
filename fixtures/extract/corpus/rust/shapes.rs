pub struct UserRecord {
    pub id: u32,
    pub name: String,
}

pub struct TeamRecord {
    pub owner: UserRecord,
    pub label: String,
}
