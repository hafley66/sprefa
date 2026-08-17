//! FAIL-PRE-FIX: against the unfixed code the digest branch discovers the
//! repository from `"."` (the cwd), so running from `temp_dir` reads the wrong
//! repository and `soopy::discover` fails -- the process exits 2 with one
//! stderr line carrying `git cat-file blob ... not a git repository`.
//!
//! SABOTAGE 1, drop the path from `cat_blob` and discover from `"."`: this test
//! goes RED because the cwd is not the path's repository.
//! SABOTAGE 2, discover from `path` rather than `path.parent()`: a file path
//! that is not a directory fails to discover.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn build() -> Self {
        let root = std::env::temp_dir().join(format!(
            "query_digest_repo_from_path_{}_{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("fixture directory");
        let fixture = Fixture { root };
        fixture.git(&["init", "-q"]);
        fixture.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
        fixture.commit("sample.rs", "pub fn trim(value: String) -> String {\n    value\n}\n");
        fixture
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "query-digest-repo-from-path")
            .env("GIT_AUTHOR_EMAIL", "query-digest-repo-from-path@example.invalid")
            .env("GIT_COMMITTER_NAME", "query-digest-repo-from-path")
            .env("GIT_COMMITTER_EMAIL", "query-digest-repo-from-path@example.invalid")
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn commit(&self, name: &str, body: &str) {
        std::fs::write(self.root.join(name), body).expect("write fixture file");
        self.git(&["add", "."]);
        self.git(&["commit", "-qm", body]);
    }

    fn blob_oid(&self, path: &str) -> String {
        self.git(&["rev-parse", &format!("HEAD:{path}")])
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn query_digest_reads_the_blob_from_the_repo_holding_the_path() {
    let fixture = Fixture::build();
    let source = fixture.root.join("sample.rs");
    let oid = fixture.blob_oid("sample.rs");

    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(std::env::temp_dir())
        .args([
            "query",
            "--lang",
            "rust",
            "--query",
            "(function_item name: (identifier) @name) @item",
            "--digest",
            &oid,
            source.to_str().unwrap(),
        ])
        .output()
        .expect("extract binary runs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"name\":\"trim\""), "stdout: {stdout}");
}
