use oxc_allocator::Allocator;
use oxc_ast::ast::{Argument, Expression, Statement};
use oxc_parser::{ParseOptions, Parser};
use oxc_span::SourceType;
use oxc_syntax::module_record::{ExportExportName, ExportImportName, ExportLocalName, ImportImportName};
use sprefa_extract::{kind, ExtractContext, Extractor, RawRef};

const EXTENSIONS: &[&str] = &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"];

pub struct JsExtractor;

impl Extractor for JsExtractor {
    fn extensions(&self) -> &[&str] {
        EXTENSIONS
    }

    fn extract(&self, source: &[u8], path: &str, _ctx: &ExtractContext) -> Vec<RawRef> {
        let Ok(source_text) = std::str::from_utf8(source) else {
            return vec![];
        };

        let source_type = source_type_for(path);
        let allocator = Allocator::default();
        let ret = Parser::new(&allocator, source_text, source_type)
            .with_options(ParseOptions::default())
            .parse();

        let mut refs = Vec::new();
        let mut gc: u32 = 0;
        let mr = &ret.module_record;

        // --- ImportPath: all module specifier strings (static, dynamic, re-exports) ---
        // requested_modules is keyed by specifier string, valued by all occurrence spans.
        // span covers the string literal including quotes -- strip 1 byte each side.
        for (specifier, requests) in &mr.requested_modules {
            for req in requests {
                refs.push(RawRef {
                    value: specifier.to_string(),
                    span_start: req.span.start + 1,
                    span_end: req.span.end - 1,
                    kind: kind::IMPORT_PATH.into(),
                    rule_name: kind::IMPORT_PATH.into(),
                    is_path: true,
                    parent_key: None,
                    node_path: None,
                    scan: None,
                    group: { gc += 1; Some(gc - 1) },
                });
            }
        }

        // --- ImportName + ImportAlias: named binding pairs from import statements ---
        for entry in &mr.import_entries {
            match &entry.import_name {
                ImportImportName::NamespaceObject => {
                    // import * as ns -- ns is the namespace binding, no source export name
                    refs.push(RawRef {
                        value: entry.local_name.name.to_string(),
                        span_start: entry.local_name.span.start,
                        span_end: entry.local_name.span.end,
                        kind: kind::IMPORT_NAME.into(),
                    rule_name: kind::IMPORT_NAME.into(),
                        is_path: false,
                        parent_key: None,
                        node_path: None,
                        scan: None,
                    group: { gc += 1; Some(gc - 1) },
                    });
                }
                ImportImportName::Name(ns) => {
                    let import_str = ns.name.as_str();
                    refs.push(RawRef {
                        value: import_str.to_string(),
                        span_start: ns.span.start,
                        span_end: ns.span.end,
                        kind: kind::IMPORT_NAME.into(),
                    rule_name: kind::IMPORT_NAME.into(),
                        is_path: false,
                        parent_key: None,
                        node_path: None,
                        scan: None,
                    group: { gc += 1; Some(gc - 1) },
                    });
                    // Alias only when local name differs from import name
                    let local = entry.local_name.name.as_str();
                    if local != import_str {
                        refs.push(RawRef {
                            value: local.to_string(),
                            span_start: entry.local_name.span.start,
                            span_end: entry.local_name.span.end,
                            kind: kind::IMPORT_ALIAS.into(),
                    rule_name: kind::IMPORT_ALIAS.into(),
                            is_path: false,
                            parent_key: Some(import_str.to_string()),
                            node_path: None,
                            scan: None,
                    group: { gc += 1; Some(gc - 1) },
                        });
                    }
                }
                ImportImportName::Default(_) => {
                    // import React from 'react'
                    // "default" has no physical span in this file; use statement_span.start
                    // for uniqueness in the UNIQUE(file_id, string_id, span_start) constraint.
                    let stmt_start = entry.statement_span.start;
                    refs.push(RawRef {
                        value: "default".to_string(),
                        span_start: stmt_start,
                        span_end: stmt_start,
                        kind: kind::IMPORT_NAME.into(),
                    rule_name: kind::IMPORT_NAME.into(),
                        is_path: false,
                        parent_key: None,
                        node_path: None,
                        scan: None,
                    group: { gc += 1; Some(gc - 1) },
                    });
                    refs.push(RawRef {
                        value: entry.local_name.name.to_string(),
                        span_start: entry.local_name.span.start,
                        span_end: entry.local_name.span.end,
                        kind: kind::IMPORT_ALIAS.into(),
                    rule_name: kind::IMPORT_ALIAS.into(),
                        is_path: false,
                        parent_key: Some("default".to_string()),
                        node_path: None,
                        scan: None,
                    group: { gc += 1; Some(gc - 1) },
                    });
                }
            }
        }

        // --- ExportName + ExportLocalBinding: direct exports ---
        for entry in &mr.local_export_entries {
            let (export_name_str, export_span) = match &entry.export_name {
                ExportExportName::Name(ns) => (ns.name.as_str().to_string(), ns.span),
                ExportExportName::Default(span) => ("default".to_string(), *span),
                ExportExportName::Null => continue,
            };
            refs.push(RawRef {
                value: export_name_str.clone(),
                span_start: export_span.start,
                span_end: export_span.end,
                kind: kind::EXPORT_NAME.into(),
                rule_name: kind::EXPORT_NAME.into(),
                is_path: false,
                parent_key: None,
                node_path: None,
                scan: None,
                group: { gc += 1; Some(gc - 1) },
            });
            // Emit ExportLocalBinding when internal name differs from exported name
            if let ExportLocalName::Name(local_ns) = &entry.local_name {
                let local_str = local_ns.name.as_str();
                if local_str != export_name_str {
                    refs.push(RawRef {
                        value: local_str.to_string(),
                        span_start: local_ns.span.start,
                        span_end: local_ns.span.end,
                        kind: kind::EXPORT_LOCAL_BINDING.into(),
                    rule_name: kind::EXPORT_LOCAL_BINDING.into(),
                        is_path: false,
                        parent_key: Some(export_name_str),
                        node_path: None,
                        scan: None,
                    group: { gc += 1; Some(gc - 1) },
                    });
                }
            }
        }

        // --- ExportName + ImportName: re-exported names from indirect + star entries ---
        //
        // `export { Foo } from './utils'` produces:
        //   - ExportName "Foo" (the public name consumers import)
        //   - ImportName "Foo" (the source-side name from the target module)
        //
        // `export { Foo as Bar } from './utils'` produces:
        //   - ExportName "Bar"
        //   - ImportName "Foo" (different from export name)
        //
        // The ImportName on re-exports is critical for transitive rename propagation:
        // when Foo is renamed in utils.ts, the barrel file is found as an importer of
        // "Foo" through the same import_names_from_file query used for direct imports.
        for entry in mr.indirect_export_entries.iter().chain(mr.star_export_entries.iter()) {
            let (export_name_str, export_span) = match &entry.export_name {
                ExportExportName::Name(ns) => (ns.name.as_str().to_string(), ns.span),
                ExportExportName::Default(span) => ("default".to_string(), *span),
                ExportExportName::Null => continue,
            };
            refs.push(RawRef {
                value: export_name_str.clone(),
                span_start: export_span.start,
                span_end: export_span.end,
                kind: kind::EXPORT_NAME.into(),
                rule_name: kind::EXPORT_NAME.into(),
                is_path: false,
                parent_key: None,
                node_path: None,
                scan: None,
                group: { gc += 1; Some(gc - 1) },
            });

            // Emit ImportName for the source-side name of the re-export.
            // This makes the barrel file visible as an "importer" of the name
            // from the source module, enabling transitive chain following.
            if let ExportImportName::Name(import_ns) = &entry.import_name {
                let import_str = import_ns.name.as_str();
                refs.push(RawRef {
                    value: import_str.to_string(),
                    span_start: import_ns.span.start,
                    span_end: import_ns.span.end,
                    kind: kind::IMPORT_NAME.into(),
                    rule_name: kind::IMPORT_NAME.into(),
                    is_path: false,
                    parent_key: None,
                    node_path: None,
                    scan: None,
                    group: { gc += 1; Some(gc - 1) },
                });
                // If the re-export aliases (Foo as Bar), the ImportName "Foo"
                // differs from ExportName "Bar". No alias ref needed here --
                // the ExportName is already the public-facing name.
            }
        }

        // --- ImportPath: require() calls (CJS) ---
        collect_require_calls(&ret.program.body, &mut refs, &mut gc);

        refs
    }
}

