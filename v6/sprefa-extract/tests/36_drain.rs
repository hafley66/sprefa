//! The ast-grep -> soopy edit drain: byte spans, the one Replace per file, and
//! the expected-hash precondition soopy checks before it derives any output.
//!
//! @comment-ok: sabotage receipt, repo law keeps these in TEST headers.
//! SABOTAGE: `From<BoundEdit>` rewritten to `end: start` (deleted_length
//! dropped, every edit a pure insertion) measured 4 failed / 4 passed, and
//! `stage_edits_reaches_soopy_with_the_drained_spans` was one of the GREEN ones:
//! reaching soopy judges nothing about span width, so the (start, end, bytes)
//! assertions are what catch it.
//! FAIL-FIRST: `stale_expected_is_refused_by_stage` with `expected` hashed from
//! the real on-disk bytes measured `Ok` at its `expect_err`, so it fails only
//! on the wrong hash, which is the precondition the whole drain rests on.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ast_grep_core::source::Edit;
use ast_grep_core::tree_sitter::LanguageExt;
use ast_grep_core::AstGrep;
use ast_grep_language::SupportLang;
use sprefa_extract::{
    directory_source, drain_edits, replace_action, stage_edits, BoundEdit, PendingReplaceDoc,
};

const SRC: &str = "fn main() { foo(); foo(); }\n";
const REL: &str = "src/main.rs";

fn producer() -> soopy::ActionProducer {
    soopy::ActionProducer::unordered("test-drain")
}

fn detached_identity() -> soopy::DirectoryId {
    soopy::DirectoryId(Arc::from("test-directory"))
}

fn bind(source: &soopy::ActionSource, edits: Vec<Edit<String>>) -> Vec<soopy::TextEdit> {
    edits
        .into_iter()
        .map(|edit| {
            BoundEdit {
                source: source.clone(),
                producer: producer(),
                edit,
            }
            .into()
        })
        .collect()
}

fn spans(action: &soopy::SourceAction) -> Vec<(u64, u64, String)> {
    let soopy::SourceAction::Replace { edits, .. } = action else {
        panic!("not a Replace: {action:?}");
    };
    edits
        .iter()
        .map(|edit| {
            (
                edit.range.start,
                edit.range.end,
                String::from_utf8(edit.replacement.clone()).unwrap(),
            )
        })
        .collect()
}

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "extract_drain_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join(REL), SRC).unwrap();
    root.canonicalize().unwrap()
}

fn identity_of(root: &Path) -> soopy::DirectoryId {
    soopy::SourceRoot::open_directory(root)
        .unwrap()
        .directory()
        .identity
        .clone()
}

#[test]
fn bound_edit_maps_position_deleted_length_and_bytes() {
    let source = directory_source(&detached_identity(), REL);
    let edit: soopy::TextEdit = BoundEdit {
        source: source.clone(),
        producer: producer(),
        edit: Edit {
            position: 12,
            deleted_length: 5,
            inserted_text: b"bar()".to_vec(),
        },
    }
    .into();

    assert_eq!(edit.range.start, 12);
    assert_eq!(edit.range.end, 17);
    assert_eq!(edit.range.source, source);
    assert_eq!(edit.replacement, b"bar()");
    assert_eq!(edit.producer, producer());
}

#[test]
fn replace_all_drains_into_one_replace_action_per_file() {
    let grep = SupportLang::Rust.ast_grep(SRC);
    let edits = grep.root().replace_all("foo()", "bar()");
    assert_eq!(edits.len(), 2, "two call sites: {edits:?}");

    let source = directory_source(&detached_identity(), REL);
    let action = replace_action(
        source.clone(),
        soopy::ContentId::blake3(SRC.as_bytes()),
        bind(&source, edits),
    );

    let soopy::SourceAction::Replace {
        source: action_source,
        expected,
        ..
    } = &action
    else {
        panic!("not a Replace: {action:?}");
    };
    assert_eq!(action_source, &source);
    assert_eq!(expected, &soopy::ContentId::blake3(SRC.as_bytes()));
    assert_eq!(
        spans(&action),
        vec![(12, 17, "bar()".to_string()), (19, 24, "bar()".to_string()),],
        "src: {SRC:?}"
    );
}

