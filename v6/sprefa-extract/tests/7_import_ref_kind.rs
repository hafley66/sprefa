//! Rails for the `ImportRefKind` enum and the `Rehome` moved-names seam.
//! The kind tags are frozen wire text (`as_str` feeds receipts and the scip
//! disagreement detail), so `as_str` must stay byte-stable. The moved-names
//! shape lives once on the trait; each language answers only its own
//! directory-standing file name.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use sprefa_extract::lang::prolog::PrologSource;
use sprefa_extract::lang::rust::RustSource;
use sprefa_extract::lang::rust_rehome::{INCLUDE as RUST_INCLUDE, USE_PATH as RUST_USE_PATH};
use sprefa_extract::lang::ts::TsSource;
use sprefa_extract::{ImportRefKind, MoveCx, Rehome};

#[test]
fn import_ref_kind_as_str_is_byte_stable() {
    assert_eq!(ImportRefKind::Import.as_str(), "import");
    assert_eq!(ImportRefKind::PathLiteral.as_str(), "path_literal");
    assert_eq!(ImportRefKind::ManifestTarget.as_str(), "manifest_target");
    assert_eq!(RUST_INCLUDE.as_str(), "include");
    assert_eq!(RUST_USE_PATH.as_str(), "use_path");
}

#[test]
fn no_ext_import_ref_tag_collides_with_a_core_tag() {
    const CORE_TAGS: &[&str] = &["import", "path_literal", "manifest_target"];
    let ext = [
        ("rust::INCLUDE", RUST_INCLUDE.as_str()),
        ("rust::USE_PATH", RUST_USE_PATH.as_str()),
    ];
    for (name, tag) in ext {
        assert!(
            !CORE_TAGS.contains(&tag),
            "ext {name} tag '{tag}' collides with a core tag"
        );
    }
}

fn moved_names_root() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("import_ref_kind_stem_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn batch(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(old, new)| (old.to_string(), new.to_string()))
        .collect()
}

#[test]
fn moved_names_uses_the_language_directory_stem() {
    let root = moved_names_root();
    for rel in ["src/a/mod.rs", "src/a/index.ts", "src/a.pl"] {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }
    let cx =
        |moved: BTreeMap<String, String>| MoveCx::open(&root).unwrap().with_batch(moved, false);

    let rust = cx(batch(&[("src/a/mod.rs", "src/b/mod.rs")]));
    assert_eq!(
        RustSource.moved_names(&rust),
        BTreeSet::from(["a".to_string(), "mod".to_string()]),
    );

    let ts = cx(batch(&[("src/a/index.ts", "src/b/index.ts")]));
    assert_eq!(
        TsSource.moved_names(&ts),
        BTreeSet::from(["a".to_string(), "index".to_string()]),
    );

    let prolog = cx(batch(&[("src/a.pl", "src/b.pl")]));
    assert_eq!(
        PrologSource.moved_names(&prolog),
        BTreeSet::from(["a".to_string()]),
    );
    let _ = Path::new("");
}

#[test]
fn stem_and_moved_names_live_once() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let mut owners = Vec::new();
    for entry in walk(&format!("{manifest}/src")) {
        let text = std::fs::read_to_string(&entry).unwrap();
        if text.contains("fn stem(") {
            owners.push(entry.clone());
        }
        if text.contains("fn moved_names(") {
            owners.push(entry);
        }
    }
    let unexpected: Vec<_> = owners
        .iter()
        .filter(|path| !path.ends_with("move_cx.rs") && !path.ends_with("types.rs"))
        .collect();
    assert!(
        unexpected.is_empty(),
        "stem/moved_names duplicated outside the seam: {unexpected:?}"
    );
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
