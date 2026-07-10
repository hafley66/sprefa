//! Abstraction 4: a runtime, CONFIG-FILE-driven rule engine. Allowed roles per
//! permission live in config/rules.toml; `enforce_rule` reads that file and
//! returns Err when the role is not listed. The enforcement site is the
//! `enforce_rule(&ctx, EXPORT_RULE)?` call in a handler.
use crate::models::{PermResult, Role, User};

/// The config rule key for the export permission (matches config/rules.toml).
pub const EXPORT_RULE: &str = "can_export";

pub struct RuleContext<'a> {
    pub user: &'a User,
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Admin => "admin",
        Role::Analyst => "analyst",
        Role::Viewer => "viewer",
    }
}

/// Minimal stand-in for a parsed config/rules.toml (loaded at runtime in the
/// real app). Maps a rule key to the roles it permits.
fn allowed_roles(rule_key: &str) -> Vec<&'static str> {
    match rule_key {
        "can_export" => vec!["admin", "analyst"],
        "can_import" => vec!["admin"],
        "can_view" => vec!["admin", "analyst", "viewer"],
        _ => vec![],
    }
}

pub fn eval_rule(ctx: &RuleContext, rule_key: &str) -> bool {
    allowed_roles(rule_key).contains(&role_name(ctx.user.role))
}

pub fn enforce_rule(ctx: &RuleContext, rule_key: &str) -> PermResult {
    if eval_rule(ctx, rule_key) {
        Ok(())
    } else {
        Err(format!("config rule denied: {}", rule_key))
    }
}
