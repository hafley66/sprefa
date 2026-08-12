//! A harness driven through its own terminal UI in a tmux window: the brief is
//! typed, and so is every later hail.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEnd};

/// A TUI stops repainting when it stops working, so an unchanged pane body is
/// the turn-end signal.
const IDLE_SETTLE: Duration = Duration::from_secs(20);
/// Footer lines carry a rotating tip and a clock; they are not turn state.
const FOOTER_LINES: usize = 3;
const BOOT_CEILING: Duration = Duration::from_secs(90);

/// How one harness's terminal UI is spoken to.
#[derive(Clone, Debug)]
pub struct TuiProfile {
    pub harness: &'static str,
    /// The shell command the window runs, model already resolved in.
    pub command: String,
    /// The key that pushes text into a running turn. `None` means this TUI
    /// treats a plain Enter as a steer.
    pub steer_key: Option<&'static str>,
    /// Keys sent once after boot to clear a first-run dialog. Every lane gets
    /// a fresh worktree, so a per-folder trust prompt fires on every spawn.
    pub boot_keys: Vec<&'static str>,
}

pub struct TuiChannel {
    profile: TuiProfile,
    socket: Option<String>,
    target: String,
    session: String,
    /// Pane body hash and when it was first seen, for the idle test.
    settled_since: Option<(u64, Instant)>,
    turn_open: bool,
}

impl TuiChannel {
    pub fn open(profile: TuiProfile, spec: &ChannelSpec, socket: Option<String>) -> Result<TuiChannel> {
        let session = host_session(socket.as_deref())?;
        let cwd = spec.cwd.display().to_string();
        let target = crate::tmux::mux()
            .new_window(
                socket.as_deref(),
                &session,
                &format!("{}-agent", profile.harness),
                &cwd,
                &profile.command,
            )
            .with_context(|| format!("open a {} tui window", profile.harness))?;
        let mut channel = TuiChannel {
            profile,
            socket,
            target,
            session,
            settled_since: None,
            turn_open: false,
        };
        channel.wait_for_boot()?;
        Ok(channel)
    }

