//! sprefa v3 — lifted from v2 verbatim, numeric prefixes dropped.
//! Built on top of `effect_runtime` (perf crate, redux-saga dispatch).
//!
//! Not a v2 port. v2 is reference material; this crate is where
//! op-plugin × language × core design converges. The v2 reader/writer
//! and LSP analysis layer ride along as proven substrate; ops and
//! core runtime evolve from here.

pub mod data;
pub mod jq_path;
pub mod walk;

pub mod types;
pub mod diagnostic;
pub mod config;
pub mod reader;
pub mod writer;
pub mod op;
pub mod extractor;
pub mod init_cursors;
// parse lives in the `sprefa_parse` crate. Alias so `crate::parse::X` keeps
// resolving (callers inside this crate use that path heavily).
pub use sprefa_parse as parse;
pub mod hash;
pub mod registry;
pub mod dag;
pub mod result_store;
pub mod scan_check;
pub mod scan_loop;
pub mod pipeline_rewrite;
pub mod pattern;
pub mod parallel;
pub mod str_intern;
pub mod path_expr;
pub use pattern::{CompiledPattern, PatternMatcher, Segment, compile_pattern, compile_patterns};

pub mod readers;
pub mod writers;
pub mod ops;
pub mod effects;
pub mod analysis;
pub mod position;

pub mod task_guard;
pub mod telemetry;
pub mod store;
pub mod mutations;
pub mod server;

pub use readers::{MemReader, CheckoutLocator, ConfigLocator, InMemoryLocator, GitBlobReader};
pub use writers::MemWriter;

pub use types::*;
pub use diagnostic::{Diagnostic, Renderer, ScanPointerUnverified, ScanPointerDepthExhausted};
pub use scan_check::{check_scan_pointers, fs_path_in_tree};
pub use scan_loop::{run_scan_loop, ScanLoopResult, DEFAULT_DEPTH};
pub use config::{Config, ConfigDiff, RuntimeConfig};
pub use reader::{Reader, ParserKind, ParsedTree, ScanKind, ScanCombo, CrossRefHit, ViolationEntry};
pub use writer::{
    Writer, WResult, WriterError,
    RuleTableSpec, ColumnSpec, ColumnKind, CrossFk,
    RefEntry, ExtractionRow, ProvenanceRow, RunVisit, ViolationRow, EffectLogRow,
    EffectStatus, FileEdit, ShellCall, ShellReply,
};
pub use op::{
    Op, Operator, OpCtx, OpInvocation, BracketSlot, ParenSlot, BraceSlot, CrossRefOccurrence,
    BraceMode, GrammarRef, Pipeline, LoweredOp, ForkBranch, ChannelSelector,
    ProgramCtx, RuleHandle,
    DiagSink, EventSink, TokenSpan, TokenKind, HoverInfo,
    CompletionItem,
    XrefEmptyJoin, XrefCartesianLimit,
    ExpansionMode,
};
pub use result_store::{ResultStore, RuleResult, CaptureMap};
pub use extractor::{Extractor, ExtractorKind};
pub use parse::{
    TokenClass, CaptureRef, ScanPointerRef, CrossRefRef, AtomRef,
    parse_capture, parse_scan_pointer, parse_cross_ref, parse_atom, classify_token,
    scan_balanced, host_parse, host_parse_brace, host_parse_arm_brace,
    host_parse_tolerant, host_parse_arm_brace_abs_tolerant,
    ChildKind, ParseMode,
    ParseError, Pipe, levenshtein,
};
pub use hash::{content_hash, path_hash_static, cursor_hash};
pub use registry::{OperatorRegistry, LowerOutcome, lower_rules, lower_chain};
pub use dag::{RuleDag, build as build_rule_dag};

