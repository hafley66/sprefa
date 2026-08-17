use std::process::Command;

use sprefa_engine_rs::source_bind::{
    SourceBind, SourceBindRelations, BLOB_COLUMNS, FILE_COLUMNS, REPO_COLUMNS, REV_COLUMNS,
    SPAN_COLUMNS, SPECIFIER_COLUMNS,
};
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::SqlStatement;
use sprefa_engine_rs::{program::run_boot, result_rows, ArrivalSign, GenProgram, RowColumnType};
use sprefa_rust_runtime_host::{
    ClockedSourceHostRequest, ReadRequestWire, SourceFilesDemand, SourceHostDemand,
    SourceHostOutcome, SourceHostSuccess, SourceIdentityDemand,
};
use tempfile::tempdir;

fn git(root: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "source-bind")
        .env("GIT_AUTHOR_EMAIL", "source-bind@example.invalid")
        .env("GIT_COMMITTER_NAME", "source-bind")
        .env("GIT_COMMITTER_EMAIL", "source-bind@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn fixture() -> tempfile::TempDir {
    let root = tempdir().unwrap();
    git(root.path(), &["init", "-q", "-b", "main"]);
    std::fs::write(
        root.path().join("main.dl6"),
        b"use \"other.dl6\" as other.\nrel app.\n",
    )
    .unwrap();
    git(root.path(), &["add", "main.dl6"]);
    git(root.path(), &["commit", "-q", "-m", "initial"]);
    root
}

