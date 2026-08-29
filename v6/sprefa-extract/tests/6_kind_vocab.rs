//! Kind-vocabulary rails: the family kind enums are core + per-language
//! extension (option B). The core list in types.rs carries only the kinds two
//! or more languages construct; a kind one language owns lives in that
//! language's file as an `Ext(LangKind)` constant. The wire tags are frozen:
//! the committed `wire_golden.jsonl` is the extract output over
//! `tests/fixtures/**` at 946460d75, and today's output must reproduce it
//! byte-for-byte.

use std::collections::BTreeSet;
use std::process::Command;

use sprefa_extract::lang::rust;
use sprefa_extract::lang::ts;
use sprefa_extract::{CallKind, DfNodeKind, TypeEntityKind};

/// Every tag the core variants answer. An ext tag must never land in this set
/// (the Ext door exists so a language never edits this list).
const CORE_TAGS: &[&str] = &[
    // DfNodeKind core ("try" is core with no constructor yet)
    "param",
    "let_bind",
    "var_read",
    "var_write",
    "lit",
    "call_res",
    "new",
    "member",
    "ret",
    "binop",
    "unop",
    "loop",
    "if",
    "closure",
    "try",
    "expr",
    "logic",
    // TypeEntityKind core
    "struct",
    "enum",
    "class",
    "interface",
    "alias",
    "function",
    "method",
    "const",
    // CallKind core
    "function",
    "method",
    "lambda",
];

/// The per-language ext constants and the tags they freeze.
const EXT_KINDS: &[(&str, &'static str)] = &[
    ("rust::BORROW", rust::BORROW.as_str()),
    ("rust::BREAK", rust::BREAK.as_str()),
    ("rust::MATCH", rust::MATCH.as_str()),
    ("rust::BLOCK", rust::BLOCK.as_str()),
    ("rust::TRAIT", rust::TRAIT.as_str()),
    ("rust::CONST_INIT", rust::CONST_INIT.as_str()),
    ("ts::COND", ts::COND.as_str()),
    ("ts::CONCAT", ts::CONCAT.as_str()),
    ("ts::TEMPLATE", ts::TEMPLATE.as_str()),
];

fn every_core_kind_tag() -> Vec<&'static str> {
    use DfNodeKind as D;
    use TypeEntityKind as T;
    vec![
        D::Param.as_str(),
        D::LetBind.as_str(),
        D::VarRead.as_str(),
        D::VarWrite.as_str(),
        D::Lit.as_str(),
        D::CallRes.as_str(),
        D::New.as_str(),
        D::Member.as_str(),
        D::Ret.as_str(),
        D::Binop.as_str(),
        D::Unop.as_str(),
        D::Loop.as_str(),
        D::If.as_str(),
        D::Closure.as_str(),
        D::Try.as_str(),
        D::Expr.as_str(),
        D::Logic.as_str(),
        T::Struct.as_str(),
        T::Enum.as_str(),
        T::Class.as_str(),
        T::Interface.as_str(),
        T::Alias.as_str(),
        T::Function.as_str(),
        T::Method.as_str(),
        T::Const.as_str(),
        CallKind::Free.as_str(),
        CallKind::Method.as_str(),
        CallKind::Lambda.as_str(),
    ]
}

#[test]
fn no_ext_tag_collides_with_a_core_tag() {
    for (name, tag) in EXT_KINDS {
        assert!(
            !CORE_TAGS.contains(tag),
            "ext {name} tag '{tag}' collides with a core tag"
        );
    }
    // The core list in types.rs still answers exactly its pinned tags: a core
    // variant renamed into an ext slot (or a new ext tag adopted into the core)
    // breaks this set equality.
    let core: BTreeSet<&str> = every_core_kind_tag().into_iter().collect();
    let pinned: BTreeSet<&str> = CORE_TAGS.iter().copied().collect();
    assert_eq!(core, pinned, "core tag set drifted from the pinned list");
}

#[test]
fn as_str_is_byte_stable_for_every_kind() {
    let mut observed: BTreeSet<&str> = every_core_kind_tag().into_iter().collect();
    for (_, tag) in EXT_KINDS {
        observed.insert(tag);
    }
    let mut golden: BTreeSet<&str> = CORE_TAGS.iter().copied().collect();
    golden.extend(EXT_KINDS.iter().map(|(_, tag)| *tag));
    assert_eq!(observed, golden, "the tag vocabulary drifted");
}