/// Walk top-level statements looking for `require('specifier')` calls.
/// Handles: variable initializers and expression statements.
/// Nested require() inside callbacks/closures is not extracted.
fn collect_require_calls<'a>(stmts: &'a [Statement<'a>], refs: &mut Vec<RawRef>, gc: &mut u32) {
    for stmt in stmts {
        match stmt {
            Statement::VariableDeclaration(decl) => {
                for d in &decl.declarations {
                    if let Some(init) = &d.init {
                        collect_require_expr(init, refs, gc);
                    }
                }
            }
            Statement::ExpressionStatement(s) => {
                collect_require_expr(&s.expression, refs, gc);
            }
            _ => {}
        }
    }
}

fn collect_require_expr<'a>(expr: &'a Expression<'a>, refs: &mut Vec<RawRef>, gc: &mut u32) {
    match expr {
        Expression::CallExpression(call) => {
            if let Expression::Identifier(id) = &call.callee {
                if id.name == "require" {
                    if let Some(Argument::StringLiteral(s)) = call.arguments.first() {
                        refs.push(RawRef {
                            value: s.value.to_string(),
                            span_start: s.span.start + 1,
                            span_end: s.span.end - 1,
                            kind: kind::IMPORT_PATH.into(),
                            rule_name: kind::IMPORT_PATH.into(),
                            is_path: true,
                            parent_key: None,
                            node_path: None,
                            scan: None,
                            group: { *gc += 1; Some(*gc - 1) },
                        });
                    }
                }
            }
        }
        Expression::AssignmentExpression(assign) => {
            collect_require_expr(&assign.right, refs, gc);
        }
        _ => {}
    }
}

