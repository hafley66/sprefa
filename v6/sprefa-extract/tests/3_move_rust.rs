//! `extract move` over a Rust corpus: the `mod` decl, the `#[path]` literal, the
//! `include!` argument, the `use` path that outlives a move, and `Cargo.toml`.
//!
//! @comment-ok: fail-first receipt, repo law keeps these in TEST headers.
//! FAIL-FIRST: every test here exits non-zero before `RustSource` joins
//! `rehomes()` (`lang/mod.rs:90`), on `extract move rehomes prolog, ts:
//! src/a.rs` out of `0_move.rs:406`, so the `move_files` success assert is the
//! line that fails.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

/// A fixture tree copied off `tests/fixtures/<rel>`, so a Rust corpus states
/// itself as files rather than as string constants.
fn fixture_tree(label: &str, source: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_move_rust_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/rust_move")
            .join(source),
        &root,
    );
    git(&root, &["init", "-q", "."]);
    git(&root, &["add", "-A"]);
    git(
        &root,
        &[
            "-c",
            "user.email=extract@move.test",
            "-c",
            "user.name=extract-move",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    Fixture {
        root: root.canonicalize().unwrap(),
        state,
    }
}

fn fixture(label: &str) -> Fixture {
    fixture_tree(label, "basic")
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read fixture dir") {
        let entry = entry.expect("fixture entry");
        let to = target.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy fixture file");
        }
    }
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git stdout is UTF-8")
}

/// The batch door over rows named relative to the fixture root, so a test states
/// its moves the way the tree spells them.
fn move_files(fixture: &Fixture, rows: &[(&str, &str)], extra: &[&str]) -> String {
    let list = fixture.state.join("moves.tsv");
    let body: String = rows
        .iter()
        .map(|(old, new)| {
            format!(
                "{}\t{}\n",
                fixture.root.join(old).display(),
                fixture.root.join(new).display()
            )
        })
        .collect();
    std::fs::write(&list, body).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg("--list")
        .arg(&list)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "extract move --list {extra:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn kind_count(table: &str, kind: &str) -> usize {
    table
        .lines()
        .filter(|line| !line.starts_with(' '))
        .filter(|line| line.split_whitespace().next() == Some(kind))
        .count()
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|error| panic!("read {rel}: {error}"))
}

const OUT_OF_DIR: [(&str, &str); 1] = [("src/a.rs", "src/util/a.rs")];
const TO_MOD_RS: [(&str, &str); 1] = [("src/a.rs", "src/a/mod.rs")];
const DEEPER: [(&str, &str); 1] = [("src/b.rs", "src/deep/b.rs")];

#[test]
fn a_mod_decl_gains_a_path_attr_when_its_file_leaves_its_dir() {
    let fixture = fixture("path_attr_grows");
    move_files(&fixture, &OUT_OF_DIR, &["--commit"]);

    assert!(
        read(&fixture.root, "src/lib.rs").contains("#[path = \"util/a.rs\"] mod a;"),
        "lib.rs re-aims the decl rustc can no longer find:\n{}",
        read(&fixture.root, "src/lib.rs")
    );
    assert!(fixture.root.join("src/util/a.rs").is_file());
}

/// `src/a.rs` and `src/a/mod.rs` are the two places rustc probes for `mod a;`,
/// so the move between them is invisible to every importer.
#[test]
fn a_move_to_the_mod_rs_form_changes_nothing() {
    let fixture = fixture("to_mod_rs");
    let before = read(&fixture.root, "src/lib.rs");
    let table = move_files(&fixture, &TO_MOD_RS, &["--commit"]);

    assert_eq!(kind_count(&table, "replace"), 0, "table:\n{table}");
    assert_eq!(kind_count(&table, "move"), 1, "table:\n{table}");
    assert_eq!(read(&fixture.root, "src/lib.rs"), before);
}

#[test]
fn an_existing_path_attr_is_respelled() {
    let fixture = fixture("path_attr_respell");
    move_files(&fixture, &DEEPER, &["--commit"]);
    let moved = read(&fixture.root, "src/deep/b.rs");

    assert!(
        moved.contains("#[path = \"../vendor/legacy.rs\"]"),
        "the attr still names src/vendor/legacy.rs from one directory deeper:\n{moved}"
    );
    assert!(
        read(&fixture.root, "src/lib.rs").contains("#[path = \"deep/b.rs\"] mod b;"),
        "and lib.rs re-aims the decl that names the moved file"
    );
}

