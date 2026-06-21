//! JSON-RPC 2.0 over `Content-Length`-framed messages (LSP-style framing).
//! Same codec on the daemon's local socket and on the LSP stdio bridge, so a
//! client that speaks this codec is transport-agnostic.
//!
//! Wire shape:
//!   Content-Length: N\r\n
//!   \r\n
//!   <N bytes of JSON-RPC>
//!
//! JSON-RPC 2.0 envelopes:
//!   request:       {"jsonrpc":"2.0","id":N,"method":"X","params":{...}}
//!   response (ok): {"jsonrpc":"2.0","id":N,"result":{...}}
//!   response (err):{"jsonrpc":"2.0","id":N,"error":{"code":C,"message":M}}
//!   notification:  {"jsonrpc":"2.0","method":"X","params":{...}}    (no id)

use anyhow::{Result, bail};
use std::io::{Read, Write};

/// Write one framed JSON-RPC message. `body` is the JSON string; the codec
/// stamps the `Content-Length` header and flushes.
pub fn write_frame<W: Write>(w: &mut W, body: &str) -> Result<()> {
    write!(w, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    w.flush()?;
    Ok(())
}

/// Read one framed message. Returns `Ok(None)` on clean EOF (peer closed
/// before sending any partial frame). Errors on a malformed header block.
pub fn read_frame<R: Read>(r: &mut R) -> Result<Option<String>> {
    let mut content_length: Option<usize> = None;
    let mut line = Vec::<u8>::new();
    loop {
        line.clear();
        let n = read_line(r, &mut line)?;
        if n == 0 {
            // EOF: a clean boundary if we are between frames (no Content-Length
            // seen yet), otherwise a truncated stream.
            return if content_length.is_some() || !line.is_empty() {
                bail!("unexpected EOF mid-frame")
            } else {
                Ok(None)
            };
        }
        // Strip the trailing \r\n; headers are ASCII so byte-equality is safe.
        let trimmed = trim_crlf(&line);
        if trimmed.is_empty() { break; }                  // end of header block
        if let Some(rest) = trimmed.strip_prefix(b"Content-Length:") {
            let s = std::str::from_utf8(rest)?.trim();
            content_length = Some(s.parse()?);
        }
        // Other headers (Content-Type, etc.) are ignored.
    }
    let len = content_length.ok_or_else(|| anyhow::anyhow!("missing Content-Length header"))?;
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok(Some(String::from_utf8(body)?))
}

/// Read one CRLF-terminated line into `buf` (including the trailing newline).
/// Returns the byte count; 0 means EOF before any byte was read.
fn read_line<R: Read>(r: &mut R, buf: &mut Vec<u8>) -> Result<usize> {
    let mut total = 0;
    let mut byte = [0u8; 1];
    loop {
        let n = r.read(&mut byte)?;
        if n == 0 { return Ok(total); }
        buf.push(byte[0]);
        total += 1;
        if byte[0] == b'\n' { return Ok(total); }
    }
}

fn trim_crlf(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b'\n' || bytes[end - 1] == b'\r') { end -= 1; }
    &bytes[..end]
}

// ---------- typed envelopes ----------

/// A JSON-RPC 2.0 request (has `id`). Notifications reuse this struct without
/// an id and are written via `write_notification`.
#[derive(Debug, Clone)]
pub struct Request {
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

/// A JSON-RPC 2.0 response. Exactly one of `result` / `error` is set.
#[derive(Debug, Clone)]
pub struct Response {
    pub id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

// Standard JSON-RPC error codes.
pub const PARSE_ERROR: i64      = -32700;
pub const INVALID_REQUEST: i64  = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64   = -32602;
pub const INTERNAL_ERROR: i64   = -32603;

impl Request {
    pub fn new(id: u64, method: &str, params: serde_json::Value) -> Self {
        Self { id, method: method.to_string(), params }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.id,
            "method": self.method,
            "params": self.params,
        })
    }
}

impl Response {
    pub fn ok(id: u64, result: serde_json::Value) -> Self {
        Self { id, result: Some(result), error: None }
    }
    pub fn err(id: u64, code: i64, message: impl Into<String>) -> Self {
        Self { id, result: None, error: Some(RpcError { code, message: message.into() }) }
    }
    pub fn to_json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({"jsonrpc": "2.0", "id": self.id});
        if let Some(r) = &self.result { v["result"] = r.clone(); }
        if let Some(e) = &self.error {
            v["error"] = serde_json::json!({"code": e.code, "message": e.message});
        }
        v
    }
    pub fn from_value(v: serde_json::Value) -> Result<Self> {
        let id = v.get("id").and_then(|i| i.as_u64())
            .ok_or_else(|| anyhow::anyhow!("response missing id"))?;
        let result = v.get("result").cloned();
        let error = v.get("error").map(|e| RpcError {
            code: e.get("code").and_then(|c| c.as_i64()).unwrap_or(INTERNAL_ERROR),
            message: e.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string(),
        });
        Ok(Response { id, result, error })
    }
}

/// Write a notification (no id). Used for server-sent events like
/// `diag_changed` pushed to subscribers.
pub fn write_notification<W: Write>(w: &mut W, method: &str, params: serde_json::Value) -> Result<()> {
    let v = serde_json::json!({"jsonrpc": "2.0", "method": method, "params": params});
    write_frame(w, &serde_json::to_string(&v)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(body: &str) -> String {
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, body).unwrap();
        let mut cursor = std::io::Cursor::new(buf);
        read_frame(&mut cursor).unwrap().unwrap()
    }

    #[test]
    fn frame_round_trip_ascii() {
        assert_eq!(round_trip(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#),
                   r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
    }

    #[test]
    fn frame_round_trip_utf8_body() {
        // JSON string with a multibyte em dash; frame length is bytes, not chars.
        let body = r#"{"jsonrpc":"2.0","method":"x","params":{"msg":"a — b"}}"#;
        assert_eq!(round_trip(body), body);
    }

    #[test]
    fn clean_eof_between_frames() {
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, r#"{"x":1}"#).unwrap();
        let mut cursor = std::io::Cursor::new(buf);
        assert!(read_frame(&mut cursor).unwrap().is_some());   // first frame
        assert!(read_frame(&mut cursor).unwrap().is_none());   // clean EOF
    }

    #[test]
    fn extra_headers_ignored() {
        let raw = b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\n\
                    Content-Length: 13\r\n\
                    \r\n\
                    {\"hello\":1}";
        let mut cursor = std::io::Cursor::new(&raw[..]);
        assert_eq!(read_frame(&mut cursor).unwrap().unwrap(), "{\"hello\":1}");
    }

    #[test]
    fn missing_content_length_errors() {
        let raw = b"Content-Type: x\r\n\r\n";
        let mut cursor = std::io::Cursor::new(&raw[..]);
        assert!(read_frame(&mut cursor).is_err());
    }

    #[test]
    fn response_round_trip() {
        let r = Response::ok(7, serde_json::json!({"ok": true}));
        let v = r.to_json();
        let back = Response::from_value(v).unwrap();
        assert_eq!(back.id, 7);
        assert_eq!(back.result, Some(serde_json::json!({"ok": true})));
        assert!(back.error.is_none());
    }
}
