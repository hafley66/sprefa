//! sprefa v2 — scaffold. Types + traits + pure utils. No impls yet.

pub mod data;
pub mod jq_path;
pub mod walk;

pub mod _0_types;
pub mod _1_diagnostic;
pub mod _2_config;
pub mod _3_reader;
pub mod _4_writer;
pub mod _5_op;
pub mod _6_extractor;
pub mod _7_runner;
pub mod _8_parse;
pub mod _9_hash;
pub mod _10_registry;
pub mod _11_dag;
pub mod _12_result_store;

pub mod readers;
pub mod writers;
pub mod ops;
pub mod analysis;
pub mod position;

pub use readers::{MemReader, CheckoutLocator, ConfigLocator, InMemoryLocator, GitBlobReader};
pub use writers::MemWriter;

pub use _0_types::*;
pub use _1_diagnostic::{Diagnostic, Renderer};
pub use _2_config::{Config, ConfigDiff, RuntimeConfig};
pub use _3_reader::{Reader, ParserKind, ParsedTree, ScanKind, ScanCombo, CrossRefHit, ViolationEntry};
pub use _4_writer::{
    Writer, WResult, WriterError,
    RuleTableSpec, ColumnSpec, ColumnKind, CrossFk,
    RefEntry, ExtractionRow, ProvenanceRow, RunVisit, ViolationRow, EffectLogRow,
    EffectStatus, FileEdit, ShellCall, ShellReply,
};
pub use _5_op::{
    Op, Operator, OpCtx, OpInvocation, BracketSlot, ParenSlot, BraceSlot, CrossRefOccurrence,
    BraceMode, GrammarRef, Pipeline, LoweredOp, ForkBranch, ChannelSelector,
    ProgramCtx, RuleHandle,
    DiagSink, EventSink, TokenSpan, TokenKind, HoverInfo,
    CompletionItem,
    XrefEmptyJoin, XrefCartesianLimit,
};
pub use _12_result_store::{ResultStore, RuleResult, CaptureMap};
pub use _6_extractor::{Extractor, ExtractorKind};
pub use _7_runner::Runner;
pub use _8_parse::{
    TokenClass, CaptureRef, ProvRef, ProvKind, CrossRefRef,
    parse_capture, parse_provenance, parse_cross_ref, classify_token,
    scan_balanced, host_parse, host_parse_brace, host_parse_arm_brace, ChildKind,
    ParseError, Pipe, glob_match, levenshtein,
};
pub use _9_hash::{content_hash, path_hash_static, cursor_hash};
pub use _10_registry::{OperatorRegistry, LowerOutcome, lower_rules, lower_chain};
pub use _11_dag::{RuleDag, build as build_rule_dag};