fn emitted_source_program() -> GenProgram {
    let path = format!(
        "{}/tests/fixtures/source-offline-golden.program.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let raw = text
        .split_once("pub const PROGRAM_JSON: &str = r")
        .unwrap()
        .1;
    let quote = raw.find('"').unwrap();
    let closing = format!("\"{};", &raw[..quote]);
    let json = &raw[quote + 1..raw[quote + 1..].find(&closing).unwrap() + quote + 1];
    GenProgram::from_json(serde_json::from_str(json).unwrap())
}

fn decoded_rows(
    program: &GenProgram,
    seam: &SqliteSeam,
    relation: &str,
) -> Vec<Vec<sprefa_engine_rs::Value>> {
    let result = seam
        .execute(&SqlStatement {
            sql: program.final_select[relation].clone(),
            args: Vec::new(),
        })
        .unwrap();
    result_rows(
        &result,
        &program.rel_columns[relation],
        &program.rel_column_types[relation],
    )
    .expect("boundary rows")
}

fn worktree_entry(root: &std::path::Path) -> soopy::SourceEntry {
    let repository = soopy::open(root).unwrap();
    let mut tree = soopy::SourceTree::open(repository);
    tree.snapshot(&soopy::SourceQuery {
        revision: soopy::Revision::Worktree,
        patterns: vec![soopy::Pattern("**/*.dl6".into())],
    })
    .unwrap()
    .files
    .remove(0)
}

fn identity(clock: u64, entry: soopy::SourceEntry) -> ClockedSourceHostRequest {
    ClockedSourceHostRequest {
        clock,
        demand: SourceHostDemand::Identity(SourceIdentityDemand {
            reads: vec![ReadRequestWire {
                source: entry.source.clone(),
                expected: Some(entry.content),
            }],
            spans: vec![soopy::SourceSpan {
                source: entry.source,
                start: 0,
                end: 3,
            }],
            retire_sources: Vec::new(),
            store_git_bytes: false,
        }),
    }
}

#[test]
fn authored_source_contract_has_only_source_values() {
    let relations = SourceBindRelations::default();
    let declarations = relations.declarations();
    assert_eq!(declarations[0].columns, REPO_COLUMNS);
    assert_eq!(declarations[1].columns, REV_COLUMNS);
    assert_eq!(declarations[2].columns, BLOB_COLUMNS);
    assert_eq!(declarations[3].columns, FILE_COLUMNS);
    assert_eq!(declarations[4].columns, SPAN_COLUMNS);
    assert_eq!(declarations[5].columns, SPECIFIER_COLUMNS);
    assert_eq!(
        declarations[3].column_types,
        &[RowColumnType::Ref, RowColumnType::Text, RowColumnType::Ref]
    );
    assert!(!declarations
        .iter()
        .flat_map(|declaration| declaration.columns)
        .any(|column| column.ends_with("_id")));
}

#[test]
fn source_inputs_keep_directories_worktrees_and_clones_distinct() {
    let plain = tempdir().unwrap();
    std::fs::write(plain.path().join("plain.txt"), b"plain\n").unwrap();
    let main = fixture();
    let linked_path = main.path().with_extension("linked");
    git(
        main.path(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            linked_path.to_str().unwrap(),
        ],
    );
    let clone = tempdir().unwrap();
    let output = Command::new("git")
        .args([
            "clone",
            "-q",
            main.path().to_str().unwrap(),
            clone.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let mut bind = SourceBind::in_memory(SourceBindRelations::default()).unwrap();
    let directory = bind.register_directory(plain.path()).unwrap();
    let snapshot = bind
        .directory_snapshot(&directory, &soopy::FileQuery::default())
        .unwrap();
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].file.path.0.as_ref(), "plain.txt");

    let main_registration = bind.register_git(main.path()).unwrap();
    let linked_registration = bind.register_git(&linked_path).unwrap();
    let clone_registration = bind.register_git(clone.path()).unwrap();
    assert_eq!(main_registration.repository, linked_registration.repository);
    assert_ne!(main_registration.worktree, linked_registration.worktree);
    assert_ne!(main_registration.repository, clone_registration.repository);

    std::fs::write(linked_path.join("main.dl6"), b"changed\n").unwrap();
    let main_state = bind
        .tracked_state(
            &main_registration.worktree,
            &soopy::GitFileQuery {
                pathspecs: vec!["main.dl6".to_string()],
                ..Default::default()
            },
        )
        .unwrap();
    let linked_state = bind
        .tracked_state(
            &linked_registration.worktree,
            &soopy::GitFileQuery {
                pathspecs: vec!["main.dl6".to_string()],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        main_state.observations[0].state,
        soopy::TrackedFileState::Clean
    );
    assert_eq!(
        linked_state.observations[0].state,
        soopy::TrackedFileState::Unstaged
    );

    git(
        main.path(),
        &[
            "worktree",
            "remove",
            "--force",
            linked_path.to_str().unwrap(),
        ],
    );
}

#[test]
fn unregistered_soopy_read_is_a_typed_bind_error() {
    let fixture = fixture();
    let entry = worktree_entry(fixture.path());
    let mut bind = SourceBind::in_memory(SourceBindRelations::default()).unwrap();
    assert!(matches!(
        bind.execute(identity(1, entry)),
        Err(sprefa_engine_rs::source_bind::SourceBindError::SourceRead { .. })
    ));
}

#[test]
fn offline_git_files_blob_extract_span_and_tick_retraction() {
    let fixture = fixture();
    let initial = worktree_entry(fixture.path());
    let program = emitted_source_program();
    let relations = SourceBindRelations::default();
    let seam = SqliteSeam::in_memory().unwrap();
    seam.run_ddl(&program.ddl).unwrap();
    run_boot(&seam, &program.boot);
    let mut bind = SourceBind::in_memory(relations).unwrap();
    let repository = bind.register_root(fixture.path()).unwrap();

    let head = match initial.source.revision.clone() {
        soopy::RevisionId::Worktree {
            head: Some(head), ..
        } => head,
        other => panic!("expected committed worktree: {other:?}"),
    };
    let files = bind
        .execute(ClockedSourceHostRequest {
            clock: 0,
            demand: SourceHostDemand::Files(SourceFilesDemand::RepoFilesAt {
                repository,
                revision: head,
                pathspecs: vec!["*.dl6".to_string()],
            }),
        })
        .unwrap();
    assert!(
        matches!(files.envelope.outcome, SourceHostOutcome::Success(SourceHostSuccess::Files(ref rows)) if rows.len() == 1 && rows[0].path == "main.dl6" && matches!(rows[0].content, soopy::ContentId::GitBlob(_)))
    );

    let first = bind
        .run_tick(&program, &seam, identity(1, initial))
        .unwrap();
    assert!(first
        .source
        .arrivals
        .iter()
        .any(|arrival| arrival.rel == "file" && matches!(arrival.sign, ArrivalSign::Add)));
    assert!(first
        .source
        .arrivals
        .iter()
        .any(|arrival| arrival.rel == "span" && matches!(arrival.sign, ArrivalSign::Add)));
    assert!(first.source.arrivals.iter().any(
        |arrival| arrival.rel == "source_specifier" && matches!(arrival.sign, ArrivalSign::Add)
    ));
    let specifier_owner = first
        .source
        .arrivals
        .iter()
        .find(|arrival| arrival.rel == "source_specifier")
        .and_then(|arrival| match &arrival.row[0] {
            sprefa_engine_rs::Value::Text(value) => Some(value),
            _ => None,
        })
        .unwrap();
    let specifier_owner = serde_json::from_str::<serde_json::Value>(specifier_owner).unwrap();
    assert_eq!(specifier_owner["file"]["path"], "main.dl6");
    assert!(specifier_owner["file"]["rev"]["repo"]["root"]
        .as_str()
        .unwrap()
        .ends_with(fixture.path().file_name().unwrap().to_str().unwrap()));
    assert!(specifier_owner["file"]["blob"]["oid"]
        .as_str()
        .unwrap()
        .starts_with("blake3:"));
    assert!(
        specifier_owner["start_byte"].as_u64().unwrap()
            < specifier_owner["end_byte"].as_u64().unwrap()
    );
    for relation in [
        "repo",
        "rev",
        "blob",
        "file",
        "span",
        "source_specifier",
        "dependency",
    ] {
        assert!(
            !decoded_rows(&program, &seam, relation).is_empty(),
            "{relation}"
        );
    }
    assert!(decoded_rows(&program, &seam, "repo").iter().any(|row| matches!(&row[0], sprefa_engine_rs::Value::Text(root) if root.ends_with(fixture.path().file_name().unwrap().to_str().unwrap()))));
    assert!(decoded_rows(&program, &seam, "file")
        .iter()
        .any(|row| matches!(&row[1], sprefa_engine_rs::Value::Text(path) if path == "main.dl6")));
    let source_specifiers = decoded_rows(&program, &seam, "source_specifier");
    let dependencies = decoded_rows(&program, &seam, "dependency");
    assert_eq!(
        program.rel_columns["source_specifier"],
        ["owner", "module", "name", "kind"]
    );
    assert_eq!(
        program.rel_columns["dependency"],
        ["owner", "target", "binding", "kind"]
    );
    assert_eq!(source_specifiers, dependencies);
    assert!(source_specifiers.iter().any(|row| matches!(&row[0], sprefa_engine_rs::Value::Text(owner) if serde_json::from_str::<serde_json::Value>(owner).unwrap()["file"]["path"] == "main.dl6")));

    std::fs::write(
        fixture.path().join("main.dl6"),
        b"use \"next.dl6\" as next.\nrel changed.\n",
    )
    .unwrap();
    let replacement = worktree_entry(fixture.path());
    let second = bind
        .run_tick(&program, &seam, identity(2, replacement))
        .unwrap();
    for relation in ["file", "span", "source_specifier", "dependency"] {
        let delta = second
            .deltas
            .rels
            .iter()
            .find(|delta| delta.rel == relation)
            .unwrap();
        assert!(!delta.add.is_empty(), "{relation} additions: {delta:?}");
        assert!(!delta.del.is_empty(), "{relation} removals: {delta:?}");
    }
    assert!(decoded_rows(&program, &seam, "dependency").iter().any(|row| matches!(&row[0], sprefa_engine_rs::Value::Text(owner) if serde_json::from_str::<serde_json::Value>(owner).unwrap()["file"]["path"] == "main.dl6")));
}

#[test]
fn persistent_receipts_retract_after_runtime_restart_before_replacement_additions() {
    let fixture = fixture();
    let identity_store = tempfile::NamedTempFile::new().unwrap();
    let program = emitted_source_program();
    let seam = SqliteSeam::in_memory().unwrap();
    seam.run_ddl(&program.ddl).unwrap();
    run_boot(&seam, &program.boot);

    let initial = worktree_entry(fixture.path());
    let first = {
        let mut bind =
            SourceBind::open(identity_store.path(), SourceBindRelations::default()).unwrap();
        bind.register_root(fixture.path()).unwrap();
        bind.run_tick(&program, &seam, identity(1, initial))
            .unwrap()
    };
    let first_additions = first.source.arrivals;
    assert!(first_additions.iter().any(|arrival| arrival.rel == "file"));
    assert!(first_additions.iter().any(|arrival| arrival.rel == "span"));
    assert!(first_additions
        .iter()
        .any(|arrival| arrival.rel == "source_specifier"));

    std::fs::write(
        fixture.path().join("main.dl6"),
        b"use \"next.dl6\" as next.\nrel changed.\n",
    )
    .unwrap();
    let replacement = worktree_entry(fixture.path());
    let mut bind = SourceBind::open(identity_store.path(), SourceBindRelations::default()).unwrap();
    bind.register_root(fixture.path()).unwrap();
    let second = bind
        .run_tick(&program, &seam, identity(2, replacement.clone()))
        .unwrap();

    let deletion_count = second
        .source
        .arrivals
        .iter()
        .take_while(|arrival| arrival.sign == ArrivalSign::Del)
        .count();
    assert_eq!(deletion_count, first_additions.len());
    assert_eq!(
        second.source.arrivals[..deletion_count]
            .iter()
            .map(|arrival| (&arrival.rel, &arrival.row))
            .collect::<Vec<_>>(),
        first_additions
            .iter()
            .map(|arrival| (&arrival.rel, &arrival.row))
            .collect::<Vec<_>>(),
        "the empty-runtime process reconstructs the exact authored deletion projection"
    );
    assert!(second.source.arrivals[deletion_count..]
        .iter()
        .all(|arrival| arrival.sign == ArrivalSign::Add));
    for relation in ["file", "span", "source_specifier", "dependency"] {
        let delta = second
            .deltas
            .rels
            .iter()
            .find(|delta| delta.rel == relation)
            .unwrap();
        assert!(!delta.del.is_empty(), "{relation} deletions: {delta:?}");
        assert!(!delta.add.is_empty(), "{relation} additions: {delta:?}");
    }
    let specifiers = decoded_rows(&program, &seam, "source_specifier");
    assert!(
        specifiers.iter().any(
            |row| matches!(&row[1], sprefa_engine_rs::Value::Text(module) if module == "next.dl6")
        ),
        "replacement specifiers: {specifiers:?}"
    );
    assert!(
        !specifiers.iter().any(
            |row| matches!(&row[1], sprefa_engine_rs::Value::Text(module) if module == "other.dl6")
        ),
        "stale specifiers: {specifiers:?}"
    );

    let replay = bind
        .run_tick(&program, &seam, identity(3, replacement))
        .unwrap();
    assert!(replay.source.arrivals.is_empty());
    assert!(replay
        .deltas
        .rels
        .iter()
        .all(|delta| delta.add.is_empty() && delta.del.is_empty()));
}
