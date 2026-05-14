//! Op-level classification consumed by the fuser.
//!
//! Adding a new lifting op = implementing `Op::classify` on the op
//! type. No central registry edit, no library change. The fuser walks
//! `&[OpCall]` for a rule body, calls `classify` on each, picks one of
//! three regimes per body (full-SQL, streamed-Rust) and emits the
//! fused statements.
//!
//! Three lifting variants:
//!
//!   Pure       — pure SQL projection or filter fragment.
//!   Stream     — runs in Rust during streamed-Rust regime; provides
//!                row schema (column → IdKind) so the rule's prepared
//!                INSERT can be built once.
//!   RuleQuery  — query against another rule's `<rule>_facts` table.
//!
//! Note: there is no `Source` variant and no separate `Sink` variant.
//! Source-like ops (fs / repo / rev / ast) are Stream because they
//! produce rows in Rust; sinks are implicit in the rule's terminal
//! FactWrite, which the fuser owns directly.

use std::sync::Arc;

/// Canonical column-name type carried in Liftable schemas and binds.
pub type Col = Arc<str>;

/// Refinement on what kind of id a column carries. Mirrors the
/// `TypeLattice` id refinements; carried here so the fuser can pick
/// the SQL column type (`INTEGER NOT NULL REFERENCES <intern_table>`)
/// without re-walking the TermFlowGraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdKind {
    StringId,
    WhereBytesId,
    FileId,
    PathId,
    BlobId,
    Int,
    Bool,
    Float,
}

/// How a Stream op writes rows during streamed-Rust regime. Function
/// pointer with a stable signature so we can store it in the variant
/// without a generic parameter. Today the fuser owns the actual
/// dispatch; this type exists so the variant has a non-zero-cost slot
/// the runner can call when the fuser decides streamed-Rust regime.
pub type StreamFn = fn();

/// How a literal arg pins a rule-query column.
#[derive(Clone, Debug)]
pub enum TermBind {
    /// `r(X?)` — projection binder; the rule's column flows out as the
    /// named capture.
    Bind(Arc<str>),
    /// `r(X)` — capture used as join key into the queried col.
    Read(Arc<str>),
    /// `r("literal")` — literal compared against the queried col;
    /// fuser pre-interns at compile time (P3).
    Literal(Arc<str>),
    /// `r(:tag)` — atom-shape literal (kept distinct from string
    /// literal so the fuser can apply intern policy by source shape).
    Atom(Arc<str>),
}

/// Per-op classification. The fuser branches on this enum to pick the
/// regime and emit fused SQL.
pub enum Liftable {
    /// Pure SQL fragment. `project` lists output columns (capture
    /// names) and the SQL expression to emit. `filter` is an optional
    /// WHERE-clause contribution. `udfs` names any UDFs the fragment
    /// depends on (e.g. `sprf_re_match`).
    Pure {
        project: Vec<(Col, String)>,
        filter: Option<String>,
        udfs: Vec<Arc<str>>,
    },
    /// Streamed in Rust; cannot lift to SQL. Schema is the row shape
    /// the prepared insert is built against. `run` is the per-row
    /// callback (today a placeholder fn pointer — the fuser invokes
    /// the op directly when emitting a streamed-Rust regime).
    Stream {
        schema: Vec<(Col, IdKind)>,
        run: StreamFn,
    },
    /// Query against another rule's `<rule>_facts` table. `table` is
    /// the FQ table name (e.g. `hits_facts`). `binds` describes per-
    /// column shape so the fuser can compose join + projection
    /// clauses.
    RuleQuery {
        table: Arc<str>,
        binds: Vec<(Col, TermBind)>,
    },
    /// Anything that doesn't lift — apply forms, side effects with
    /// observable state, etc. The fuser refuses to compose around it
    /// today; future fusers may treat Opaque as a "stream-with-no-
    /// schema-guarantees" rung.
    Opaque,
}

impl std::fmt::Debug for Liftable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Liftable::Pure { project, filter, udfs } => f
                .debug_struct("Pure")
                .field("project", project)
                .field("filter", filter)
                .field("udfs", udfs)
                .finish(),
            Liftable::Stream { schema, .. } => f
                .debug_struct("Stream")
                .field("schema", schema)
                .finish(),
            Liftable::RuleQuery { table, binds } => f
                .debug_struct("RuleQuery")
                .field("table", table)
                .field("binds", binds)
                .finish(),
            Liftable::Opaque => f.write_str("Opaque"),
        }
    }
}

/// Trait every op type that wants to participate in fusing
/// implements. Distinct from the lower-time `OperatorDef`: that one
/// tells the walker how to parse / lower a call into a `Pipe`; this
/// one tells the fuser how the op behaves in SQL-fusion terms.
pub trait Op {
    fn classify(&self) -> Liftable;
}

/// Default placeholder for `StreamFn`. The fuser hooks the real
/// runner when emitting; the value here only matters for unit tests
/// that pattern-match the variant.
pub fn stream_noop() {}