#[test]
fn an_include_str_literal_is_respelled() {
    let fixture = fixture("include_respell");
    move_files(&fixture, &DEEPER, &["--commit"]);
    let moved = read(&fixture.root, "src/deep/b.rs");

    assert!(
        moved.contains("include_str!(\"../../data/note.txt\")"),
        "the include resolves against the moved file's own directory:\n{moved}"
    );
}

/// A `use` path names a module through the module TREE, and a `#[path]` keeps
/// that tree, so the batch that re-aims the decl leaves the `use` alone.
#[test]
fn a_use_path_survives_when_the_mod_name_survives() {
    let fixture = fixture("use_path_survives");
    let table = move_files(&fixture, &OUT_OF_DIR, &["--commit"]);
    let lib = read(&fixture.root, "src/lib.rs");

    assert!(lib.contains("use crate::a::f;"), "lib.rs:\n{lib}");
    assert_eq!(
        kind_count(&table, "replace"),
        1,
        "only the decl is rewritten:\n{table}"
    );
}

#[test]
fn cargo_toml_bin_path_follows_the_move() {
    let fixture = fixture("cargo_bin_path");
    let table = move_files(
        &fixture,
        &[("src/bin/x.rs", "src/bin/tools/x.rs")],
        &["--commit"],
    );
    let manifest = read(&fixture.root, "Cargo.toml");

    assert!(
        manifest.contains("path = \"src/bin/tools/x.rs\""),
        "the [[bin]] target follows its file:\n{manifest}"
    );
    assert!(
        manifest.contains("path = \"src/lib.rs\""),
        "and the [lib] target, naming nothing that moved, is left alone:\n{manifest}"
    );
    assert!(
        table.contains("manifest Cargo.toml: bin[0].path src/bin/x.rs -> src/bin/tools/x.rs"),
        "the run names the manifest row it rewrote:\n{table}"
    );
}

#[test]
fn dry_run_prints_every_respell_and_touches_nothing() {
    let fixture = fixture("dry_run");
    let table = move_files(&fixture, &OUT_OF_DIR, &[]);

    assert_eq!(kind_count(&table, "move"), 1, "table:\n{table}");
    assert_eq!(kind_count(&table, "replace"), 1, "table:\n{table}");
    assert!(
        table.contains("+#[path = \"util/a.rs\"] mod a;"),
        "the preview carries the line it would write:\n{table}"
    );
    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");
    assert!(fixture.root.join("src/a.rs").is_file());
    assert!(!fixture.root.join("src/util/a.rs").exists());
}

// ── the --relocate-mod strategy ─────────────────────────────────────────────

#[test]
fn relocate_mod_moves_the_decl_into_the_new_parent() {
    let fixture = fixture_tree("relocate_decl", "relocate");
    move_files(&fixture, &OUT_OF_DIR, &["--commit", "--relocate-mod"]);

    let lib = read(&fixture.root, "src/lib.rs");
    assert!(
        !lib.contains("mod a;"),
        "the old parent no longer declares the module:\n{lib}"
    );
    // Sorted ahead of `seeded`, public because lib.rs names it from outside.
    assert_eq!(
        read(&fixture.root, "src/util/mod.rs"),
        "pub mod a;\npub mod seeded;\n"
    );
}

#[test]
fn relocate_mod_respells_use_paths_crate_wide() {
    let fixture = fixture_tree("relocate_uses", "relocate");
    move_files(&fixture, &OUT_OF_DIR, &["--commit", "--relocate-mod"]);

    let lib = read(&fixture.root, "src/lib.rs");
    assert!(
        lib.contains("use crate::util::a::f;"),
        "lib.rs re-spells its use path:\n{lib}"
    );
    let b = read(&fixture.root, "src/b.rs");
    assert!(
        b.contains("super::util::a::f()"),
        "the sibling's super path grows the intermediate segment:\n{b}"
    );
}

