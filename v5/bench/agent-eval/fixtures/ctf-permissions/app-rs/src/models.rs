//! The user model. `can_export` is the export-data permission the service
//! gates in four different styles across the crate.

#[derive(Clone, Debug)]
pub struct User {
    pub id: String,
    pub role: Role,
    // Direct permission flags (the oldest gating style).
    pub can_export: bool,
    pub can_import: bool,
    pub can_view: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Admin,
    Analyst,
    Viewer,
}

pub type PermResult = Result<(), String>;

pub fn test_user() -> User {
    User {
        id: "u1".into(),
        role: Role::Analyst,
        can_export: true,
        can_import: false,
        can_view: true,
    }
}
