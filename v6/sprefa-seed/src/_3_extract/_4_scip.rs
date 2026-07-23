//! SCIP — the compiler-backed Tier-1 source. "diet scip": we keep ONLY the
//! fields that project onto the four families (symbol + range + role + the four
//! relationship booleans), and drop the rest (hover docs, markdown, full type
//! signatures we do not consume). Sourcegraph's format, not their runtime.
//!
//! Boundary law (owner ruling, language-interfaces plan): SCIP indexers are
//! FOREIGN TOOLS behind a subprocess/IPC seam — `rust-analyzer scip`,
//! `scip-typescript`, `scip-python`, `scip-go`, `scip-java`, `scip-clang`. We
//! NEVER build a SCIP indexer and NEVER bespoke-FFI a compiler. `ScipSource`
//! shells out, parses the protobuf, projects to `_0_shape` rows, discards the
//! tree. The index file is reload-gated by mtime (v5 `scip.rs:99-110` `dirty()`).
//!
//! Merge precedence (the typed rule, not a heuristic): when a SCIP index is
//! present for a language, SCIP definitions/references are GROUND TRUTH for the
//! call/type/module resolution families — they override native-AST name
//! resolution (v5 `scip_ref` override, `std/flow.dl:96-114`). The native AST
//! still owns DATAFLOW (SCIP carries no CFG/DDG) and owns every span's byte
//! coordinate when SCIP's range needs anchoring. df is never SCIP-sourced.
//!
//! Prior art: the OccurrenceRole bitfield + Symbol grammar are lifted from the
//! SCIP proto verbatim (scip-code/scip/scip.proto); the role vocabulary matches
//! Kythe's defines/ref/ref/writes edge kinds 1:1.

use crate::_3_extract::_0_shape::{BlobHash, NameId, Span};

/// A SCIP occurrence: one (symbol, span, role) triple — a definition or a
/// reference site. Diet: we keep `symbol`, `span`, `role`, and the syntax kind
/// for call-site detection; we drop `diagnostics`, `enclosing_range` (recomputed
/// from containment), and the redundant range encodings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScipOccurrence {
    pub symbol: ScipSymbol,   // the resolved identity (a stable cross-file string)
    pub span: Span,           // content-localized by the native parser's line index
    pub role: OccurrenceRole, // bitfield: def/ref/import/read/write/...
}

/// A SCIP symbol information: what a symbol IS. Diet: keep kind + display_name +
/// signature_text; drop markdown docs + the nested signature_documentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScipSymbol {
    pub scheme: NameId,          // the stable symbol string, arena-interned
    pub kind: ScipSymbolKind,    // maps onto our NodeKind families
    pub display_name: NameId,    // the last identifier run (v5 scip_name)
}

/// The SCIP `SymbolRole` bitfield, lifted from the proto. Composes (a definition
/// that is also a write). Stored as one i32 on the wire + in the store; the
/// bitset is the kind vocabulary for occurrence edges.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct OccurrenceRole(pub i32);

impl OccurrenceRole {
    pub const DEFINITION: Self   = Self(0x1);
    pub const IMPORT: Self       = Self(0x2);
    pub const WRITE_ACCESS: Self = Self(0x4);
    pub const READ_ACCESS: Self  = Self(0x8);
    pub const GENERATED: Self    = Self(0x10);
    pub const TEST: Self         = Self(0x20);
    pub const FORWARD_DEF: Self  = Self(0x40);
    pub fn contains(self, bit: Self) -> bool { (self.0 & bit.0) != 0 }
}

/// The v6-relevant slice of SCIP's 87-value `SymbolInformation.Kind`. The full
/// enum is the indexer's business; diet-scip maps every incoming kind onto one
/// of these, which then maps 1:1 onto `NodeKind` (call/type) + the role bits.
/// (df has no SCIP kind — dataflow is never SCIP-sourced.)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ScipSymbolKind {
    Function, Method, Constructor, Getter, Setter,    // -> CallKind / call family
    Field, Property, Parameter, Local,                 // -> DfNodeKind (var_read/write/param)
    Struct, Class, Interface, Trait, Enum, EnumMember, // -> TypeEntityKind
    TypeAlias, TypeParameter, Module, Namespace,       // -> TypeEntityKind::Alias / Module
    Constant, Macro, Other,                            // -> TypeEntityKind::Const / fallback
}

/// A SCIP relationship: four booleans (the proto uses bools, not a kind enum).
/// `is_implementation` drives `type_edge kind=impl` + `scip_impl`; the others
/// drive find-references / go-to-type-def fan-out.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ScipRelation {
    pub target: ScipSymbol,
    pub is_reference: bool,
    pub is_implementation: bool,
    pub is_type_definition: bool,
    pub is_definition: bool,
}

/// One parsed index.scip: documents + externally-referenced symbols. The unit a
/// `ScipSource` produces from one indexer run.
#[derive(Clone, Debug, Default)]
pub struct ScipIndex {
    pub documents: Vec<ScipDocument>,
    pub external_symbols: Vec<ScipSymbol>,
}

#[derive(Clone, Debug)]
pub struct ScipDocument {
    pub relative_path: NameId,
    pub blob: BlobHash,                // joined to our content-addressed files
    pub encoding: PositionEncoding,    // SCIP lets each doc pick; we normalize to UTF-8 bytes
    pub occurrences: Vec<ScipOccurrence>,
    pub symbols: Vec<ScipSymbol>,
    pub relations: Vec<ScipRelation>,
}

/// SCIP `PositionEncoding`. Indexers pick by implementation language
/// (Rust/C++/Go = UTF8ByteOffset). We normalize to UTF-8 byte spans on ingest.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PositionEncoding { Utf8Byte, Utf16CodeUnit, Utf32CodeUnit }

// ── the shell-out source ────────────────────────────────────────────────────

/// The Tier-1 source: build or load a SCIP index for one repo. NEVER a bespoke
/// indexer, NEVER compiler FFI. Two ops: `build` runs the foreign indexer
/// (subprocess; `scip_setup.rs:51` INDEXERS), `load` parses an existing
/// index.scip (v5 `scip_import::load`). Both are SYNC + CPU-bound; rayon can fan
/// the per-document projection across workers after the index is in hand.
pub trait ScipSource: Sync {
    fn indexer(&self) -> &'static str;                      // e.g. "scip-typescript"
    /// Run the foreign indexer over `root`, writing index.scip. Shells out.
    fn build(&self, root: &std::path::Path) -> Result<(), ScipError>;
    /// Parse index.scip into a diet `ScipIndex`. The protobuf parse lives here.
    fn load(&self, index_path: &std::path::Path) -> Result<ScipIndex, ScipError>;
}

#[derive(Debug)]
pub enum ScipError {
    IndexerMissing(&'static str),  // the foreign tool is not installed
    IndexerFailed(i32),            // non-zero exit from the subprocess
    Parse(&'static str),           // protobuf decode failure
    StaleIndex,                    // mtime says the index predates the source
}
