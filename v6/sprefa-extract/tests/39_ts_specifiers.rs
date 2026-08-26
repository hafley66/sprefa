//! The TS corpus walk (arc 1) and the module-specifier rows the move rewrites
//! by (arc 2). Expected values are hand-derived from the fixture text, never
//! copied from the extractor's output.

use std::path::PathBuf;

use sprefa_extract::{
    corpus_lang, is_ts_family, specifier_corpus, ts_corpus, ts_specifiers, CorpusLang, FamilyMask,
    Source, TsSource,
};

const PATH: &str = "tests/fixtures/ts_move/src/index.ts";
const SOURCE: &str = include_str!("fixtures/ts_move/src/index.ts");

/// SABOTAGE RECEIPT: dropping the `visit_ts_import_equals_declaration` arm
/// leaves row 11 missing; taking `named.span` instead of `import.source.span`
/// for `module_span` makes row 1's slice read `thing` instead of `'./b.ts'`.
#[test]
fn ts_specifier_rows_carry_the_literal_span() {
    let scanned = ts_specifiers(PATH, SOURCE).expect("fixture parses");
    let rows: Vec<(&str, &str, &str)> = scanned
        .iter()
        .map(|row| {
            let start = row.module_span.start as usize;
            let end = row.module_span.end() as usize;
            (row.kind.as_str(), row.module.as_str(), &SOURCE[start..end])
        })
        .collect();

    assert_eq!(
        rows,
        [
            ("named", "./b.ts", "'./b.ts'"),
            ("named", "./b", "'./b'"),
            ("named", "./dir", "'./dir'"),
            ("named", "./b.js", "'./b.js'"),
            ("named", "@app/b", "'@app/b'"),
            ("named", "pkg-exports", "'pkg-exports'"),
            ("reexport", "./reexport", "'./reexport'"),
            ("reexport", "./dir", "'./dir'"),
            ("dynamic_import", "./b.js", "'./b.js'"),
            ("require", "./b", "'./b'"),
            ("require", "./b", "'./b'"),
        ]
    );
}

/// Every row's quote is the one the literal was written with, and every span
/// starts on it: what a re-aim needs to replace the literal whole.
#[test]
fn ts_specifier_spans_start_on_their_quote() {
    let double = "import a from \"./b\";\n";
    let rows = ts_specifiers("x.ts", double).expect("parses");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].quote, '"');
    let text = &double[rows[0].module_span.start as usize..rows[0].module_span.end() as usize];
    assert_eq!(text, "\"./b\"");

    let single = ts_specifiers("x.ts", SOURCE).expect("parses");
    assert!(single.iter().all(|row| row.quote == '\''));
}

/// A computed path has none to record, so it stays out of the specifier rows
/// (it is `CallFAux.unresolved`'s row); a plain string is not a specifier.
#[test]
fn computed_and_plain_strings_are_not_specifiers() {
    let source = "const name = './b';\nconst m = import(name);\nconst r = require(name);\n\
                  const s = 'hello';\nreadFile('./b');\n";
    let scanned = ts_specifiers("x.ts", source).expect("parses");
    let modules: Vec<&str> = scanned.iter().map(|row| row.module.as_str()).collect();
    assert!(modules.is_empty(), "unexpected rows: {modules:?}");
}

/// A runtime module reference nested in a function body is still a row: the
/// scan visits the whole tree, not just top-level statements.
#[test]
fn nested_runtime_references_are_rows() {
    let source = "async function load() {\n  const m = await import('./deep');\n\
                  \n  return require('./other');\n}\n";
    let scanned = ts_specifiers("x.ts", source).expect("parses");
    let rows: Vec<(&str, &str)> = scanned
        .iter()
        .map(|row| (row.kind.as_str(), row.module.as_str()))
        .collect();
    assert_eq!(rows, [("dynamic_import", "./deep"), ("require", "./other")]);
}

/// The extractor's own `CallFAux.specifiers` carry the same set through
/// `TsSource`: the move and the fact plane read one scan.
#[test]
fn extractor_rows_include_the_runtime_forms() {
    let output = TsSource.extract(PATH, SOURCE.as_bytes(), FamilyMask::ALL);
    let call = output.call.as_ref().expect("call family");
    let rows: Vec<(&str, &str)> = call
        .aux
        .specifiers
        .iter()
        .map(|specifier| {
            (
                specifier.kind.as_str(),
                specifier
                    .module
                    .map(|id| output.strings.lookup(id))
                    .unwrap_or_default(),
            )
        })
        .collect();

    assert_eq!(
        rows,
        [
            ("named", "./b.ts"),
            ("named", "./b"),
            ("named", "./dir"),
            ("named", "./b.js"),
            ("named", "@app/b"),
            ("named", "pkg-exports"),
            ("reexport", "./reexport"),
            ("reexport", "./dir"),
            ("dynamic_import", "./b.js"),
            ("require", "./b"),
            ("require", "./b"),
        ]
    );
}