#[test]
fn duplicate_spans_collapse_to_one_edit() {
    let source = directory_source(&detached_identity(), REL);
    let twice = vec![
        Edit {
            position: 12,
            deleted_length: 5,
            inserted_text: b"bar()".to_vec(),
        },
        Edit {
            position: 12,
            deleted_length: 5,
            inserted_text: b"bar()".to_vec(),
        },
    ];
    let action = replace_action(
        source.clone(),
        soopy::ContentId::blake3(SRC.as_bytes()),
        bind(&source, twice),
    );

    assert_eq!(spans(&action), vec![(12, 17, "bar()".to_string())]);
}

#[test]
fn drain_edits_matches_replace_all_and_skips_nested_matches() {
    let grep = SupportLang::Rust.ast_grep(SRC);
    let drained = drain_edits(&grep.root(), &"foo()", &"bar()");
    let by_replace_all = grep.root().replace_all("foo()", "bar()");

    let key = |edits: &[Edit<String>]| -> Vec<(usize, usize)> {
        edits
            .iter()
            .map(|edit| (edit.position, edit.deleted_length))
            .collect()
    };
    assert_eq!(key(&drained), key(&by_replace_all));

    let nested = SupportLang::Rust.ast_grep("fn main() { foo(foo()); }\n");
    let inner = drain_edits(&nested.root(), &"foo($A)", &"bar($A)");
    assert_eq!(inner.len(), 1, "the nested call is inside the outer match");
}

#[test]
fn pending_doc_appends_edits_without_mutating_the_source() {
    let source = directory_source(&detached_identity(), REL);
    let pending =
        PendingReplaceDoc::open(SRC, SupportLang::Rust, source.clone(), producer()).unwrap();
    assert_eq!(
        pending.expected(),
        &soopy::ContentId::blake3(SRC.as_bytes())
    );

    let mut root = AstGrep::doc(pending);
    assert!(root.replace("foo()", "bar()").unwrap());

    let doc = root.root().get_doc();
    assert_eq!(doc.source_text(), SRC, "do_edit never rewrites the string");
    assert_eq!(doc.edits().len(), 1);
    assert_eq!(doc.edits()[0].range.start, 12);
    assert_eq!(doc.edits()[0].range.end, 17);

    let action = root.root().get_doc().clone().into_action().unwrap();
    assert_eq!(spans(&action), vec![(12, 17, "bar()".to_string())]);
}

#[test]
fn pending_doc_with_no_match_stages_nothing() {
    let source = directory_source(&detached_identity(), REL);
    let pending = PendingReplaceDoc::open(SRC, SupportLang::Rust, source, producer()).unwrap();
    assert!(pending.into_action().is_none());
}

#[test]
fn stage_edits_reaches_soopy_with_the_drained_spans() {
    let root = temp_root("stage");
    let identity = identity_of(&root);
    let source = directory_source(&identity, REL);
    let grep = SupportLang::Rust.ast_grep(SRC);
    let request = stage_edits(
        source.clone(),
        soopy::ContentId::blake3(SRC.as_bytes()),
        bind(&source, grep.root().replace_all("foo()", "bar()")),
        soopy::SourceRootId::Directory {
            directory: identity,
        },
    );
    request.validate_shape().expect("shape holds");

    let mut source_root = soopy::SourceRoot::open_directory(&root).unwrap();
    let mut store = soopy::InMemoryStageStore::new();
    let staged = soopy::stage_mutations(&mut source_root, &request, &mut store).expect("staged");
    assert_eq!(staged.previews.len(), 1);

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn stale_expected_is_refused_by_stage() {
    let root = temp_root("stale");
    let identity = identity_of(&root);
    let source = directory_source(&identity, REL);
    let grep = SupportLang::Rust.ast_grep(SRC);
    let stale = soopy::ContentId::blake3(b"not what is on disk");
    let request = stage_edits(
        source.clone(),
        stale.clone(),
        bind(&source, grep.root().replace_all("foo()", "bar()")),
        soopy::SourceRootId::Directory {
            directory: identity,
        },
    );

    let mut source_root = soopy::SourceRoot::open_directory(&root).unwrap();
    let mut store = soopy::InMemoryStageStore::new();
    let refusal = soopy::stage_mutations(&mut source_root, &request, &mut store)
        .expect_err("a wrong expected hash cannot stage");

    let soopy::StageRefusal::Stale { inputs } = refusal else {
        panic!("expected a Stale refusal, got {refusal:?}");
    };
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].source, source);
    assert_eq!(inputs[0].expected, stale);
    assert_eq!(
        inputs[0].observed,
        Some(soopy::ContentId::blake3(SRC.as_bytes()))
    );

    std::fs::remove_dir_all(&root).unwrap();
}