#[test]
fn relocate_mod_with_no_parent_module_is_a_named_error() {
    let fixture = fixture_tree("relocate_missing", "relocate_missing");
    let list = fixture.state.join("moves.tsv");
    std::fs::write(
        &list,
        format!(
            "{}\t{}\n",
            fixture.root.join("src/a.rs").display(),
            fixture.root.join("src/util/a.rs").display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg("--list")
        .arg(&list)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .arg("--commit")
        .arg("--relocate-mod")
        .output()
        .expect("extract binary runs");

    assert!(
        !output.status.success(),
        "--relocate-mod without a parent module file refuses to guess"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--relocate-mod"),
        "the error names the flag and what it needed:\n{stderr}"
    );
    assert_eq!(
        git(&fixture.root, &["status", "--porcelain"]),
        "",
        "a failed plan leaves the tree untouched"
    );
}

/// The same fixture without the flag must behave exactly as #489 did: the decl
/// stays put with a `#[path]` attribute, `use crate::a::f` survives.
#[test]
fn default_strategy_is_unchanged() {
    let fixture = fixture_tree("relocate_default", "relocate");
    let table = move_files(&fixture, &OUT_OF_DIR, &["--commit"]);
    let lib = read(&fixture.root, "src/lib.rs");

    assert!(
        lib.contains("#[path = \"util/a.rs\"] mod a;"),
        "lib.rs:\n{lib}"
    );
    assert!(lib.contains("use crate::a::f;"), "lib.rs:\n{lib}");
    assert_eq!(kind_count(&table, "replace"), 1, "table:\n{table}");
}

/// Oracle over the whole strategy, judged by rustc. MEASURED 2026-08-27:
/// cold `cargo check` on the relocated fixture ran in under 10 s, so it runs
/// in-battery.
#[test]
fn relocate_mod_fixture_compiles() {
    let fixture = fixture_tree("relocate_oracle", "relocate");
    move_files(&fixture, &OUT_OF_DIR, &["--commit", "--relocate-mod"]);
    let check = Command::new("cargo")
        .args(["check"])
        .current_dir(&fixture.root)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .output()
        .expect("cargo runs");
    assert!(
        check.status.success(),
        "cargo check on the relocated fixture: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

// ── the self-move oracle ────────────────────────────────────────────────────

/// The verb run against this crate's own tree, judged by rustc, not by an
/// assertion. MEASURED 2026-08-26: 20.2 s, over the 10-second cap, so it runs by
/// hand: `cargo test --features cli --test 3_move_rust -- --ignored`.
#[test]
#[ignore]
fn moving_this_crates_own_module_leaves_it_compiling() {
    let base = std::env::temp_dir().join(format!("extract_move_rust_self_{}", std::process::id()));
    let root = base.join("sprefa-extract");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    copy_crate(source, &root);
    re_aim_path_deps(source, &root.join("Cargo.toml"));
    git(&root, &["init", "-q", "."]);
    git(&root, &["add", "-A"]);
    git(
        &root,
        &[
            "-c",
            "user.email=extract@move.test",
            "-c",
            "user.name=extract-move",
            "commit",
            "-qm",
            "self move oracle",
        ],
    );
    let root = root.canonicalize().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg(root.join("src/lang/ts_rehome.rs"))
        .arg(root.join("src/lang/ts/rehome.rs"))
        .arg("--root")
        .arg(&root)
        .arg("--state")
        .arg(&state)
        .arg("--commit")
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "extract move exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        read(&root, "src/lang/mod.rs").contains("#[path = \"ts/rehome.rs\"] pub mod ts_rehome;"),
        "the roster's decl re-aims:\n{}",
        read(&root, "src/lang/mod.rs")
    );

    let check = Command::new("cargo")
        .args(["check", "--features", "cli"])
        .current_dir(&root)
        .output()
        .expect("cargo runs");
    assert!(
        check.status.success(),
        "cargo check on the moved tree: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// The crate's sources, minus the build output and the git store the copy mints
/// for itself.
fn copy_crate(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read crate dir") {
        let entry = entry.expect("crate entry");
        let name = entry.file_name();
        if matches!(name.to_string_lossy().as_ref(), ".git" | "target") {
            continue;
        }
        let to = target.join(&name);
        if entry.file_type().expect("file type").is_dir() {
            copy_crate(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy crate file");
        }
    }
}

/// A copy sits at a different depth, so its two sibling path dependencies are
/// re-pointed at the originals before cargo reads them.
fn re_aim_path_deps(source: &Path, manifest: &Path) {
    let text = std::fs::read_to_string(manifest).expect("read manifest");
    let mut out = text;
    for (rel, name) in [
        ("../../../hafley-rs/crates/soopy", "soopy"),
        ("../../../hafley-rs/crates/hafley-observe", "hafley-observe"),
    ] {
        let absolute = source.join(rel).canonicalize().expect(name);
        out = out.replace(rel, &absolute.to_string_lossy());
    }
    std::fs::write(manifest, out).expect("write manifest");
}
