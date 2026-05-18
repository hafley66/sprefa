//! RED target: a bodied `rule(:hits,…){ fs(glob…) > ast(…) }` run via
//! `sprefa-run --root <dir>` must materialize the same rows as the
//! identical pipe written at top level. Today it materializes ZERO.
//!
//! Evidence (release sprefa-run, same binary, same --root, linux fixture):
//!   top-level  fs(glob`**/*.{c,h}`) > ast(:c)`printk($$$)`  → fs seen=93299, 16627 matches
//!   rule(:hits){ <same body> }                              → fs seen=0,     0 matches
//!
//! `rule(:greet){ \`hello world\` }` DOES materialize (see
//! rule_decl_smoke), so bodied rules run. The defect is specific to a
//! source op (`fs`) inside a rule body not seeing `ctx.root` / not
//! being seeded the way a top-level pipe is. Pre-existing on `main`;
//! not introduced by the retraction work.
//!
//! `control_*` should PASS today (proves corpus + pattern + fs/ast are
//! fine). `bodied_rule_*` is RED until the defect is fixed. The fix
//! must make the bodied-rule row count equal the top-level row count.

use std::process::Command;

fn run_sprf(script: &str) -> (String, usize) {
    // Corpus: one C file with a single printk site.
    let corpus = tempfile::tempdir().unwrap();
    std::fs::write(
        corpus.path().join("a.c"),
        "void f(void) { printk(\"hi\"); }\n",
    )
    .unwrap();
    // Script lives outside the corpus so `**/*.c` cannot pick it up.
    let scratch = tempfile::tempdir().unwrap();
    let sprf = scratch.path().join("prog.sprf");
    std::fs::write(&sprf, script).unwrap();

    let bin = env!("CARGO_BIN_EXE_sprefa-run");
    let out = Command::new(bin)
        .arg(&sprf)
        .arg("--root")
        .arg(corpus.path())
        .arg("--show-rows")
        .output()
        .expect("sprefa-run executes");
    assert!(
        out.status.success(),
        "sprefa-run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();

    // Facts listing prints `hits: N rows ...` under `── facts ──`.
    let n = stdout
        .lines()
        .find_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("hits:")?;
            rest.trim().split_whitespace().next()?.parse::<usize>().ok()
        })
        .unwrap_or_else(|| panic!("no `hits:` facts line in stdout:\n{stdout}"));
    (stdout, n)
}

/// CONTROL — top-level pipe with an explicit `fact` sink. Must pass
/// today: it proves the corpus, the `printk($$$)` pattern, `fs`, and
/// `ast(:c)` all work through `sprefa-run --root`.
#[test]
fn control_top_level_fs_materializes_hits() {
    let (stdout, n) = run_sprf(
        "fs(glob`**/*.c`) > ast(:c)`printk($$$)` > fact(:hits, FS, LO, HI)",
    );
    assert!(
        n >= 1,
        "control: top-level fs/ast must materialize >=1 hit row, got {n}\n{stdout}"
    );
}

/// RED — identical body wrapped in a bodied rule. Must materialize the
/// same row(s). Today it yields 0. The subagent's fix makes this equal
/// the control count.
#[test]
fn bodied_rule_fs_materializes_hits() {
    let (stdout, n) = run_sprf(
        "rule(:hits, :FS, :LO, :HI) {\n  fs(glob`**/*.c`) > ast(:c)`printk($$$)`\n};\n",
    );
    assert!(
        n >= 1,
        "bodied rule with fs(glob) source must materialize >=1 hit row, got {n} \
         (identical top-level pipe does — see control_top_level_fs_materializes_hits)\n{stdout}"
    );
}