fn source_type_for(path: &str) -> SourceType {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "ts" | "mts" | "cts" => SourceType::ts(),
        "tsx" => SourceType::tsx(),
        "jsx" => SourceType::jsx(),
        "mjs" | "cjs" | "js" => SourceType::mjs(),
        _ => SourceType::tsx(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(src: &str, path: &str) -> Vec<RawRef> {
        JsExtractor.extract(src.as_bytes(), path, &ExtractContext::default())
    }

    #[test]
    fn extracts_named_imports() {
        let refs = extract(
            r#"import { foo, bar } from './utils';
import type { Baz } from '../types';"#,
            "src/index.ts",
        );
        insta::assert_yaml_snapshot!("named_imports", refs);
    }

    #[test]
    fn extracts_default_import() {
        let refs = extract(
            r#"import React from 'react';
import express from 'express';"#,
            "src/app.tsx",
        );
        insta::assert_yaml_snapshot!("default_imports", refs);
    }

    #[test]
    fn extracts_exports() {
        let refs = extract(
            r#"export const foo = 1;
export function bar() {}
export default class Baz {}"#,
            "src/mod.ts",
        );
        insta::assert_yaml_snapshot!("exports", refs);
    }

    #[test]
    fn extracts_reexports() {
        let refs = extract(
            r#"export { foo, bar } from './foo';
export * from './barrel';"#,
            "src/index.ts",
        );
        insta::assert_yaml_snapshot!("reexports", refs);
    }

    #[test]
    fn extracts_import_alias() {
        let refs = extract(
            r#"import { Foo as localFoo } from './mod';
import { Bar } from './other';"#,
            "src/x.ts",
        );
        insta::assert_yaml_snapshot!("import_alias", refs);
    }

    #[test]
    fn extracts_export_alias() {
        let refs = extract(
            r#"const internalName = 1;
export { internalName as PublicName };"#,
            "src/y.ts",
        );
        insta::assert_yaml_snapshot!("export_alias", refs);
    }

    #[test]
    fn extracts_namespace_import() {
        let refs = extract(
            r#"import * as utils from './utils';"#,
            "src/z.ts",
        );
        insta::assert_yaml_snapshot!("namespace_import", refs);
    }

    #[test]
    fn extracts_require() {
        let refs = extract(
            r#"const fs = require('fs');
const path = require('path');"#,
            "src/legacy.cjs",
        );
        let import_paths: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::IMPORT_PATH)
            .map(|r| r.value.as_str())
            .collect();
        insta::assert_yaml_snapshot!("require_calls", import_paths);
    }

    #[test]
    fn span_points_at_specifier() {
        let src = r#"import { foo } from './utils';"#;
        let refs = extract(src, "src/x.ts");
        let r = refs.iter().find(|r| r.kind == kind::IMPORT_PATH).unwrap();
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
        assert!(refs.iter().any(|r| r.value == "@ui/components"));
    }

    #[test]
    fn mts_extension_parses() {
        let refs = extract(
            r#"import { foo } from './utils';"#,
            "src/mod.mts",
        );
        assert!(refs.iter().any(|r| r.kind == kind::IMPORT_PATH && r.value == "./utils"));
    }

    #[test]
    fn cts_extension_with_require() {
        let refs = extract(
            r#"const x = require('./lib');"#,
            "src/mod.cts",
        );
        assert!(refs.iter().any(|r| r.kind == kind::IMPORT_PATH && r.value == "./lib"));
    }

    #[test]
    fn reexport_with_rename() {
        let refs = extract(
            r#"export { foo as Bar } from './source';"#,
            "src/barrel.ts",
        );
        insta::assert_yaml_snapshot!("reexport_rename", refs);
    }

    #[test]
    fn import_alias_parent_key_links_to_import_name() {
        let refs = extract(
            r#"import { Foo as localFoo } from './mod';"#,
            "src/x.ts",
        );
        let alias = refs.iter().find(|r| r.kind == kind::IMPORT_ALIAS).unwrap();
        assert_eq!(alias.value, "localFoo");
        assert_eq!(alias.parent_key.as_deref(), Some("Foo"));
    }

    #[test]
    fn export_local_binding_parent_key_links_to_export_name() {
        let refs = extract(
            r#"export { internal as Public };"#,
            "src/x.ts",
        );
        let binding = refs.iter().find(|r| r.kind == kind::EXPORT_LOCAL_BINDING).unwrap();
        assert_eq!(binding.value, "internal");
        assert_eq!(binding.parent_key.as_deref(), Some("Public"));
    }

    // ── edge cases ────────────────────────────────────────────────────────

    #[test]
    fn side_effect_import() {
        // import './polyfill' has an ImportPath but no ImportName/ImportAlias
        let refs = extract(
            r#"import './polyfill';
import 'reflect-metadata';"#,
            "src/main.ts",
        );
        let mut paths: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::IMPORT_PATH)
            .map(|r| r.value.as_str())
            .collect();
        paths.sort();
        assert_eq!(paths, vec!["./polyfill", "reflect-metadata"]);
        // No ImportName refs for side-effect imports
        assert!(refs.iter().all(|r| r.kind != kind::IMPORT_NAME));
    }

    #[test]
    fn type_only_import() {
        // `import type` still produces ImportPath + ImportName refs
        let refs = extract(
            r#"import type { MyType, OtherType } from './types';"#,
            "src/app.ts",
        );
        assert!(refs.iter().any(|r| r.kind == kind::IMPORT_PATH && r.value == "./types"));
        let names: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::IMPORT_NAME)
            .map(|r| r.value.as_str())
            .collect();
        assert_eq!(names, vec!["MyType", "OtherType"]);
    }

    #[test]
    fn type_only_export() {
        let refs = extract(
            r#"export type { Foo, Bar } from './models';"#,
            "src/index.ts",
        );
        assert!(refs.iter().any(|r| r.kind == kind::IMPORT_PATH && r.value == "./models"));
        let exports: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::EXPORT_NAME)
            .map(|r| r.value.as_str())
            .collect();
        assert_eq!(exports, vec!["Foo", "Bar"]);
    }

    #[test]
    fn dynamic_import_not_captured() {
        // import() expressions are NOT in oxc's requested_modules --
        // they're runtime calls, not static module requests.
        // This is correct: we only rewrite static imports.
        let refs = extract(
            r#"const mod = await import('./lazy');
const other = import('./another');"#,
            "src/app.ts",
        );
        let paths: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::IMPORT_PATH)
            .map(|r| r.value.as_str())
            .collect();
        assert!(paths.is_empty());
    }

    #[test]
    fn empty_file() {
        let refs = extract("", "src/empty.ts");
        assert!(refs.is_empty());
    }

    #[test]
    fn whitespace_only_file() {
        let refs = extract("   \n\n  \t  \n", "src/blank.ts");
        assert!(refs.is_empty());
    }

    #[test]
    fn anonymous_default_export_function() {
        let refs = extract(
            r#"export default function() { return 42; }"#,
            "src/anon.ts",
        );
        let exports: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::EXPORT_NAME)
            .map(|r| r.value.as_str())
            .collect();
        assert_eq!(exports, vec!["default"]);
    }

    #[test]
    fn anonymous_default_export_arrow() {
        let refs = extract(
            r#"export default () => 42;"#,
            "src/arrow.ts",
        );
        assert!(refs.iter().any(|r| r.kind == kind::EXPORT_NAME && r.value == "default"));
    }

    #[test]
    fn namespace_reexport() {
        // export * as ns from './mod'
        let refs = extract(
            r#"export * as utils from './utils';"#,
            "src/barrel.ts",
        );
        assert!(refs.iter().any(|r| r.kind == kind::IMPORT_PATH && r.value == "./utils"));
        // "utils" should be an ExportName
        assert!(refs.iter().any(|r| r.kind == kind::EXPORT_NAME && r.value == "utils"));
    }

    #[test]
    fn multiple_imports_same_module() {
        // Two import statements from same module -- both produce ImportPath
        let refs = extract(
            r#"import { foo } from './utils';
import { bar } from './utils';"#,
            "src/app.ts",
        );
        let path_count = refs.iter()
            .filter(|r| r.kind == kind::IMPORT_PATH && r.value == "./utils")
            .count();
        assert_eq!(path_count, 2);
    }

    #[test]
    fn mixed_esm_and_require() {
        let refs = extract(
            r#"import { foo } from './esm';
const bar = require('./cjs');"#,
            "src/mixed.ts",
        );
        let paths: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::IMPORT_PATH)
            .map(|r| r.value.as_str())
            .collect();
        assert!(paths.contains(&"./esm"));
        assert!(paths.contains(&"./cjs"));
    }

    #[test]
    fn require_with_non_string_arg_ignored() {
        let refs = extract(
            r#"const x = require(variableName);
const y = require('./valid');"#,
            "src/dynamic.cjs",
        );
        let paths: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::IMPORT_PATH)
            .map(|r| r.value.as_str())
            .collect();
        // Only the string literal require is captured
        assert_eq!(paths, vec!["./valid"]);
    }

    #[test]
    fn nested_require_in_function_not_captured() {
        let refs = extract(
            r#"function load() {
    const x = require('./nested');
}"#,
            "src/fn.cjs",
        );
        // collect_require_calls only walks top-level statements
        assert!(refs.iter().all(|r| r.value != "./nested"));
    }

    #[test]
    fn export_default_class_named() {
        let refs = extract(
            r#"export default class MyClass {}"#,
            "src/cls.ts",
        );
        assert!(refs.iter().any(|r| r.kind == kind::EXPORT_NAME && r.value == "default"));
    }

    #[test]
    fn import_and_reexport_same_specifier() {
        let refs = extract(
            r#"import { foo } from './shared';
export { bar } from './shared';"#,
            "src/bridge.ts",
        );
        let path_refs: Vec<_> = refs.iter()
            .filter(|r| r.kind == kind::IMPORT_PATH && r.value == "./shared")
            .collect();
        assert_eq!(path_refs.len(), 2);
    }

    #[test]
    fn invalid_utf8_returns_empty() {
        let bytes: &[u8] = &[0xFF, 0xFE, 0x00, 0x00];
        let refs = JsExtractor.extract(bytes, "src/binary.ts", &ExtractContext::default());
        assert!(refs.is_empty());
    }

    #[test]
    fn span_accuracy_on_import_name() {
        let src = r#"import { FooBar } from './mod';"#;
        let refs = extract(src, "src/x.ts");
        let name_ref = refs.iter().find(|r| r.kind == kind::IMPORT_NAME && r.value == "FooBar").unwrap();
        let slice = &src[name_ref.span_start as usize..name_ref.span_end as usize];
        assert_eq!(slice, "FooBar");
    }

    #[test]
    fn span_accuracy_on_export_name() {
        let src = r#"export function myFunc() {}"#;
        let refs = extract(src, "src/x.ts");
        let exp = refs.iter().find(|r| r.kind == kind::EXPORT_NAME).unwrap();
        let slice = &src[exp.span_start as usize..exp.span_end as usize];
        assert_eq!(slice, "myFunc");
    }

    // ── re-export chain extraction ───────────────────────────────────────

    #[test]
    fn reexport_emits_import_name_for_source_side() {
        // `export { Foo } from './utils'` should produce both ExportName and ImportName "Foo"
        let refs = extract(
            r#"export { Foo } from './utils';"#,
            "src/barrel.ts",
        );
        let import_names: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::IMPORT_NAME)
            .map(|r| r.value.as_str())
            .collect();
        let export_names: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::EXPORT_NAME)
            .map(|r| r.value.as_str())
            .collect();
        assert_eq!(import_names, vec!["Foo"]);
        assert_eq!(export_names, vec!["Foo"]);
    }

    #[test]
    fn aliased_reexport_emits_different_import_and_export_names() {
        // `export { Foo as Bar } from './utils'` -> ImportName "Foo", ExportName "Bar"
        let refs = extract(
            r#"export { Foo as Bar } from './utils';"#,
            "src/barrel.ts",
        );
        let import_name = refs.iter()
            .find(|r| r.kind == kind::IMPORT_NAME)
            .expect("should emit ImportName for source-side name");
        let export_name = refs.iter()
            .find(|r| r.kind == kind::EXPORT_NAME)
            .expect("should emit ExportName for public name");
        assert_eq!(import_name.value, "Foo");
        assert_eq!(export_name.value, "Bar");
    }

    #[test]
    fn reexport_import_name_span_accuracy() {
        let src = r#"export { Foo } from './utils';"#;
        let refs = extract(src, "src/barrel.ts");
        let import_name = refs.iter()
            .find(|r| r.kind == kind::IMPORT_NAME)
            .unwrap();
        let slice = &src[import_name.span_start as usize..import_name.span_end as usize];
        assert_eq!(slice, "Foo");
    }

    #[test]
    fn multiple_reexports_emit_import_names() {
        let refs = extract(
            r#"export { Foo, Bar, Baz } from './utils';"#,
            "src/barrel.ts",
        );
        let mut import_names: Vec<&str> = refs.iter()
            .filter(|r| r.kind == kind::IMPORT_NAME)
            .map(|r| r.value.as_str())
            .collect();
        import_names.sort();
        assert_eq!(import_names, vec!["Bar", "Baz", "Foo"]);
    }

    #[test]
    fn star_reexport_no_import_name() {
        // `export * from './utils'` has no specific import name (ExportName is Null -> skipped)
        let refs = extract(
            r#"export * from './utils';"#,
            "src/barrel.ts",
        );
        // Star re-exports produce ImportPath but no ImportName (no specific named binding)
        assert!(refs.iter().all(|r| r.kind != kind::IMPORT_NAME));
    }

    #[test]
    fn cjs_reexport_module_exports_require() {
        // CJS re-export pattern: module.exports = require('./utils')
        // This only captures the require ImportPath, not individual names
        let refs = extract(
            r#"module.exports = require('./utils');"#,
            "src/barrel.cjs",
        );
        assert!(refs.iter().any(|r| r.kind == kind::IMPORT_PATH && r.value == "./utils"));
    }
}
