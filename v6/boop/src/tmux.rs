//! Layer 1: tmux control. The long-lived control-mode client is built here
//! because no crate on crates.io sells one (tmux_interface documents CLI-only,
//! no `-C` guard parsing). `tmux_interface` builds the one-shot command argv
//! it can express; send-keys literal injection and the control client are raw
//! spawns because tmux_interface exposes no literal-key mode.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result};

/// A control-mode notification. Unknown `%`-prefixed lines are kept, never
/// dropped; tmux adds notification types across versions.
#[derive(Clone, Debug, PartialEq)]
pub enum Notification {
    Output { pane: String, text: String },
    SessionChanged { id: String, name: String },
    WindowAdd { id: String },
    Exit,
    Unknown(String),
}

/// A line read from the control-mode stream.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlEvent {
    BlockBegin { num: usize },
    BlockEnd { num: usize },
    BlockError { num: usize },
    Body(String),
    Notification(Notification),
}

/// Parse one control-mode line into a control event.
pub fn parse_event(line: &str) -> ControlEvent {
    let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
    if let Some(rest) = trimmed.strip_prefix("%begin") {
        return match num_field(rest) {
            Some(num) => ControlEvent::BlockBegin { num },
            None => ControlEvent::Body(trimmed.to_owned()),
        };
    }
    if let Some(rest) = trimmed.strip_prefix("%end") {
        return match num_field(rest) {
            Some(num) => ControlEvent::BlockEnd { num },
            None => ControlEvent::Body(trimmed.to_owned()),
        };
    }
    if let Some(rest) = trimmed.strip_prefix("%error") {
        return match num_field(rest) {
            Some(num) => ControlEvent::BlockError { num },
            None => ControlEvent::Body(trimmed.to_owned()),
        };
    }
    if let Some(rest) = trimmed.strip_prefix("%output") {
        let (pane, text) = split_two(rest);
        return ControlEvent::Notification(Notification::Output { pane, text });
    }
    if let Some(rest) = trimmed.strip_prefix("%session-changed") {
        let (id, name) = split_two(rest);
        return ControlEvent::Notification(Notification::SessionChanged { id, name });
    }
    if trimmed.starts_with('%') {
        if trimmed == "%exit" {
            return ControlEvent::Notification(Notification::Exit);
        }
        if let Some(rest) = trimmed.strip_prefix("%window-add") {
            return ControlEvent::Notification(Notification::WindowAdd {
                id: rest.trim().to_owned(),
            });
        }
        return ControlEvent::Notification(Notification::Unknown(trimmed.to_owned()));
    }
    ControlEvent::Body(trimmed.to_owned())
}

/// The `%begin/%end/%error` line's command number, its second field (the first
/// is a timestamp).
fn num_field(rest: &str) -> Option<usize> {
    let mut fields = rest.split_whitespace();
    let _time = fields.next()?;
    fields.next()?.parse().ok()
}

fn split_two(rest: &str) -> (String, String) {
    let mut parts = rest.splitn(2, ' ');
    let first = parts.next().unwrap_or_default();
    let second = parts.next().unwrap_or_default();
    (first.trim().to_owned(), second.trim_start().to_owned())
}

/// A long-lived `tmux -C` child, kept across questions instead of forking a
/// tmux process per command the way `bus` does.
pub struct ControlClient {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// tmux emits an empty attach block on connect before any command; the
    /// first command must skip it.
    first_block: bool,
}

impl ControlClient {
    /// Spawn `tmux [-L socket] -C`. `-C` (not `-CC`) keeps the terminal in
    /// canonical mode with echo on; `boop` is not a terminal emulator and must
    /// not change terminal attributes.
    pub fn spawn(socket: Option<&str>) -> Result<Self> {
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        builder.arg("-C");
        let mut child = builder
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("spawn tmux -C; is tmux installed and reachable?")?;
        let stdin = child.stdin.take().context("tmux control stdin")?;
        let stdout = child.stdout.take().context("tmux control stdout")?;
        Ok(ControlClient {
            _child: child,
            stdin,
            stdout: BufReader::new(stdout),
            first_block: true,
        })
    }