#[test]
fn a_single_language_kind_lives_in_its_language_file() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let types_rs =
        std::fs::read_to_string(format!("{manifest}/src/types.rs")).expect("types.rs readable");
    let lang_files = walk_rs_files(&format!("{manifest}/src/lang"));
    assert!(lang_files.len() > 5, "lang/ files found: {lang_files:?}");

    for (enum_name, kinds_prefix) in [
        ("DfNodeKind", "DfNodeKind::"),
        ("TypeEntityKind", "TypeEntityKind::"),
        ("CallKind", "CallKind::"),
    ] {
        let variants = enum_variants(&types_rs, enum_name)
            .unwrap_or_else(|| panic!("{enum_name} block not found in types.rs"));
        for variant in variants {
            let needle = format!("{kinds_prefix}{variant}");
            let owners: Vec<&String> = lang_files
                .iter()
                .filter(|file| std::fs::read_to_string(file).unwrap().contains(&needle))
                .collect();
            assert_ne!(
                owners.len(),
                1,
                "{enum_name}::{variant} is constructed in exactly one language file ({:?}); \
                 it must leave the core and become that language's Ext constant",
                owners[0]
            );
        }
    }
}

/// The variant idents between `pub enum <name> {` and its closing brace.
fn enum_variants(types_rs: &str, name: &str) -> Option<Vec<String>> {
    let start = types_rs.find(&format!("pub enum {name} "))?;
    let body = &types_rs[start..];
    let open = body.find('{')?;
    let mut depth = 0;
    let mut close = open;
    for (offset, char) in body[open..].char_indices() {
        match char {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = open + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    let block = &body[open + 1..close];
    Some(
        block
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("//"))
            .filter(|line| !line.starts_with("Ext("))
            .map(|line| line.trim_end_matches(','))
            .map(str::to_string)
            .collect(),
    )
}

#[test]
fn extract_lang_has_no_path_switch() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let source = std::fs::read_to_string(format!("{manifest}/src/lang/extract_lang.rs"))
        .expect("extract_lang.rs readable");
    assert!(
        source.contains("source_for"),
        "ExtractLang::from_path must delegate to the Source roster"
    );
    for suffix in [".dl6", ".pl", ".md", ".markdown", ".horn", ".datalog"] {
        assert!(
            !source.contains(&format!("ends_with(\"{suffix}\")")),
            "extract_lang.rs still path-switches on '{suffix}'; that knowledge belongs to the Source roster"
        );
    }
}

#[test]
fn wire_output_is_byte_identical_to_the_946460d75_golden() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let exe = env!("CARGO_BIN_EXE_extract");
    // The corpus is the fixture list at 946460d75, pinned in corpus.txt so a
    // fixture added later never changes the golden.
    let corpus = std::fs::read_to_string(format!(
        "{manifest}/tests/fixtures/kind_vocab/corpus.txt"
    ))
    .expect("corpus list readable");
    let fixture_files: Vec<String> = corpus.lines().map(str::to_string).collect();
    assert!(
        fixture_files.len() > 100,
        "fixture files: {}",
        fixture_files.len()
    );

    let golden = std::fs::read(format!(
        "{manifest}/tests/fixtures/kind_vocab/wire_golden.jsonl"
    ))
    .expect("wire golden readable");
    let mut current: Vec<u8> = Vec::new();
    for path in &fixture_files {
        let output = Command::new(exe)
            .current_dir(manifest)
            .arg(path)
            .output()
            .expect("extract binary runs");
        assert!(
            output.status.success(),
            "extract failed on {path}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        current.extend_from_slice(&output.stdout);
    }
    assert_eq!(
        current.len(),
        golden.len(),
        "wire byte count drifted from the 946460d75 golden"
    );
    if current != golden {
        let first_diff = current
            .iter()
            .zip(golden.iter())
            .position(|(now, then)| now != then)
            .unwrap_or(0);
        let line = golden[..first_diff]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            + 1;
        panic!(
            "wire output differs from the golden at line {line}; the kind tags must stay byte-identical"
        );
    }
}

fn walk(root: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_string()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("dir readable") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path.to_string_lossy().to_string());
            } else {
                out.push(path.to_string_lossy().to_string());
            }
        }
    }
    out
}

fn walk_rs_files(root: &str) -> Vec<String> {
    walk(root)
        .into_iter()
        .filter(|path| path.ends_with(".rs"))
        .collect()
}
