use oxc_allocator::Allocator;
use oxc_parser::{ParseOptions, Parser};
use oxc_span::SourceType;
use oxc_syntax::module_record::ExportExportName;
use sprefa_extract::{Extractor, RawRef};
use sprefa_schema::RefKind;

const EXTENSIONS: &[&str] = &["js", "jsx", "ts", "tsx", "mjs", "cjs"];

pub struct JsExtractor;

impl Extractor for JsExtractor {
    fn extensions(&self) -> &[&str] {
        EXTENSIONS
    }

    fn extract(&self, source: &[u8], path: &str) -> Vec<RawRef> {
        let Ok(source_text) = std::str::from_utf8(source) else {
            return vec![];
        };

        let source_type = source_type_for(path);
        let allocator = Allocator::default();
        let ret = Parser::new(&allocator, source_text, source_type)
            .with_options(ParseOptions::default())
            .parse();

        let mut refs = Vec::new();
        let mr = &ret.module_record;

        // Imports + re-exports: requested_modules is the complete set of all specifiers.
        // req.span covers the string literal including quotes -- strip 1 byte each side.
        for (specifier, requests) in &mr.requested_modules {
            for req in requests {
                refs.push(RawRef {
                    value: specifier.to_string(),
                    span_start: req.span.start + 1,
                    span_end: req.span.end - 1,
                    kind: RefKind::ImportPath,
                    is_path: true,
                    parent_key: None,
                    node_path: None,
                });
            }
        }

        // Local exports: names declared in this file
        for entry in &mr.local_export_entries {
            let name = match &entry.export_name {
                ExportExportName::Name(ns) => ns.name.as_str(),
                ExportExportName::Default(_) => "default",
                ExportExportName::Null => continue,
            };
            refs.push(RawRef {
                value: name.to_string(),
                span_start: entry.span.start,
                span_end: entry.span.end,
                kind: RefKind::ExportName,
                is_path: false,
                parent_key: None,
                node_path: None,
            });
        }

        refs
    }
}

fn source_type_for(path: &str) -> SourceType {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "ts" => SourceType::ts(),
        "tsx" => SourceType::tsx(),
        "jsx" => SourceType::jsx(),
        "mjs" | "cjs" | "js" => SourceType::mjs(),
        _ => SourceType::tsx(), // superset fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(src: &str, path: &str) -> Vec<RawRef> {
        JsExtractor.extract(src.as_bytes(), path)
    }

    #[test]
    fn extracts_named_imports() {
        let refs = extract(
            r#"import { foo, bar } from './utils';
import type { Baz } from '../types';"#,
            "src/index.ts",
        );
        let import_paths: Vec<&str> = refs.iter()
            .filter(|r| r.kind == RefKind::ImportPath)
            .map(|r| r.value.as_str())
            .collect();
        insta::assert_yaml_snapshot!("named_imports", import_paths);
    }

    #[test]
    fn extracts_default_import() {
        let refs = extract(
            r#"import React from 'react';
import express from 'express';"#,
            "src/app.tsx",
        );
        let paths: Vec<&str> = refs.iter().map(|r| r.value.as_str()).collect();
        insta::assert_yaml_snapshot!("default_imports", paths);
    }

    #[test]
    fn extracts_exports() {
        let refs = extract(
            r#"export const foo = 1;
export function bar() {}
export default class Baz {}"#,
            "src/mod.ts",
        );
        let exports: Vec<&str> = refs.iter()
            .filter(|r| r.kind == RefKind::ExportName)
            .map(|r| r.value.as_str())
            .collect();
        insta::assert_yaml_snapshot!("exports", exports);
    }

    #[test]
    fn extracts_reexports() {
        let refs = extract(
            r#"export { foo, bar } from './foo';
export * from './barrel';"#,
            "src/index.ts",
        );
        let paths: Vec<&str> = refs.iter()
            .filter(|r| r.kind == RefKind::ImportPath)
            .map(|r| r.value.as_str())
            .collect();
        insta::assert_yaml_snapshot!("reexports", paths);
    }

    #[test]
    fn span_points_at_specifier() {
        let src = r#"import { foo } from './utils';"#;
        let refs = extract(src, "src/x.ts");
        let r = refs.iter().find(|r| r.kind == RefKind::ImportPath).unwrap();
        // span should cover "./utils" (without quotes)
        let slice = &src.as_bytes()[r.span_start as usize..r.span_end as usize];
        assert_eq!(std::str::from_utf8(slice).unwrap(), "./utils");
    }

    #[test]
    fn jsx_file_parses_without_error() {
        let refs = extract(
            r#"import { Button } from '@ui/components';
export default function App() { return <Button />; }"#,
            "src/App.tsx",
        );
        // should extract the import path
        assert!(refs.iter().any(|r| r.value == "@ui/components"));
    }

    #[test]
    fn commonjs_style_returns_empty() {
        // CJS require() is not a module-record import, should produce no refs
        let refs = extract(
            r#"const fs = require('fs');
module.exports = { fs };"#,
            "src/legacy.cjs",
        );
        assert!(refs.is_empty());
    }
}
