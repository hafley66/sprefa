//! Process-environment relation: `env(name, value)` over an allowlisted
//! `std::env::vars()`.

use anyhow::Result;

use crate::ast::{RelDecl, Type, Value};
use crate::engine::Engine;
use crate::lower::txt_tbl;

use super::{col, RelKind};

const ENV_ALLOW_PREFIXES: &[&str] = &["SPREFA_", "DL_", "SG_"];
const ENV_ALLOW_EXACT: &[&str] = &["CI", "GITHUB_ACTIONS", "GITLAB_CI"];

/// Allowlisted process environment: `env(name, value)`, one row per var whose
/// name starts with a tool-namespace prefix or exactly matches a CI marker.
/// Everything else (tokens, credentials, `AWS_*`, ...) stays off disk. The
/// environment is constant for the process lifetime, so the refresh fills
/// once and every subsequent call self-diffs to `Ok(false)`.
pub struct EnvKind;

impl RelKind for EnvKind {
    fn rels(&self) -> &'static [&'static str] {
        &["env"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl {
            name: "env".into(),
            cols: vec![col("name", Type::Text), col("value", Type::Text)],
            group: "sys",
            doc: "allowlisted process environment variables (name, value) captured at process start; constant for the process lifetime; scoped to SPREFA_/DL_/SG_ prefixes plus CI markers so secrets stay off disk",
            ..Default::default()
        }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in process-environment relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let mut rows: Vec<(String, String)> = std::env::vars()
            .filter(|(name, _)| {
                ENV_ALLOW_PREFIXES.iter().any(|p| name.starts_with(p))
                    || ENV_ALLOW_EXACT.contains(&name.as_str())
            })
            .collect();
        rows.sort();
        let existing: Vec<(String, String)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"name\", \"value\" FROM {} ORDER BY \"name\", \"value\"",
                txt_tbl("env")))?;
            let rs = s.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            rs.filter_map(|x| x.ok()).collect()
        };
        if existing == rows { return Ok(false); }
        let out: Vec<Vec<Value>> = rows.into_iter()
            .map(|(name, value)| vec![Value::Text(name), Value::Text(value)]).collect();
        eng.refresh_rel("env", &["name", "value"], &out)?;
        Ok(true)
    }
}