/// `.kts` is Kotlin, which `ends_with(".ts")` cannot say; `.d.ts` is TS.
#[test]
fn ts_family_membership_is_by_extension() {
    assert!(is_ts_family("a/b.ts"));
    assert!(is_ts_family("a/b.d.ts"));
    assert!(is_ts_family("a/b.mjs"));
    assert!(!is_ts_family("a/b.kts"));
    assert!(!is_ts_family("a/b.pl"));
    assert!(!is_ts_family("a/ts"));
    assert!(!is_ts_family("a.ts/b"));
    assert_eq!(corpus_lang("a/b.plt"), Some(CorpusLang::Prolog));
    assert_eq!(corpus_lang("a/b.tsx"), Some(CorpusLang::Tsx));
    assert_eq!(corpus_lang("a/b.cjs"), Some(CorpusLang::Ts));
    assert_eq!(corpus_lang("a/b.kt"), None);
}

/// SABOTAGE RECEIPT: dropping `node_modules` from `SKIP_DIRS` adds
/// `node_modules/dep/index.js` to the corpus and this test fails on length.
#[test]
fn corpus_walk_takes_carriers_and_skips_vendor_trees() {
    let root = temp_root("corpus-walk");
    for rel in [
        "a.ts",
        "ui/a.tsx",
        "m.mts",
        "c.cts",
        "legacy.js",
        "legacy.jsx",
        "esm.mjs",
        "cjs.cjs",
        "rule.pl",
        "rule.plt",
        "notes.md",
        "script.kts",
        "node_modules/dep/index.js",
        ".boop-worktrees/lane/x.ts",
        "target/debug/build.ts",
        "dist/a.js",
        ".git/hooks/x.ts",
    ] {
        write(&root, rel, "export const x = 1;\n");
    }

    let corpus: Vec<(String, CorpusLang)> = specifier_corpus(&root)
        .into_iter()
        .map(|(path, lang)| {
            (
                path.strip_prefix(&root)
                    .expect("under root")
                    .to_string_lossy()
                    .to_string(),
                lang,
            )
        })
        .collect();

    assert_eq!(
        corpus,
        [
            ("a.ts".to_string(), CorpusLang::Ts),
            ("c.cts".to_string(), CorpusLang::Ts),
            ("cjs.cjs".to_string(), CorpusLang::Ts),
            ("esm.mjs".to_string(), CorpusLang::Ts),
            ("legacy.js".to_string(), CorpusLang::Ts),
            ("legacy.jsx".to_string(), CorpusLang::Tsx),
            ("m.mts".to_string(), CorpusLang::Ts),
            ("rule.pl".to_string(), CorpusLang::Prolog),
            ("rule.plt".to_string(), CorpusLang::Prolog),
            ("ui/a.tsx".to_string(), CorpusLang::Tsx),
        ]
    );

    let ts: Vec<String> = ts_corpus(&root)
        .into_iter()
        .map(|path| {
            path.strip_prefix(&root)
                .expect("under root")
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert!(!ts.iter().any(|rel| rel.ends_with(".pl")));
    assert_eq!(ts.len(), 8);

    std::fs::remove_dir_all(&root).ok();
}

/// A monorepo root reaches every package: the walk descends `packages/*` and
/// drops each package's own `node_modules`.
#[test]
fn corpus_walk_spans_workspace_packages() {
    let root = temp_root("corpus-monorepo");
    for rel in [
        "tsconfig.json",
        "packages/one/src/a.ts",
        "packages/two/src/b.ts",
        "packages/two/node_modules/dep/index.ts",
    ] {
        write(&root, rel, "export const x = 1;\n");
    }

    let corpus: Vec<String> = ts_corpus(&root)
        .into_iter()
        .map(|path| {
            path.strip_prefix(&root)
                .expect("under root")
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert_eq!(corpus, ["packages/one/src/a.ts", "packages/two/src/b.ts"]);

    std::fs::remove_dir_all(&root).ok();
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "sprefa-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("create temp root");
    root
}

fn write(root: &std::path::Path, rel: &str, body: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    std::fs::write(path, body).expect("write fixture file");
}
