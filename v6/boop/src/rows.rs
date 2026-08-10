//! Typed public rows for the read surface.
//!
//! Every row is a flat set of scalar columns with no nesting: one struct maps
//! 1:1 onto one relation, so a later DL6 fixture can bind each field to a
//! column without descending. The CLI serializes these to JSON at the boundary;
//! a host links the crate and reads the typed fields directly.

use serde::Serialize;

/// One session as the `db session list` surface exposes it, least recent last.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct SessionRow {
    pub session: String,
    pub nickname: String,
    pub harness: String,
    pub cwd: Option<String>,
    pub branch: Option<String>,
    pub started_ts: Option<i64>,
    pub turns: i64,
    pub last_ts: Option<i64>,
}

/// One session that moved inside a window, with window-scoped usage totals.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct StatusRow {
    pub session: String,
    pub nickname: String,
    pub harness: String,
    pub cwd: Option<String>,
    pub parent_session: Option<String>,
    pub last_turn_ts: Option<i64>,
    pub turns: i64,
    pub calls_in_window: i64,
    pub tokens_in_window: i64,
}

/// One turn: a text block or a tool call, dense per session. `turn` and `ts`
/// together are the reconnect coordinate into the transcript stream.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct TurnRow {
    pub session: String,
    pub harness: String,
    pub turn: i64,
    pub ts: i64,
    pub role: String,
    pub said: String,
}

/// One file a tool touched. `verb` is the canonical lowercase verb; `raw_verb`
/// keeps the adapter's spelling as it arrived.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct TouchRow {
    pub session: String,
    pub turn: i64,
    pub ts: i64,
    pub path: String,
    pub verb: String,
    pub raw_verb: String,
}

/// One shell command a Bash tool ran.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct CommandRow {
    pub session: String,
    pub turn: i64,
    pub ts: i64,
    pub program: String,
    pub argline: String,
}

/// One outbound network act: a fetch (url+domain) or a search (query).
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct FetchRow {
    pub session: String,
    pub turn: i64,
    pub ts: i64,
    pub url: Option<String>,
    pub domain: Option<String>,
    pub kind: Option<String>,
    pub query: Option<String>,
}

/// One parent-child edge. `first_ts`/`last_ts`/`n` separate a single structural
/// spawn from repeated communication across the same edge.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct EdgeRow {
    pub parent: String,
    pub child: String,
    pub edge: String,
    pub first_ts: Option<i64>,
    pub last_ts: Option<i64>,
    pub n: i64,
}

/// One usage aggregate bucket. `bucket` is `None` for the totals row; grouped
/// reads set it to the group value. `cost_usd` is `None` when any call in the
/// bucket is unpriced; the priced-only total moves to `cost_usd_priced_only`.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct UsageRow {
    pub bucket: Option<String>,
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_create_5m_tokens: i64,
    pub cache_create_1h_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_usd: Option<f64>,
    pub unpriced_calls: i64,
    pub first_ts: i64,
    pub last_ts: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd_priced_only: Option<f64>,
}
