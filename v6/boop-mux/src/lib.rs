//! Layer 1: tmux control behind one trait, `Multiplexer`. The long-lived
//! control-mode client is built here because no crate on crates.io sells one
//! (tmux_interface documents CLI-only, no `-C` guard parsing). `tmux_interface`
//! builds the one-shot command argv it can express; send-keys literal injection
//! and the control client are raw spawns because tmux_interface exposes no
//! literal-key mode.
//!
//! The trait is the seam boop binds to; `Tmux` is the one implementation. The
//! socket is a per-call argument, not trait state, because call sites mix a
//! `None` (default socket) with per-harness sockets in one process.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result};

/// The tmux multiplexer operations boop drives. Object-safe: every method takes
/// `&self` and returns a concrete or `anyhow` type.
pub trait Multiplexer {
    /// The tmux session that owns a pane. `None` when tmux is unreachable or
    /// the pane is unknown.
    fn session_of_pane(&self, socket: Option<&str>, pane: &str) -> Option<String>;
    /// The pid of the shell in the first pane of `target`.
    fn pane_pid(&self, socket: Option<&str>, target: &str) -> Option<u32>;
    /// One-shot `tmux list-sessions`. `None` means tmux itself is unreachable,
    /// which is NOT the same as "no sessions".
    fn live_sessions(&self, socket: Option<&str>) -> Option<LiveSessions>;
    /// One-shot exact `has-session` probe.
    fn has_session(&self, socket: Option<&str>, session: &str) -> Result<bool>;
    /// One-shot exact `kill-session`.
    fn kill_session(&self, socket: Option<&str>, session: &str) -> Result<()>;
    /// Whether a route's tmux target is live, judged by a direct per-target
    /// probe: exact `has-session =` for a name, `list-panes` for a pane.
    fn target_alive(&self, socket: Option<&str>, target: &str) -> bool;
    /// Capture a pane's visible region, or the last `lines` rows of history.
    fn capture_pane(
        &self,
        socket: Option<&str>,
        target: &str,
        lines: Option<u32>,
    ) -> Result<String>;
    /// Spawn a detached tmux session with a shell command.
    fn new_detached_session(
        &self,
        socket: Option<&str>,
        name: &str,
        cwd: &str,
        command: &str,
    ) -> Result<()>;
    /// Send a literal line then Enter into a pane.
    fn send_keys_literal(&self, socket: Option<&str>, pane: &str, body: &str) -> Result<()>;

    /// Send literal text with NO trailing Enter. A TUI that binds Enter to
    /// submit needs the text and the submit as separate steps.
    fn send_text(&self, socket: Option<&str>, pane: &str, body: &str) -> Result<()>;

    /// Send one tmux key name (`Enter`, `C-s`, `Escape`).
    fn send_key_named(&self, socket: Option<&str>, pane: &str, key: &str) -> Result<()>;

    /// Open a window in an existing session and return its target.
    fn new_window(
        &self,
        socket: Option<&str>,
        session: &str,
        name: &str,
        cwd: &str,
        command: &str,
    ) -> Result<String>;
}

/// The one `Multiplexer` implementation: tmux itself, driven by a mix of raw
/// spawns and `tmux_interface` one-shot builders.
pub struct Tmux;

