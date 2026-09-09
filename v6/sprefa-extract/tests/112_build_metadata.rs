use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn run(dir: &Path, program: &str, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|error| panic!("run {program}: {error}"))
}

fn must_run(dir: &Path, program: &str, args: &[&str]) -> Output {
    let output = run(dir, program, args);
    assert!(
        output.status.success(),
        "{program} {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn commit(dir: &Path, message: &str) {
    must_run(dir, "git", &["add", "."]);
    must_run(
        dir,
        "git",
        &[
            "-c",
            "user.name=sprefa-test",
            "-c",
            "user.email=sprefa-test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            message,
        ],
    );
}

fn metadata(dir: &Path) -> (String, String) {
    let output = Command::new("cargo")
        .args(["run", "--quiet"])
        .current_dir(dir)
        .env_remove("CARGO_TARGET_DIR")
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .output()
        .expect("run isolated cargo");
    assert!(
        output.status.success(),
        "cargo run: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let line = String::from_utf8(output.stdout).expect("metadata is UTF-8");
    let (hash, datetime) = line.trim().split_once(' ').expect("hash and datetime");
    (hash.to_string(), datetime.to_string())
}

#[test]
fn build_metadata_tracks_source_and_checked_out_branch() {
    let root = std::env::temp_dir().join(format!(
        "sprefa-build-metadata-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).expect("miniature crate");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"build-metadata-probe\"\nversion = \"0.0.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n",
    )
    .expect("manifest");
    std::fs::write(root.join("build.rs"), include_str!("../build.rs")).expect("build script");
    std::fs::write(
        root.join("src/main.rs"),
        "fn main() { println!(\"{} {}\", env!(\"SPREFA_BUILD_GIT_HASH\"), env!(\"SPREFA_BUILD_DATETIME\")); }\n",
    )
    .expect("main");
    must_run(&root, "git", &["init", "--quiet"]);
    commit(&root, "initial");

    let (initial_hash, initial_datetime) = metadata(&root);
    std::thread::sleep(Duration::from_secs(1));
    std::fs::write(
        root.join("src/main.rs"),
        "// source changed\nfn main() { println!(\"{} {}\", env!(\"SPREFA_BUILD_GIT_HASH\"), env!(\"SPREFA_BUILD_DATETIME\")); }\n",
    )
    .expect("changed main");
    let (source_hash, source_datetime) = metadata(&root);
    assert_eq!(source_hash, initial_hash);
    assert_ne!(source_datetime, initial_datetime);

    commit(&root, "source changed");
    let (committed_hash, _) = metadata(&root);
    assert_ne!(committed_hash, source_hash);
}
