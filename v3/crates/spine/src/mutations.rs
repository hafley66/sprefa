//! `mutations` — trait surface for deferred write effects.
//!
//! Ops construct `Arc<dyn MutationEffect>` and queue it via a
//! `MutationRequest` on an mpsc channel. The handler half of the
//! pipeline (`AutoApprove`, `InteractiveCli`, `LspPromptBridge`,
//! `spawn_handler`, `await_approval`) lives in the `sprefa` crate
//! because it depends on the concrete `Store` impl for effect-cache
//! checks. Only the trait / request side lives here so ops can
//! construct effects without pulling sqlite.
//!
//! **Who reaches for this:**
//!   - op plugins — implement `MutationEffect` for fs/shell/render
//!     ops, construct `MutationRequest` values.
//!   - `sprefa::mutations` — re-exports + handler implementations and
//!     the `await_approval` dispatcher.
//!
//! **Cardinality:** mutation requests per run range from zero (pure
//! read queries) to thousands (bulk rename / codemod runs). Each
//! request allocates one oneshot channel for ack.

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use sprefa_parse::ParseSite;

use crate::reader::Reader;
use crate::writer::Writer;
use crate::config::Config;

// ---------------------------------------------------------------------------
// Effect cache result types
// ---------------------------------------------------------------------------
//
// Returned by `Store::effect_status` in the sprefa runtime, but the
// Store trait itself stays in sprefa (because the concrete sqlite impl
// lives there). Ops only touch these types to decide what to do with
// an effect's prior disposition; keeping them here lets
// `MutationEffect::apply` return `EffectOutcome` without a
// spine→sprefa dep reversal.

/// Decision the effect cache hands back for a given `MutationEffect`.
///
/// - `Skip`  — identical effect already applied; don't prompt, don't
///             re-apply. Approval flow short-circuits to `Yes`.
/// - `Emit`  — new effect; prompt the handler; apply on approval.
/// - `Stale` — prior outcome exists but source content changed; prompt
///             the handler with the stale context attached.
///
/// **Cardinality:** one per mutation request per run.
#[derive(Debug, Clone)]
pub enum EffectStatus {
    Skip,
    Emit,
    Stale(EffectOutcome),
}

/// Persisted record of what happened when an effect was applied (or
/// why it was not). `effect_hash` pins the exact effect that ran.
///
/// **Cardinality:** one per applied effect across the workspace's
/// entire history. Stored in SQLite; retrieved during effect-status
/// checks.
#[derive(Debug, Clone)]
pub struct EffectOutcome {
    pub result:      EffectResult,
    pub when:        DateTime<Utc>,
    pub effect_hash: Arc<str>,
}

/// Terminal disposition of an effect. `Superseded` means a later run
/// already reapplied the same logical effect with a fresher content
/// hash — the older outcome is kept for audit but isn't live.
///
/// **Cardinality:** three variants. Serialized to TEXT in SQLite via
/// `as_sql_str` / `from_str`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectResult {
    Applied,
    Rejected,
    Superseded,
}

impl EffectResult {
    /// Parse from SQL column value. Unknown strings collapse to
    /// `Superseded` (treat as historical / unreliable).
    pub fn from_str(s: &str) -> Self {
        match s {
            "Applied"  => Self::Applied,
            "Rejected" => Self::Rejected,
            _          => Self::Superseded,
        }
    }
    /// Stable SQL column string. Matches `from_str` round-trip.
    pub fn as_sql_str(self) -> &'static str {
        match self {
            Self::Applied    => "Applied",
            Self::Rejected   => "Rejected",
            Self::Superseded => "Superseded",
        }
    }
}

// ---------------------------------------------------------------------------
// Approval primitives
// ---------------------------------------------------------------------------

/// Two-valued approval reply sent back to the op's `await_approval`
/// call via the oneshot channel.
///
/// **Cardinality:** one per mutation request.
pub enum Approve {
    Yes,
    No,
}