impl Multiplexer for Tmux {
    fn session_of_pane(&self, socket: Option<&str>, pane: &str) -> Option<String> {
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        builder.args(["display-message", "-p", "-t", pane, "#{session_name}"]);
        let output = builder.output().ok()?;
        if !output.status.success() {
            return None;
        }
        let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!name.is_empty()).then_some(name)
    }

    fn pane_pid(&self, socket: Option<&str>, target: &str) -> Option<u32> {
        if !target.contains(':')
            && !self
                .has_session(socket, target)
                .ok()
                .filter(|alive| *alive)?
        {
            return None;
        }
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        let output = builder
            .args(["list-panes", "-t", target, "-F", "#{pane_pid}"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .and_then(|line| line.trim().parse().ok())
    }

    fn live_sessions(&self, socket: Option<&str>) -> Option<LiveSessions> {
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

    fn has_session(&self, socket: Option<&str>, session: &str) -> Result<bool> {
        use tmux_interface::{HasSession, Tmux};
        let builder = Tmux::new();
        let builder = match socket {
            Some(socket) => builder.socket_name(socket),
            None => builder,
        };
        let status = builder
            .add_command(HasSession::new().target_session(exact_target(session)))
            .status()
            .context("tmux has-session")?;
        Ok(status.success())
    }

    fn kill_session(&self, socket: Option<&str>, session: &str) -> Result<()> {
        use tmux_interface::{KillSession, Tmux};
        let builder = Tmux::new();
        let builder = match socket {
            Some(socket) => builder.socket_name(socket),
            None => builder,
        };
        builder
            .add_command(KillSession::new().target_session(exact_target(session)))
            .output()
            .context("tmux kill-session")?;
        Ok(())
    }

    fn target_alive(&self, socket: Option<&str>, target: &str) -> bool {
        if !target.contains(':') {
            return self.has_session(socket, target).unwrap_or(false);
        }
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        let target = exact_target(target);
        let output = builder
            .args(["list-panes", "-t", &target, "-F", "#{pane_pid}"])
            .output()
            .ok();
        matches!(output, Some(out) if out.status.success())
    }

    fn capture_pane(
        &self,
        socket: Option<&str>,
        target: &str,
        lines: Option<u32>,
    ) -> Result<String> {
        if !target.contains(':') && !self.has_session(socket, target)? {
            anyhow::bail!("no such tmux session {target}");
        }
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        builder.args(["capture-pane", "-p", "-t", target]);
        let start;
        if let Some(lines) = lines {
            start = format!("-{lines}");
            builder.args(["-S", &start]);
        }
        let output = builder.output().context("tmux capture-pane")?;
        if !output.status.success() {
            anyhow::bail!(
                "capture-pane {target}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn new_detached_session(
        &self,
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

    fn send_keys_literal(&self, socket: Option<&str>, pane: &str, body: &str) -> Result<()> {
        send_keys(socket, &["-t", pane, "-l", "--", body])?;
        send_keys(socket, &["-t", pane, "Enter"])
    }

    fn send_text(&self, socket: Option<&str>, pane: &str, body: &str) -> Result<()> {
        send_keys(socket, &["-t", pane, "-l", "--", body])
    }

    fn send_key_named(&self, socket: Option<&str>, pane: &str, key: &str) -> Result<()> {
        send_keys(socket, &["-t", pane, key])
    }

    fn new_window(
        &self,
        socket: Option<&str>,
        session: &str,
        name: &str,
        cwd: &str,
        command: &str,
    ) -> Result<String> {
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        builder.args([
            "new-window", "-d", "-P", "-F", "#{session_name}:#{window_index}", "-t", session, "-n",
            name, "-c", cwd, command,
        ]);
        let output = builder.output().context("tmux new-window")?;
        if !output.status.success() {
            anyhow::bail!(
                "new-window in {session}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

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
/// tmux process per command the way `boop bus` does.
pub struct ControlClient {
    child: Child,
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
            child,
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
        let line = argv
            .iter()
            .map(|arg| quote_arg(arg))
            .collect::<Vec<_>>()
            .join(" ");
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

/// Rust does not reap a child on drop, so without this every dropped client
/// leaves a live `tmux -C` process holding its server open.
impl Drop for ControlClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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

/// The exact-match target form. `-t name` prefix-matches a sibling session
/// (`-t boop` matches `boop-shell-v2`); `-t =name` pins to the exact name.
pub(crate) fn exact_target(name: &str) -> String {
    format!("={name}")
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
        anyhow::bail!(
            "tmux send-keys failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Kill a throwaway test server AND unlink its socket: tmux leaves the socket
/// file behind on macOS, so kill-server alone still litters /tmp.
pub fn kill_test_server(socket: &str) {
    let reported = Command::new("tmux")
        .args(["-L", socket, "display-message", "-p", "#{socket_path}"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|path| !path.is_empty());
    let _ = Command::new("tmux")
        .args(["-L", socket, "kill-server"])
        .status();
    // A dead server cannot report its path, so the convention is tried too.
    let uid = Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_default();
    let base = std::env::var("TMUX_TMPDIR").unwrap_or_else(|_| "/tmp".to_owned());
    let conventional = format!("{base}/tmux-{uid}/{socket}");
    for path in reported.into_iter().chain(std::iter::once(conventional)) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{parse_event, ControlClient, ControlEvent, Notification, Tmux};
    use crate::Multiplexer;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    /// A throwaway tmux server. Every tmux call in these tests goes through its
    /// socket; drop kills the whole server so nothing leaks onto the live one.
    struct TestServer {
        socket: String,
    }

    impl TestServer {
        /// One socket per server, never one per process: tests run in parallel
        /// and a shared socket name makes each new server kill its neighbour.
        fn new() -> TestServer {
            let socket = format!(
                "boop-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            super::kill_test_server(&socket);
            TestServer { socket }
        }

        fn create_session(&self, name: &str) {
            let status = Command::new("tmux")
                .args(["-L", &self.socket, "new-session", "-d", "-s", name])
                .status()
                .expect("tmux installed and reachable to create the test session");
            assert!(status.success(), "failed to create test session {name}");
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            super::kill_test_server(&self.socket);
        }
    }

    fn session_name() -> String {
        format!(
            "boop-s{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn mux() -> Tmux {
        Tmux
    }

    #[test]
    fn lists_the_created_session_inside_one_block() {
        let server = TestServer::new();
        let name = session_name();
        server.create_session(&name);
        let mut client = ControlClient::spawn(Some(&server.socket)).unwrap();
        let body = client
            .command(&["list-sessions", "-F", "#{session_name}"])
            .unwrap();
        assert!(
            body.iter().any(|line| line.trim() == name),
            "expected {name} in {body:?}"
        );
    }

    #[test]
    fn a_failing_command_returns_err_from_error_block() {
        let server = TestServer::new();
        let name = session_name();
        server.create_session(&name);
        let mut client = ControlClient::spawn(Some(&server.socket)).unwrap();
        let result = client.command(&["list-sessions", "-t", "boop-no-such-session"]);
        assert!(
            result.is_err(),
            "expected an Err from %error, got {result:?}"
        );
    }

    #[test]
    fn two_commands_match_their_replies_by_number() {
        let server = TestServer::new();
        let name = session_name();
        server.create_session(&name);
        let mut client = ControlClient::spawn(Some(&server.socket)).unwrap();
        let first = client
            .command(&["list-sessions", "-F", "#{session_name}"])
            .unwrap();
        let second = client
            .command(&["list-sessions", "-F", "#{session_name}"])
            .unwrap();
        assert!(first.iter().any(|line| line.trim() == name));
        assert!(second.iter().any(|line| line.trim() == name));
    }

    /// FAIL-FIRST (D7). Measured before the Drop impl: 60 live tmux processes
    /// from earlier suite runs, the oldest at 13h36m.
    #[test]
    fn a_dropped_control_client_leaves_no_tmux_process() {
        let server = TestServer::new();
        server.create_session(&session_name());
        for _ in 0..3 {
            let mut client = ControlClient::spawn(Some(&server.socket)).unwrap();
            let _ = client.command(&["list-sessions", "-F", "#{session_name}"]);
        }
        let pattern = format!("tmux -L {} -C", server.socket);
        let found = Command::new("pgrep")
            .args(["-f", &pattern])
            .output()
            .expect("pgrep available");
        let pids = String::from_utf8_lossy(&found.stdout);
        assert!(
            pids.trim().is_empty(),
            "dropped clients left tmux processes: {pids}"
        );
    }

    #[test]
    fn an_unknown_percent_line_is_a_kept_notification() {
        // A fake future tmux notification must not be dropped or misparsed.
        assert_eq!(
            parse_event("%some-future-notification foo bar"),
            ControlEvent::Notification(Notification::Unknown(
                "%some-future-notification foo bar".into()
            ))
        );
    }

    #[test]
    fn unreachable_is_not_the_same_as_empty() {
        // No server behind this socket makes tmux fail: None (unreachable).
        let nonexistent = format!("boop-test-{}-noserver", std::process::id());
        assert!(mux().live_sessions(Some(&nonexistent)).is_none());
        // A reachable server (even one with no lane of ours) is Some, never None.
        let server = TestServer::new();
        server.create_session(&session_name());
        assert!(mux().live_sessions(Some(&server.socket)).is_some());
    }

    /// RECEIPT (Job 3a). The exact `has-session =` gate pins a session-name
    /// target, so resolving `x` never reads a sibling `x2`.
    #[test]
    fn exact_target_does_not_prefix_match_a_sibling_session() {
        let server = TestServer::new();
        server.create_session("x");
        server.create_session("x2");
        let pid = mux().pane_pid(Some(&server.socket), "x").unwrap();
        assert_ne!(
            pid,
            mux().pane_pid(Some(&server.socket), "x2").unwrap(),
            "two sibling sessions must resolve distinct pids"
        );
        let server2 = TestServer::new();
        server2.create_session("x2");
        // No session literally named `x`; prefix matching would resolve to x2.
        // The exact gate must return None instead of touching x2.
        assert!(
            mux().pane_pid(Some(&server2.socket), "x").is_none(),
            "resolving `x` must not touch `x2`"
        );
        assert!(mux().pane_pid(Some(&server2.socket), "x2").is_some());
    }

    /// RECEIPT (Job 3c). Prune's liveness is a direct per-target probe: a live
    /// pane target survives, a dead session does not.
    #[test]
    fn target_alive_tracks_a_live_pane_and_drops_a_dead_session() {
        let server = TestServer::new();
        server.create_session("alive");
        assert!(
            mux().target_alive(Some(&server.socket), "alive"),
            "a live session is alive"
        );
        assert!(
            mux().target_alive(Some(&server.socket), "alive:0.0"),
            "a live adopted pane target is alive"
        );
        let status = Command::new("tmux")
            .args(["-L", &server.socket, "kill-session", "-t", "alive"])
            .status()
            .unwrap();
        assert!(status.success());
        assert!(
            !mux().target_alive(Some(&server.socket), "alive"),
            "a dead session must not survive prune"
        );
        assert!(
            !mux().target_alive(Some(&server.socket), "alive:0.0"),
            "a dead session's pane target must not survive prune"
        );
    }
}
