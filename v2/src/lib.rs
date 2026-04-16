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
pub mod _7_init_cursors;
pub mod _8_parse;
pub mod _9_hash;
pub mod _10_registry;
pub mod _11_dag;
pub mod _12_result_store;
pub mod _13_scan_check;
pub mod _14_scan_loop;
pub mod _15_pipeline_rewrite;
pub mod _16_pattern;
pub mod path_expr;
pub use _16_pattern::{CompiledPattern, PatternMatcher, Segment, compile_pattern, compile_patterns};

pub mod readers;
pub mod writers;
pub mod ops;
pub mod analysis;
pub mod position;

pub mod _task_guard;
pub mod store;
pub mod mutations;

pub use readers::{MemReader, CheckoutLocator, ConfigLocator, InMemoryLocator, GitBlobReader};
pub use writers::MemWriter;

pub use _0_types::*;
pub use _1_diagnostic::{Diagnostic, Renderer, ScanPointerUnverified, ScanPointerDepthExhausted};
pub use _13_scan_check::{check_scan_pointers, fs_path_in_tree};
pub use _14_scan_loop::{run_scan_loop, ScanLoopResult, DEFAULT_DEPTH};
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
    ExpansionMode,
};
pub use _12_result_store::{ResultStore, RuleResult, CaptureMap};
pub use _6_extractor::{Extractor, ExtractorKind};
pub use _8_parse::{
    TokenClass, CaptureRef, ScanPointerRef, CrossRefRef,
    parse_capture, parse_scan_pointer, parse_cross_ref, classify_token,
    scan_balanced, host_parse, host_parse_brace, host_parse_arm_brace,
    host_parse_tolerant, host_parse_arm_brace_abs_tolerant,
    ChildKind, ParseMode,
    ParseError, Pipe, levenshtein,
};
pub use _9_hash::{content_hash, path_hash_static, cursor_hash};
pub use _10_registry::{OperatorRegistry, LowerOutcome, lower_rules, lower_chain};
pub use _11_dag::{RuleDag, build as build_rule_dag};