    /// Send one command, block for its `%begin`/`%end` (or `%error`) block,
    /// return the body. A `%error` block is a returned `Err`, never a panic
    /// and never a silent empty result. Blocks are paired by the command
    /// number tmux assigns (which is server-global and not predictable), and
    /// the first pair on a fresh connection is tmux's own attach block.
    pub fn command(&mut self, argv: &[&str]) -> Result<Vec<String>> {
        let line = argv.iter().map(|arg| quote_arg(arg)).collect::<Vec<_>>().join(" ");
        writeln!(self.stdin, "{line}").context("write tmux control command")?;
        self.stdin.flush().context("flush tmux control stdin")?;

        let mut open: Option<usize> = None;
        let mut body: Vec<String> = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let line = next_line(&mut self.stdout, deadline)?;
            match parse_event(&line) {
                ControlEvent::BlockBegin { num } => {
                    open = Some(num);
                    body.clear();
                }
                ControlEvent::BlockEnd { num } if Some(num) == open => {
                    if self.first_block {
                        self.first_block = false;
                        open = None;
                        continue;
                    }
                    return Ok(body);
                }
                ControlEvent::BlockError { num } if Some(num) == open => {
                    if self.first_block {
                        self.first_block = false;
                        open = None;
                        continue;
                    }
                    anyhow::bail!("tmux command failed: {}", argv.join(" "));
                }
                ControlEvent::BlockEnd { .. } | ControlEvent::BlockError { .. } => {}
                ControlEvent::Body(text) => {
                    if open.is_some() {
                        body.push(text);
                    }
                }
                ControlEvent::Notification(_) => {}
            }
        }
    }
}

/// Quote an argv item for tmux's command-line parser when it contains a
/// character that would otherwise be parsed specially (a leading `#` begins a
/// comment, so format strings must be quoted).
fn quote_arg(arg: &str) -> String {
    if arg.contains([' ', '\t', '#', '"', '\'', '{', '}']) {
        format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        arg.to_owned()
    }
}

/// Read the next line from the control stream. `ChildStdout` has no timeout,
/// so a read that reaches the deadline errors instead of hanging forever.
fn next_line(reader: &mut BufReader<ChildStdout>, deadline: std::time::Instant) -> Result<String> {
    let mut buffer = Vec::new();
    loop {
        if std::time::Instant::now() > deadline {
            anyhow::bail!("tmux control mode timed out");
        }
        let read = reader.fill_buf().context("read tmux control stdout")?;
        if read.is_empty() {
            anyhow::bail!("tmux control process closed its stdout");
        }
        match read.iter().position(|byte| *byte == b'\n') {
            Some(index) => {
                buffer.extend_from_slice(&read[..index]);
                reader.consume(index + 1);
                return Ok(String::from_utf8_lossy(&buffer).into_owned());
            }
            None => {
                buffer.extend_from_slice(read);
                let len = read.len();
                reader.consume(len);
            }
        }
    }
}

/// One-shot `tmux list-sessions -F '#{session_name}'`. Returns `None` when tmux
/// itself is unreachable, which is NOT the same as "no sessions".
pub fn live_sessions(socket: Option<&str>) -> Option<LiveSessions> {
    let mut builder = Command::new("tmux");
    if let Some(socket) = socket {
        builder.arg("-L").arg(socket);
    }
    builder.args(["list-sessions", "-F", "#{session_name}"]);
    let output = builder.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let mut names = LiveSessions::default();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if !line.trim().is_empty() {
            names.names.insert(line.trim().to_owned());
        }
    }
    Some(names)
}

#[derive(Default)]
pub struct LiveSessions {
    pub names: std::collections::BTreeSet<String>,
}

impl LiveSessions {
    pub fn has(&self, session: &str) -> bool {
        self.names.contains(session)
    }
}

/// One-shot `tmux has-session -t <name>` via `tmux_interface`.
pub fn has_session(socket: Option<&str>, session: &str) -> Result<bool> {
    use tmux_interface::{HasSession, Tmux};
    let builder = Tmux::new();
    let builder = match socket {
        Some(socket) => builder.socket_name(socket),
        None => builder,
    };
    let status = builder
        .add_command(HasSession::new().target_session(session))
        .status()
        .context("tmux has-session")?;
    Ok(status.success())
}

/// One-shot `tmux kill-session -t <name>` via `tmux_interface`.
pub fn kill_session(socket: Option<&str>, session: &str) -> Result<()> {
    use tmux_interface::{KillSession, Tmux};
    let builder = Tmux::new();
    let builder = match socket {
        Some(socket) => builder.socket_name(socket),
        None => builder,
    };
    builder
        .add_command(KillSession::new().target_session(session))
        .output()
        .context("tmux kill-session")?;
    Ok(())
}

