//! Turnkey config: a `[[repos]]` TOML at `$SPREFA_CONFIG` populates the `repo`
//! relation. (File ingestion from the extra roots is a later step; this proves
//! the config is loaded and drives the built-in `repo` relation.)

use std::fs;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

#[test]
fn config_repos_populate_the_repo_relation() {
    let d = std::env::temp_dir().join("cfg_repos_test");
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/lib.rs"), "fn main() {}\n").unwrap();
    fs::write(d.join("cfg.toml"), "\
        [[repos]]\n\
        slug = \"alpha/one\"\n\
        root = \"/tmp/alpha\"\n\
        [[repos]]\n\
        slug = \"beta/two\"\n\
        root = \"/tmp/beta\"\n").unwrap();
    fs::write(d.join("p.dl"), "\
        rel src(p: file).\n\
        src(p) <- scan(\"WORK\", \"src/**/*.rs\", p, rev).\n\
        ? repo(id, slug, root).\n").unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--root", d.to_str().unwrap(), "--db", d.join("db").to_str().unwrap()])
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "run failed: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));

    let block = stdout.split("? repo").nth(1).unwrap_or("");
    assert!(block.contains("alpha/one\talpha/one\t/tmp/alpha"), "repo alpha from config: {stdout}");
    assert!(block.contains("beta/two\tbeta/two\t/tmp/beta"), "repo beta from config: {stdout}");
    assert!(block.contains("(2 rows)"), "exactly the two configured repos: {stdout}");

    // No config -> the single --root repo (one row), not the configured two.
    let out2 = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--root", d.to_str().unwrap(), "--db", d.join("db2").to_str().unwrap()])
        .env_remove("SPREFA_CONFIG")
        .env("XDG_CONFIG_HOME", d.join("noconfig").to_str().unwrap())
        .env("HOME", d.join("noconfig").to_str().unwrap())
        .output().expect("run dl");
    let stdout2 = String::from_utf8_lossy(&out2.stdout);
    let block2 = stdout2.split("? repo").nth(1).unwrap_or("");
    assert!(block2.contains("(1 rows)"), "no config -> single --root repo: {stdout2}");
}
