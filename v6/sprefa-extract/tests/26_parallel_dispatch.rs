//! CONTROL: 1 passed. The parallel map's answer does not depend on how many
//! workers ran it. `SPREFA_EXTRACT_THREADS=1` and the unset default produce
//! byte-identical `--resolve` output over a three-file Git fixture whose files
//! call each other, so the equality is over 3 resolved_edge rows and not over
//! two empty runs.
//!
//! FAIL-PRE-FIX: written against the fixture's original bodies (`pub fn a() {}`
//! and friends), which resolve to NOTHING; the row-count guard caught it,
//! `resolve emitted no output`, 0 passed 1 failed. That is the receipt that the
//! guard works.
//!
//! SABOTAGE 1, collect the parallel results without preserving index order:
//! caught in `src/project.rs` by `read_inputs_preserves_path_order`, not here.
//! `read_inputs` is `pub(crate)` and `sorted_lines` (project.rs) sorts the wire
//! output, so input order is not observable from outside the crate at all.
//! SABOTAGE 2, drop the `saturating_sub(1)` from the cap: caught in
//! `src/project.rs` by `thread_cap_honors_the_request_then_clamps`. Thread
//! COUNT is likewise not observable from out here; only its answer is.

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
            "parallel_dispatch_{}_{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("fixture directory");
        let fixture = Fixture { root };
        fixture.git(&["init", "-q"]);
        fixture.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
        // Cross-file callers, so `--resolve` has resolved_edge rows to emit and
        // the equality assertion below is not comparing two empty strings.
        fixture.commit("a.rs", "pub fn alpha() -> i32 {\n    1\n}\n");
        fixture.commit("b.rs", "pub fn beta() -> i32 {\n    alpha() + 1\n}\n");
        fixture.commit("c.rs", "pub fn gamma() -> i32 {\n    beta() + alpha()\n}\n");
        fixture
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "parallel-dispatch")
            .env("GIT_AUTHOR_EMAIL", "parallel-dispatch@example.invalid")
            .env("GIT_COMMITTER_NAME", "parallel-dispatch")
            .env("GIT_COMMITTER_EMAIL", "parallel-dispatch@example.invalid")
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
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn resolve_output(fixture: &Fixture, threads: Option<&str>) -> String {
    let mut args: Vec<String> = vec!["--resolve".to_string()];
    args.extend(
        ["a.rs", "b.rs", "c.rs"]
            .map(|name| fixture.root.join(name).to_str().unwrap().to_string()),
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_extract"));
    command.current_dir(std::env::temp_dir()).args(&args);
    if let Some(threads) = threads {
        command.env("SPREFA_EXTRACT_THREADS", threads);
    }
    let output = command.output().expect("extract binary runs");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn single_thread_matches_default_cap() {
    let fixture = Fixture::build();
    let default = resolve_output(&fixture, None);
    let capped_one = resolve_output(&fixture, Some("1"));
    assert_eq!(
        default.lines().count(),
        3,
        "the fixture must emit resolved edges, else this compares two empty runs: {default}"
    );
    assert_eq!(default, capped_one);
}