/// Send a literal line then Enter into a pane. `-l` types the body literally,
/// never as a key name; the Enter is a separate call for the same reason.
pub fn send_keys_literal(socket: Option<&str>, pane: &str, body: &str) -> Result<()> {
    send_keys(socket, &["-t", pane, "-l", "--", body])?;
    send_keys(socket, &["-t", pane, "Enter"])
}

fn send_keys(socket: Option<&str>, argv: &[&str]) -> Result<()> {
    let mut builder = Command::new("tmux");
    if let Some(socket) = socket {
        builder.arg("-L").arg(socket);
    }
    builder.arg("send-keys");
    builder.args(argv);
    let output = builder.output().context("tmux send-keys")?;
    if !output.status.success() {
        anyhow::bail!("tmux send-keys failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(())
}

/// Spawn a detached tmux session with a shell command via `tmux_interface`.
pub fn new_detached_session(
    socket: Option<&str>,
    name: &str,
    cwd: &str,
    command: &str,
) -> Result<()> {
    use tmux_interface::{NewSession, Tmux};
    let builder = Tmux::new();
    let builder = match socket {
        Some(socket) => builder.socket_name(socket),
        None => builder,
    };
    let new = NewSession::new()
        .detached()
        .start_directory(cwd)
        .session_name(name)
        .shell_command(command);
    builder
        .add_command(new)
        .output()
        .context("tmux new-session")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{parse_event, ControlClient, ControlEvent, Notification, live_sessions};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    /// A session this test owns; kills it on drop. Never touches a session it
    /// did not create.
    struct OwnedSession {
        name: String,
    }

    impl OwnedSession {
        fn create() -> OwnedSession {
            let name = format!(
                "boop-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let status = Command::new("tmux")
                .args(["new-session", "-d", "-s", &name])
                .status()
                .expect("tmux installed and reachable to create the test session");
            assert!(status.success(), "failed to create test session {name}");
            OwnedSession { name }
        }
    }

    impl Drop for OwnedSession {
        fn drop(&mut self) {
            let _ = Command::new("tmux")
                .args(["kill-session", "-t", &self.name])
                .status();
        }
    }

    #[test]
    fn lists_the_created_session_inside_one_block() {
        let owned = OwnedSession::create();
        let mut client = ControlClient::spawn(None).unwrap();
        let body = client
            .command(&["list-sessions", "-F", "#{session_name}"])
            .unwrap();
        assert!(
            body.iter().any(|line| line.trim() == owned.name),
            "expected {0} in {1:?}",
            owned.name,
            body
        );
    }

    #[test]
    fn a_failing_command_returns_err_from_error_block() {
        let _owned = OwnedSession::create();
        let mut client = ControlClient::spawn(None).unwrap();
        let result = client.command(&["list-sessions", "-t", "boop-no-such-session"]);
        assert!(result.is_err(), "expected an Err from %error, got {result:?}");
    }

    #[test]
    fn two_commands_match_their_replies_by_number() {
        let owned = OwnedSession::create();
        let mut client = ControlClient::spawn(None).unwrap();
        let first = client
            .command(&["list-sessions", "-F", "#{session_name}"])
            .unwrap();
        let second = client
            .command(&["list-sessions", "-F", "#{session_name}"])
            .unwrap();
        assert!(!first.is_empty());
        assert!(!second.is_empty());
        assert!(first.iter().any(|line| line.trim() == owned.name));
        assert!(second.iter().any(|line| line.trim() == owned.name));
    }

    #[test]
    fn an_unknown_percent_line_is_a_kept_notification() {
        // A fake future tmux notification must not be dropped or misparsed.
        assert_eq!(
            parse_event("%some-future-notification foo bar"),
            ControlEvent::Notification(Notification::Unknown("%some-future-notification foo bar".into()))
        );
    }

    #[test]
    fn unreachable_is_not_the_same_as_empty() {
        // A socket with no server behind it makes tmux fail: None (unreachable).
        let nonexistent = format!("boop-nosock-{}", std::process::id());
        assert!(live_sessions(Some(&nonexistent)).is_none());
        // The default server is reachable, even though that says nothing about
        // how many sessions exist: Some, never None.
        assert!(live_sessions(None).is_some());
    }
}
