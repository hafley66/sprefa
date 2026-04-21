//! Effect kind: extract import / export edges from a JS/TS file via oxc.
//!
//! Like `OxcParse`, this returns an owned projection (not the AST) so
//! the `!Send` Allocator can be dropped before crossing the async boundary.

use effect_runtime::{Batcher, BoxFuture, CancellationToken, EffectKind};
use std::path::PathBuf;
use std::sync::Arc;

use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;
use oxc_parser::Parser;
use oxc_span::SourceType;

#[derive(Clone, Debug, Default)]
pub struct OxcImportSummary {
    pub bytes: u64,
    pub errors: u64,
    pub panicked: bool,
    /// Every static import source string (duplicates preserved).
    pub imports: Vec<String>,
    /// Re-exports with a `from` source (export … from "x").
    pub reexports: Vec<String>,
    /// Dynamic import argument literal strings we could resolve.
    pub dynamic: Vec<String>,
}

pub struct OxcImports {
    pub path: PathBuf,
}

impl EffectKind for OxcImports {
    type Response = OxcImportSummary;
    fn payload_bytes(&self) -> Option<usize> { Some(0) }
    fn response_bytes(r: &Self::Response) -> Option<usize> { Some(r.bytes as usize) }
}

pub struct OxcImportsBatcher;

impl Batcher<OxcImports> for OxcImportsBatcher {
    fn run(
        &self,
        req: OxcImports,
        _cancel: CancellationToken,
    ) -> BoxFuture<'static, OxcImportSummary> {
        Box::pin(async move { extract_one(&req.path) })
    }
}

/// Batch variant: one effect put, handler fans internally via rayon.
pub struct OxcImportsBatch {
    pub paths: Arc<Vec<PathBuf>>,
}

impl EffectKind for OxcImportsBatch {
    type Response = (u64, u64, u64); // (bytes, errors, files)
    fn payload_bytes(&self) -> Option<usize> { Some(0) }
    fn response_bytes(r: &Self::Response) -> Option<usize> { Some(r.0 as usize) }
}

pub fn extract_one(path: &std::path::Path) -> OxcImportSummary {
    let Ok(src) = std::fs::read_to_string(path) else {
        return OxcImportSummary::default();
    };
    let bytes = src.len() as u64;
    let alloc = Allocator::default();
    let st = SourceType::from_path(path).unwrap_or_default();
    let ret = Parser::new(&alloc, &src, st).parse();

    let mut summary = OxcImportSummary {
        bytes,
        errors: ret.errors.len() as u64,
        panicked: ret.panicked,
        ..OxcImportSummary::default()
    };

    for stmt in ret.program.body {
        match stmt {
            Statement::ImportDeclaration(decl) => {
                summary.imports.push(decl.source.value.to_string());
            }
            Statement::ExportNamedDeclaration(decl) => {
                if let Some(src) = &decl.source {
                    summary.reexports.push(src.value.to_string());
                }
            }
            Statement::ExportAllDeclaration(decl) => {
                summary.reexports.push(decl.source.value.to_string());
            }
            _ => {}
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn smoke_extract_imports() {
        let dir = std::env::temp_dir().join("sprefa_oxc_imports_test.ts");
        let mut f = std::fs::File::create(&dir).unwrap();
        write!(
            f,
            r#"import {{ foo }} from "bar";
import * as baz from "qux";
export {{ a }} from "reexp";
export * from "allreexp";
const x = 1;
"#
        )
        .unwrap();
        drop(f);
        let s = extract_one(&dir);
        let _ = std::fs::remove_file(&dir);
        assert_eq!(s.imports, vec!["bar", "qux"]);
        assert_eq!(s.reexports, vec!["reexp", "allreexp"]);
    }
}
