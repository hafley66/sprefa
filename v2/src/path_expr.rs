//! PathExpr — dot-delimited path into a cursor's state.
//!
//! Used by `CursorRefOp` to resolve which capture or cursor field to
//! rebase onto. Dot is the only delimiter. `$NAME` segments are capture
//! lookups; bare `ident` segments are cursor fields.
//!
//! Syntax:
//!   `.$NAME`  — look up `cursor.captures["NAME"]`
//!   `.repo`   — cursor.repo string
//!   `.rev`    — cursor.rev string
//!   `.fs`     — cursor.fs file path string
//!   `.N`      — array index (future; not parsed in v1)
//!
//! Future: `Chain(Vec<PathSeg>)` for global paths like
//!   `.$ROOT.fs.0.json.1.$NAME`.

use std::borrow::Cow;
use std::sync::Arc;

use crate::_0_types::Cursor;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathExpr {
    /// `.$NAME` — look up capture by name on the current cursor.
    SelfCap(Arc<str>),
    /// `.repo` / `.rev` / `.fs` — cursor metadata fields.
    Field(FieldKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    Repo,
    Rev,
    Fs,
}

// ---------------------------------------------------------------------------
// Parse
// ---------------------------------------------------------------------------

impl PathExpr {
    /// Parse a path expression from the text that follows `&` in the source.
    /// Expected forms: `.$NAME`, `.repo`, `.rev`, `.fs`.
    pub fn parse(src: &str) -> Result<Self, String> {
        let s = src.trim();
        let rest = s.strip_prefix('.').ok_or_else(|| {
            format!("expected `.` at start of path, got `{s}`")
        })?;

        if let Some(name) = rest.strip_prefix('$') {
            if name.is_empty() {
                return Err("capture name after `$` must not be empty".to_string());
            }
            if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!("invalid capture name `{name}`: only alphanumeric and `_` allowed"));
            }
            return Ok(PathExpr::SelfCap(Arc::from(name)));
        }

        match rest {
            "repo" => Ok(PathExpr::Field(FieldKind::Repo)),
            "rev"  => Ok(PathExpr::Field(FieldKind::Rev)),
            "fs"   => Ok(PathExpr::Field(FieldKind::Fs)),
            other  => Err(format!(
                "unknown path `{other}`; expected `.$NAME`, `.repo`, `.rev`, or `.fs`"
            )),
        }
    }
}

impl PathExpr {
    /// Inverse of `parse` — render back to the `.foo` source form.
    pub fn render(&self) -> String {
        match self {
            PathExpr::SelfCap(name)         => format!(".${name}"),
            PathExpr::Field(FieldKind::Repo) => ".repo".to_string(),
            PathExpr::Field(FieldKind::Rev)  => ".rev".to_string(),
            PathExpr::Field(FieldKind::Fs)   => ".fs".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Resolve
// ---------------------------------------------------------------------------

impl PathExpr {
    /// Resolve this path against `cursor` to a string value.
    /// Returns `None` if the capture is absent or the fs field is unset.
    pub fn resolve<'a>(&self, cursor: &'a Cursor) -> Option<Cow<'a, str>> {
        match self {
            PathExpr::SelfCap(name) => cursor
                .captures
                .get(name.as_ref())
                .map(|c| Cow::Borrowed(c.value.as_ref())),
            PathExpr::Field(FieldKind::Repo) =>
                Some(Cow::Borrowed(cursor.repo.as_ref())),
            PathExpr::Field(FieldKind::Rev) =>
                Some(Cow::Borrowed(cursor.rev.as_ref())),
            PathExpr::Field(FieldKind::Fs) => cursor
                .fs
                .as_ref()
                .map(|fp| Cow::Owned(fp.0.to_string_lossy().into_owned())),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_self_cap() {
        assert_eq!(PathExpr::parse(".$FOO").unwrap(), PathExpr::SelfCap(Arc::from("FOO")));
        assert_eq!(PathExpr::parse(".$foo_bar").unwrap(), PathExpr::SelfCap(Arc::from("foo_bar")));
    }

    #[test]
    fn parse_fields() {
        assert_eq!(PathExpr::parse(".repo").unwrap(), PathExpr::Field(FieldKind::Repo));
        assert_eq!(PathExpr::parse(".rev").unwrap(),  PathExpr::Field(FieldKind::Rev));
        assert_eq!(PathExpr::parse(".fs").unwrap(),   PathExpr::Field(FieldKind::Fs));
    }

    #[test]
    fn render_round_trips() {
        for s in [".$FOO", ".$foo_bar", ".repo", ".rev", ".fs"] {
            assert_eq!(PathExpr::parse(s).unwrap().render(), s);
        }
    }

    #[test]
    fn parse_errors() {
        assert!(PathExpr::parse("$FOO").is_err());   // missing leading dot
        assert!(PathExpr::parse(".$").is_err());     // empty name after $
        assert!(PathExpr::parse(".unknown").is_err());
    }
}