    /// The tmux target a human attaches to, and the route's `tmux` field.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Boot is done when the pane has painted something and then held it. The
    /// boot keys clear a first-run dialog before the first turn is typed.
    fn wait_for_boot(&mut self) -> Result<()> {
        let deadline = Instant::now() + BOOT_CEILING;
        let mut painted = false;
        while Instant::now() < deadline {
            let body = self.body()?;
            if !painted && !body.trim().is_empty() {
                painted = true;
                self.settled_since = None;
            }
            if painted && self.settled_for(Duration::from_secs(3))? {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        for key in self.profile.boot_keys.clone() {
            crate::tmux::mux().send_key_named(self.socket.as_deref(), &self.target, key)?;
            std::thread::sleep(Duration::from_millis(1200));
        }
        self.settled_since = None;
        Ok(())
    }

    fn body(&self) -> Result<String> {
        let pane = crate::tmux::mux().capture_pane(self.socket.as_deref(), &self.target, None)?;
        let lines: Vec<&str> = pane.lines().collect();
        let keep = lines.len().saturating_sub(FOOTER_LINES);
        Ok(lines[..keep].join("\n"))
    }

    /// True once the pane body has held one hash for `window`.
    fn settled_for(&mut self, window: Duration) -> Result<bool> {
        let digest = hash(&self.body()?);
        match self.settled_since {
            Some((seen, since)) if seen == digest => Ok(since.elapsed() >= window),
            _ => {
                self.settled_since = Some((digest, Instant::now()));
                Ok(false)
            }
        }
    }

    /// `send-keys -l` writes LF for an embedded newline and only a named Enter
    /// sends CR, so a multi-line brief lands whole instead of line by line.
    fn type_and_submit(&self, text: &str) -> Result<()> {
        let mux = crate::tmux::mux();
        mux.send_text(self.socket.as_deref(), &self.target, text)?;
        std::thread::sleep(Duration::from_millis(300));
        mux.send_key_named(self.socket.as_deref(), &self.target, "Enter")?;
        Ok(())
    }
}

impl LaneChannel for TuiChannel {
    fn conversation_id(&self) -> Option<String> {
        Some(self.target.clone())
    }

    fn start_turn(&mut self, text: &str) -> Result<()> {
        self.type_and_submit(text)?;
        self.turn_open = true;
        self.settled_since = None;
        Ok(())
    }

    fn steer(&mut self, text: &str) -> Result<Delivery> {
        self.type_and_submit(text)?;
        if let Some(key) = self.profile.steer_key {
            std::thread::sleep(Duration::from_millis(400));
            crate::tmux::mux().send_key_named(self.socket.as_deref(), &self.target, key)?;
        }
        self.settled_since = None;
        Ok(Delivery::MidTurn)
    }

    fn poll_turn(&mut self, timeout: Duration) -> Result<Option<TurnEnd>> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.settled_for(IDLE_SETTLE)? {
                self.turn_open = false;
                return Ok(Some(TurnEnd::ok("pane idle")));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    fn close(&mut self) -> Result<()> {
        let _ = crate::tmux::mux().send_key_named(self.socket.as_deref(), &self.target, "C-c");
        Ok(())
    }
}

/// The tmux session the supervisor is already running in. A supervisor started
/// outside tmux gets one of its own so the TUI still has a pane.
fn host_session(socket: Option<&str>) -> Result<String> {
    if let Ok(pane) = std::env::var("TMUX_PANE") {
        if let Some(session) = crate::tmux::mux().session_of_pane(socket, &pane) {
            return Ok(session);
        }
    }
    let name = format!("boop-tui-{}", std::process::id());
    if !crate::tmux::mux().has_session(socket, &name)? {
        crate::tmux::mux().new_detached_session(
            socket,
            &name,
            &std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("/"))
                .display()
                .to_string(),
            "sh -c 'while true; do sleep 3600; done'",
        )?;
    }
    Ok(name)
}

fn hash(body: &str) -> u64 {
    let mut value: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in body.as_bytes() {
        value ^= *byte as u64;
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    value
}

/// `opencode` takes plain Enter as a steer; its own pane shows the new turn
/// starting mid-work with no extra key.
pub fn opencode_profile(spec: &ChannelSpec) -> TuiProfile {
    let mut command = String::from("opencode --auto");
    if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
        command.push_str(&format!(" -m {}", quote(model)));
    }
    if let Some(session) = &spec.resume {
        command.push_str(&format!(" -s {}", quote(session)));
    }
    TuiProfile {
        harness: "opencode",
        command,
        steer_key: None,
        boot_keys: Vec::new(),
    }
}

/// `kimi` QUEUES on Enter and steers only on ctrl-s. Its per-folder trust
/// dialog fires on every fresh worktree.
pub fn kimi_profile(spec: &ChannelSpec) -> TuiProfile {
    let mut command = String::from("kimi --auto");
    if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
        command.push_str(&format!(" -m {}", quote(model)));
    }
    if let Some(session) = &spec.resume {
        command.push_str(&format!(" -S {}", quote(session)));
    }
    TuiProfile {
        harness: "kimi",
        command,
        steer_key: Some("C-s"),
        boot_keys: vec!["Enter"],
    }
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(model: Option<&str>) -> ChannelSpec {
        ChannelSpec {
            model: model.map(str::to_owned),
            cwd: std::env::temp_dir(),
            resume: None,
        }
    }

    #[test]
    fn opencode_takes_enter_as_its_steer_and_kimi_needs_ctrl_s() {
        assert_eq!(opencode_profile(&spec(None)).steer_key, None);
        assert_eq!(kimi_profile(&spec(None)).steer_key, Some("C-s"));
    }

    /// Every lane worktree is a folder kimi has not seen, so the trust dialog
    /// fires on every spawn and one Enter clears it.
    #[test]
    fn kimi_boots_through_its_trust_dialog() {
        assert_eq!(kimi_profile(&spec(None)).boot_keys, vec!["Enter"]);
        assert!(opencode_profile(&spec(None)).boot_keys.is_empty());
    }

    #[test]
    fn both_profiles_run_auto_and_carry_the_model() {
        let opencode = opencode_profile(&spec(Some("openrouter/deepseek/deepseek-v4-flash-0731")));
        assert!(opencode.command.starts_with("opencode --auto"), "{}", opencode.command);
        assert!(
            opencode.command.contains("-m 'openrouter/deepseek/deepseek-v4-flash-0731'"),
            "{}",
            opencode.command
        );
        let kimi = kimi_profile(&spec(Some("kimi-code/k3")));
        assert!(kimi.command.starts_with("kimi --auto"), "{}", kimi.command);
        assert!(kimi.command.contains("-m 'kimi-code/k3'"), "{}", kimi.command);
    }

    #[test]
    fn a_resume_uses_each_harness_own_session_flag() {
        let mut request = spec(None);
        request.resume = Some("ses_abc".to_owned());
        assert!(opencode_profile(&request).command.contains("-s 'ses_abc'"));
        assert!(kimi_profile(&request).command.contains("-S 'ses_abc'"));
    }

    #[test]
    fn the_footer_is_not_turn_state() {
        let pane = "one\ntwo\nthree\nfooter1\nfooter2\nfooter3";
        let lines: Vec<&str> = pane.lines().collect();
        let keep = lines.len().saturating_sub(FOOTER_LINES);
        assert_eq!(lines[..keep].join("\n"), "one\ntwo\nthree");
    }

    #[test]
    fn a_changed_body_hashes_differently() {
        assert_ne!(hash("working ."), hash("working .."));
        assert_eq!(hash("idle"), hash("idle"));
    }
}
