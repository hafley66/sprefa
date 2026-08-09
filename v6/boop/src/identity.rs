//! Who is calling. Env inference answers "who am I", never "who is the child I
//! am spawning" (agent-bus, two incidents, 2026-08-07).

use std::collections::BTreeMap;

use anyhow::Result;

use crate::bus::Route;

/// The rung of the ladder that produced an identity. Ordered most trustworthy
/// first; `Env` is self-reported, so a consumer needing certainty checks this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rung {
    Env,
    Pane,
    None,
}

impl Rung {
    pub fn as_str(self) -> &'static str {
        match self {
            Rung::Env => "env",
            Rung::Pane => "pane",
            Rung::None => "none",
        }
    }

    /// Whether the rung observed the identity or inferred it.
    pub fn confidence(self) -> &'static str {
        match self {
            Rung::Env | Rung::Pane => "exact",
            Rung::None => "unresolved",
        }
    }
}

/// The caller's own identity. Every field is optional because an unresolved
/// caller is a real answer.
#[derive(Clone, Debug, Default)]
pub struct Identity {
    pub session: Option<String>,
    pub lane: Option<String>,
    pub parent: Option<String>,
    pub harness: Option<String>,
    pub pane: Option<String>,
    pub rung: Option<Rung>,
}

impl Identity {
    pub fn to_json(&self) -> serde_json::Value {
        let rung = self.rung.unwrap_or(Rung::None);
        serde_json::json!({
            "session": self.session,
            "lane": self.lane,
            "parent": self.parent,
            "harness": self.harness,
            "pane": self.pane,
            "rung": rung.as_str(),
            "confidence": rung.confidence(),
        })
    }
}

/// Resolve the caller. Rungs are tried in order and the first hit wins; a miss
/// falls through and nothing is guessed.
pub fn resolve(routes: &BTreeMap<String, Route>) -> Result<Identity> {
    if let Some(identity) = from_env() {
        return Ok(identity);
    }
    if let Some(identity) = from_pane(routes) {
        return Ok(identity);
    }
    Ok(Identity {
        rung: Some(Rung::None),
        ..Default::default()
    })
}

/// Rung 1. The stamp a boop spawn wrote into the child's own environment.
fn from_env() -> Option<Identity> {
    let session = std::env::var("BOOP_SESSION")
        .ok()
        .filter(|s| !s.is_empty())?;
    Some(Identity {
        session: Some(session),
        lane: std::env::var("BOOP_LANE").ok().filter(|s| !s.is_empty()),
        parent: std::env::var("BOOP_PARENT").ok().filter(|s| !s.is_empty()),
        harness: std::env::var("BOOP_HARNESS").ok().filter(|s| !s.is_empty()),
        pane: std::env::var("TMUX_PANE").ok(),
        rung: Some(Rung::Env),
    })
}

/// Rung 2. `$TMUX_PANE` names the pane; tmux names its session; the registry
/// names the lane that owns that session.
fn from_pane(routes: &BTreeMap<String, Route>) -> Option<Identity> {
    let pane = std::env::var("TMUX_PANE").ok().filter(|p| !p.is_empty())?;
    let tmux_session = crate::tmux::session_of_pane(None, &pane)?;
    let (lane, route) = routes
        .iter()
        .find(|(_, route)| route.tmux.as_deref() == Some(tmux_session.as_str()))?;
    Some(Identity {
        session: route.session_id.clone().or_else(|| Some(lane.clone())),
        lane: Some(lane.clone()),
        parent: None,
        harness: route.harness.clone(),
        pane: Some(pane),
        rung: Some(Rung::Pane),
    })
}

/// The env stamp a spawn writes into its CHILD. Every value describes the
/// child; the spawner appears only as `BOOP_PARENT`.
pub fn child_stamp(session: &str, lane: &str, harness: &str, parent: Option<&str>) -> String {
    let mut stamp = format!(
        "BOOP_SESSION={} BOOP_LANE={} BOOP_HARNESS={}",
        shell_word(session),
        shell_word(lane),
        shell_word(harness)
    );
    if let Some(parent) = parent {
        stamp.push_str(&format!(" BOOP_PARENT={}", shell_word(parent)));
    }
    stamp
}

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{child_stamp, resolve, Rung};

    /// The bus incident, in one assertion: the stamp describes the CHILD, and
    /// the spawner appears only as the parent.
    #[test]
    fn the_child_stamp_never_carries_the_spawners_session_as_its_own() {
        let stamp = child_stamp("child-1", "lane-a", "opencode", Some("coordinator-9"));
        assert!(stamp.contains("BOOP_SESSION='child-1'"));
        assert!(stamp.contains("BOOP_LANE='lane-a'"));
        assert!(stamp.contains("BOOP_PARENT='coordinator-9'"));
        assert!(
            !stamp.contains("BOOP_SESSION='coordinator-9'"),
            "the spawner's id must never become the child's own: {stamp}"
        );
    }

    #[test]
    fn a_stamp_with_no_parent_omits_the_variable() {
        let stamp = child_stamp("child-1", "lane-a", "claude", None);
        assert!(!stamp.contains("BOOP_PARENT"), "{stamp}");
    }

    /// An unresolved caller is reported, never defaulted to something plausible.
    #[test]
    fn an_unresolved_caller_says_so() {
        temp_env::with_vars(
            [("BOOP_SESSION", None::<&str>), ("TMUX_PANE", None::<&str>)],
            || {
                let identity = resolve(&BTreeMap::new()).unwrap();
                assert_eq!(identity.rung, Some(Rung::None));
                assert!(identity.session.is_none());
                assert_eq!(identity.to_json()["confidence"], "unresolved");
            },
        );
    }

    /// The env rung is self-reported, so it must say so rather than pass as
    /// verified: the bus incident was a self-reported id trusted blindly.
    #[test]
    fn the_env_rung_is_labelled() {
        temp_env::with_vars(
            [
                ("BOOP_SESSION", Some("s-1")),
                ("BOOP_LANE", Some("lane-a")),
                ("BOOP_PARENT", Some("coord")),
            ],
            || {
                let identity = resolve(&BTreeMap::new()).unwrap();
                assert_eq!(identity.rung, Some(Rung::Env));
                assert_eq!(identity.session.as_deref(), Some("s-1"));
                assert_eq!(identity.parent.as_deref(), Some("coord"));
                assert_eq!(identity.to_json()["rung"], "env");
            },
        );
    }

    mod temp_env {
        /// Env is process-global; these tests set and restore it around one
        /// closure and are marked serial by running inside a single mutex.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        pub fn with_vars<const N: usize>(vars: [(&str, Option<&str>); N], body: impl FnOnce()) {
            let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
            let saved: Vec<(String, Option<String>)> = vars
                .iter()
                .map(|(key, _)| ((*key).to_owned(), std::env::var(key).ok()))
                .collect();
            for (key, value) in &vars {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
            body();
            for (key, value) in saved {
                match value {
                    Some(value) => std::env::set_var(&key, value),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }
}
