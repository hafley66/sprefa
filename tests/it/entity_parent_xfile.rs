//! Cross-file `impl` parent resolution (CLAUDE.md "Cross-file `impl` parents
//! dangle"). The extractor mints a method's `parent` file-scoped
//! (`<method-file>::<kind>::<Owner>`), which joins the owner's entity sym only
//! when the owner is declared in the SAME file. A Rust `impl Engine` in a file
//! other than `struct Engine` dangled: the minted key named the impl file, the
//! owner entity named the decl file, so `type_entity.parent` had no join.
//!
//! `refresh_type_rels` now rewrites a dangling parent via the same in-repo
//! name-unique bucket that resolves `type_link`/`call_edge` dst syms:
//!  - unique in-repo owner def  -> parent becomes the declaring-file sym
//!  - ambiguous (same name in two files) -> parent stays file-scoped/dangling
//!    (an honest dangle beats an arbitrary wrong join).

use std::fs;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git").current_dir(dir).args(args).output().expect("git").status.success();
    assert!(ok, "git {args:?} in {}", dir.display());
}

fn init_repo(root: &std::path::Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@t"]);
    git(root, &["config", "user.name", "t"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "x"]);
}

/// Run the one-shot type-entity dump over a repo root, return stdout.
fn dump_entities(dir: &std::path::Path, root: &std::path::Path) -> String {
    fs::write(
        dir.join("p.dl"),
        "rel seen(path: file).\n\
         seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev).\n\
         ? type_entity(repo, sym, name, kind, parent, file, line).\n",
    )
    .unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .current_dir(root)
        .args(["--no-daemon", "--db", dir.join("db").to_str().unwrap()])
        .output()
        .expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "run failed: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    stdout
}

/// The `parent` column (index 4) of the type_entity row whose `sym` (index 1)
/// ends with `sym_suffix`. Panics if not found.
fn parent_of(stdout: &str, sym_suffix: &str) -> String {
    for line in stdout.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() >= 5 && cols[1].ends_with(sym_suffix) {
            return cols[4].to_string();
        }
    }
    panic!("no type_entity row for sym *{sym_suffix}:\n{stdout}");
}

#[test]
fn cross_file_impl_parent_resolves_to_declaring_file_sym() {
    let d = std::env::temp_dir().join("entity_parent_xfile_unique");
    let _ = fs::remove_dir_all(&d);
    let root = d.join("solo");
    fs::create_dir_all(root.join("src")).unwrap();
    // struct Engine and `impl Engine { fn tick }` live in DIFFERENT files.
    fs::write(root.join("src/engine.rs"), "pub struct Engine { pub n: i64 }\n").unwrap();
    fs::write(root.join("src/tick.rs"), "impl Engine { pub fn tick(&self) {} }\n").unwrap();
    init_repo(&root);

    let stdout = dump_entities(&d, &root);

    // The owner's own entity sym (repo-qualified, struct kind, engine.rs file).
    let engine_sym = "solo::src/engine.rs::struct::Engine";
    // tick's parent must be the DECLARING-file sym, not the impl-file key.
    let tick_parent = parent_of(&stdout, "src/tick.rs::method::Engine.tick");
    assert_eq!(
        tick_parent, engine_sym,
        "cross-file impl parent joins the declaring-file struct sym:\n{stdout}"
    );
    // And that sym must actually exist as a type_entity row (a real join).
    assert!(
        stdout.lines().any(|l| l.split('\t').nth(1) == Some(engine_sym)),
        "owner entity row present:\n{stdout}"
    );
}

#[test]
fn ambiguous_cross_file_owner_stays_file_scoped() {
    let d = std::env::temp_dir().join("entity_parent_xfile_ambig");
    let _ = fs::remove_dir_all(&d);
    let root = d.join("dup");
    fs::create_dir_all(root.join("src")).unwrap();
    // TWO structs named Engine in two files of ONE repo -> the owner name is
    // ambiguous in-repo, so the impl's parent must NOT pick one arbitrarily.
    fs::write(root.join("src/a.rs"), "pub struct Engine { pub a: i64 }\n").unwrap();
    fs::write(root.join("src/b.rs"), "pub struct Engine { pub b: i64 }\n").unwrap();
    fs::write(root.join("src/c.rs"), "impl Engine { pub fn tick(&self) {} }\n").unwrap();
    init_repo(&root);

    let stdout = dump_entities(&d, &root);

    let tick_parent = parent_of(&stdout, "src/c.rs::method::Engine.tick");
    // Stays the file-scoped key (the impl's own file), i.e. dangling — never
    // resolved to either real Engine.
    assert_eq!(
        tick_parent, "dup::src/c.rs::struct::Engine",
        "ambiguous owner keeps the file-scoped parent:\n{stdout}"
    );
    assert_ne!(tick_parent, "dup::src/a.rs::struct::Engine", "did not pick a.rs:\n{stdout}");
    assert_ne!(tick_parent, "dup::src/b.rs::struct::Engine", "did not pick b.rs:\n{stdout}");
}