/// Cancellation sentinel returned by `await_approval` when the request
/// is dropped (channel closed, cancel token fired, ack sender
/// dropped). Ops should propagate as an early return, not retry.
///
/// **Cardinality:** zero-sized; costless to construct.
pub struct Cancelled;

/// Error produced by `MutationEffect::apply` when the effect fails
/// during execution (as opposed to being skipped by cache or rejected
/// by the user).
///
/// **Cardinality:** one per failed apply.
pub struct EffectErr {
    pub code: &'static str,
    pub msg:  String,
}

/// Capabilities passed to `MutationEffect::apply`. Keeps the trait's
/// `apply` signature stable while the runtime decides which Reader /
/// Writer / Config snapshot to hand in.
///
/// **Cardinality:** one per apply call. Cheap — three Arc clones.
/// **Why three handles:** effects may read source (reader), write
/// persistence (writer), and consult config (e.g., allowlist checks
/// against `Config.shell_allow`).
pub struct MutationScope {
    pub reader: Arc<dyn Reader>,
    pub writer: Arc<dyn Writer>,
    pub config: Arc<Config>,
}

/// One mutation prompt in flight. Carried across the mpsc channel
/// between `await_approval` and the active `MutationHandler`.
///
/// **Cardinality:** one per queued mutation. Transient — lives from
/// queue enqueue to ack send.
/// **Allocation:** one oneshot per request for the ack path.
pub struct MutationRequest {
    pub effect: Arc<dyn MutationEffect>,
    pub ack:    oneshot::Sender<Approve>,
    pub cancel: CancellationToken,
    pub expr:   Option<Arc<str>>,
    pub site:   Arc<ParseSite>,
}

// ---------------------------------------------------------------------------
// The two trait surfaces
// ---------------------------------------------------------------------------

/// An effect the runtime may apply to the outside world — file edit,
/// shell call, render-into-file, external API call, etc. Ops
/// implement this per-effect-kind.
///
/// **Who implements:** `sprefa::ops::*` authors. Each op that can
/// queue a mutation defines one or more concrete effect structs.
/// **Cardinality:** one trait-object allocation per queued effect
/// (boxed as `Arc<dyn MutationEffect>`).
/// **Why object-safe:** the runtime holds `Arc<dyn MutationEffect>`
/// in `MutationRequest` without knowing the concrete type.
#[async_trait]
pub trait MutationEffect: Send + Sync + Debug + 'static {
    /// Stable discriminator for this effect kind. Drives logging
    /// (`EffectLogRow.kind`) and diagnostic codes.
    fn kind_sigil(&self)         -> &'static str;
    /// Rendered markdown preview shown to the user / LSP for approval.
    /// Should be self-contained: no Arc references outside the
    /// effect's own fields.
    fn preview_markdown(&self)   -> String;
    /// Stable hash over this effect's content. Used by the cache to
    /// detect "same effect as before". Must be pure of runtime state
    /// (no timestamps, no random IDs).
    fn fingerprint(&self)        -> Arc<str>;
    /// Returns true if the effect's inputs haven't changed since the
    /// given timestamp. Default false (always re-evaluate).
    fn content_stable_since(&self, _: chrono::DateTime<chrono::Utc>) -> bool { false }
    /// Execute the effect. Called only after approval (or cache skip).
    /// `scope` provides the reader / writer / config handles.
    async fn apply(&self, scope: MutationScope) -> Result<EffectOutcome, EffectErr>;
}

/// The approval side of the mutation pipeline. Drains
/// `MutationRequest`s off the mpsc and ultimately answers each one
/// via its `ack` channel.
///
/// **Who implements:** `sprefa::mutations::{AutoApprove,
/// InteractiveCli, LspPromptBridge}`. One impl per interaction mode.
/// **Cardinality:** one live handler per run. Runs in a dedicated
/// tokio task spawned by `spawn_handler`.
#[async_trait]
pub trait MutationHandler: Send + Sync {
    async fn handle(&self, req: MutationRequest);
}
