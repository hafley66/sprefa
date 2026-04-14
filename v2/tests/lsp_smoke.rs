//! LSP bin smoke test.
//!
//! Spawns the sprefa-v2-lsp binary as a subprocess, sends JSON-RPC initialize +
//! didOpen + hover over stdio, reads the hover response, and checks the markdown
//! prefix. Keeps the wire format real without hand-rolling tower-lsp client.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn framed(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
}

/// Read one LSP message (Content-Length framed) from `r`, returning its JSON body.
fn read_message<R: BufRead>(r: &mut R) -> Option<String> {
    let mut content_len: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line).ok()?;
        if n == 0 { return None; }
        let trimmed = line.trim_end_matches(|c| c == '\r' || c == '\n');
        if trimmed.is_empty() { break; }
        if let Some(v) = trimmed.strip_prefix("Content-Length: ") {
            content_len = v.trim().parse().ok();
        }
    }
    let len = content_len?;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

#[test]
fn lsp_bin_hover_smoke() {
    // Locate binary next to current test binary in target/debug/.
    // cargo test sets CARGO_BIN_EXE_<name> for bins in the same package.
    let bin = env!("CARGO_BIN_EXE_sprefa-v2-lsp");

    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sprefa-v2-lsp");

    let mut stdin  = child.stdin.take().unwrap();
    let stdout     = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // --- initialize ---
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#;
    stdin.write_all(framed(init).as_bytes()).unwrap();

    let init_resp = read_message(&mut reader).expect("initialize response");
    assert!(init_resp.contains("capabilities"), "got: {init_resp}");

    // --- initialized notification ---
    let initd = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
    stdin.write_all(framed(initd).as_bytes()).unwrap();

    // --- didOpen ---
    // Source: `rule(test_rule){ > repo(*) > rev(main) }`
    // Hover at offset of the `repo` op name.
    let source = "rule(test_rule){ > repo(*) > rev(main) }";
    let repo_offset = source.find("repo").unwrap();
    // character offset on line 0 (ASCII, so byte == char).
    let did_open = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///test.sprf","languageId":"sprf","version":1,"text":"{}"}}}}}}"#,
        source
    );
    stdin.write_all(framed(&did_open).as_bytes()).unwrap();

    // didOpen triggers publishDiagnostics; consume it.
    let _ = read_message(&mut reader);

    // --- hover ---
    let hover = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{{"textDocument":{{"uri":"file:///test.sprf"}},"position":{{"line":0,"character":{}}}}}}}"#,
        repo_offset
    );
    stdin.write_all(framed(&hover).as_bytes()).unwrap();

    // Hover response may interleave with further publishDiagnostics; loop until id:2 arrives.
    let mut found = None;
    for _ in 0..4 {
        match read_message(&mut reader) {
            Some(msg) if msg.contains("\"id\":2") => { found = Some(msg); break; }
            Some(_)  => continue,
            None     => break,
        }
    }

    // --- shutdown ---
    let shutdown = r#"{"jsonrpc":"2.0","id":99,"method":"shutdown"}"#;
    let _ = stdin.write_all(framed(shutdown).as_bytes());
    let exit = r#"{"jsonrpc":"2.0","method":"exit"}"#;
    let _ = stdin.write_all(framed(exit).as_bytes());
    drop(stdin);
    let _ = child.wait_timeout_or_kill(Duration::from_secs(5));

    let msg = found.expect("no hover response with id=2");
    // RepoOp.hover_self() returns "# repo(*)" for glob literal "*".
    assert!(msg.contains("repo"), "hover response missing `repo`: {msg}");
}

// wait_timeout polyfill — stdlib has no built-in timeout on Child::wait.
trait WaitOrKill {
    fn wait_timeout_or_kill(&mut self, dur: Duration);
}
impl WaitOrKill for std::process::Child {
    fn wait_timeout_or_kill(&mut self, dur: Duration) {
        let start = std::time::Instant::now();
        loop {
            match self.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => {
                    if start.elapsed() > dur { let _ = self.kill(); let _ = self.wait(); return; }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => { let _ = self.kill(); return; }
            }
        }
    }
}
