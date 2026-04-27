//! Effect kind: parse one JS/TS file with oxc.
//!
//! Response is a projection, not the AST: oxc's `Program<'arena>` is
//! tied to an `Allocator` lifetime and is not `Send`. The batcher
//! allocates a fresh arena per call, parses, records a summary, and
//! drops the arena before returning. Downstream ops that only care
//! about byte count, error count, or an owned import edge vec read
//! the response directly. Ops that need the AST must colocate their
//! traversal inside an inner closure (see `run_with_ast`).

use effect_runtime::{Batcher, BoxFuture, CancellationToken, EffectKind};
use std::path::PathBuf;
use std::sync::Arc;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

pub struct OxcParse {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub struct OxcParseSummary {
    pub bytes: u64,
    pub errors: u64,
    pub panicked: bool,
}

impl EffectKind for OxcParse {
    type Response = OxcParseSummary;
    fn payload_bytes(&self) -> Option<usize> { Some(0) }
    fn response_bytes(r: &Self::Response) -> Option<usize> { Some(r.bytes as usize) }
}

pub struct OxcParser_;

impl Batcher<OxcParse> for OxcParser_ {
    fn run(
        &self,
        req: OxcParse,
        _cancel: CancellationToken,
    ) -> BoxFuture<'static, OxcParseSummary> {
        Box::pin(async move {
            parse_one(&req.path)
        })
    }
}

/// Batch variant: one effect put, handler fans internally via rayon.
pub struct OxcParseBatch {
    pub paths: Arc<Vec<PathBuf>>,
}

impl EffectKind for OxcParseBatch {
    type Response = (u64, u64, u64); // (bytes, errors, files)
    fn payload_bytes(&self) -> Option<usize> { Some(0) }
    fn response_bytes(r: &Self::Response) -> Option<usize> { Some(r.0 as usize) }
}

pub fn parse_one(path: &std::path::Path) -> OxcParseSummary {
    let Ok(src) = std::fs::read_to_string(path) else {
        return OxcParseSummary { bytes: 0, errors: 0, panicked: false };
    };
    let bytes = src.len() as u64;
    let alloc = Allocator::default();
    let st = SourceType::from_path(path).unwrap_or_default();
    let ret = Parser::new(&alloc, &src, st).parse();
    OxcParseSummary {
        bytes,
        errors: ret.errors.len() as u64,
        panicked: ret.panicked,
    }
}
