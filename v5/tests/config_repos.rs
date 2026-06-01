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

fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git").current_dir(dir).args(args).output().expect("git").status.success();
    assert!(ok, "git {args:?} in {}", dir.display());
}

/// A configured repo whose `root` is not on disk but has a `url` is cloned on
/// first scan, then its committed rev is ingested into the `file` relation.
#[test]
fn config_repo_with_url_is_cloned_on_first_scan() {
    let d = std::env::temp_dir().join("cfg_clone_test");
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();

    // The upstream we clone from: a real git repo with one committed file.
    let upstream = d.join("upstream");
    fs::create_dir_all(upstream.join("src")).unwrap();
    fs::write(upstream.join("src/lib.rs"), "fn gamma() {}\n").unwrap();
    git(&upstream, &["init", "-q"]);
    git(&upstream, &["config", "user.email", "t@t"]);
    git(&upstream, &["config", "user.name", "t"]);
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "x"]);

    let clone_root = d.join("cache/gamma");
    assert!(!clone_root.exists(), "clone target must not exist yet");

    fs::write(d.join("cfg.toml"), format!("\
        [[repos]]\n\
        slug = \"gamma\"\n\
        root = \"{root}\"\n\
        url = \"{url}\"\n",
        root = clone_root.display(), url = upstream.display())).unwrap();
    fs::write(d.join("p.dl"), "\
        rel s(p: file).\n\
        s(p) <- scan(\"gamma\", \"HEAD\", \"src/**/*.rs\", p, rev).\n\
        ? file(repo, rev, path, content).\n").unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--root", upstream.to_str().unwrap(), "--db", d.join("db").to_str().unwrap()])
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "run failed: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));

    assert!(clone_root.join(".git").is_dir(), "repo was cloned into root: {stdout}");
    let block = stdout.split("? file").nth(1).unwrap_or("");
    assert!(block.contains("gamma\t"), "cloned repo's file row present: {stdout}");
    assert!(block.contains("src/lib.rs"), "cloned file path present: {stdout}");
}

/// `scan("*", ...)` fans a single source rule out over every configured repo,
/// so one program queries the whole config repo set.
#[test]
fn scan_star_fans_out_over_every_config_repo() {
    let d = std::env::temp_dir().join("cfg_star_test");
    let _ = fs::remove_dir_all(&d);
    for (slug, body) in [("ra", "fn alpha() {}\n"), ("rb", "fn beta() {}\n")] {
        let r = d.join(slug);
        fs::create_dir_all(r.join("src")).unwrap();
        fs::write(r.join("src/lib.rs"), body).unwrap();
    }
    fs::write(d.join("cfg.toml"), format!("\
        [[repos]]\n\
        slug = \"ra\"\n\
        root = \"{a}\"\n\
        [[repos]]\n\
        slug = \"rb\"\n\
        root = \"{b}\"\n",
        a = d.join("ra").display(), b = d.join("rb").display())).unwrap();
    fs::write(d.join("p.dl"), "\
        rel s(p: file).\n\
        s(p) <- scan(\"*\", \"WORK\", \"src/**/*.rs\", p, rev).\n\
        ? file(repo, rev, path, content).\n").unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--root", d.join("ra").to_str().unwrap(), "--db", d.join("db").to_str().unwrap()])
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "run failed: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));

    let block = stdout.split("? file").nth(1).unwrap_or("");
    assert!(block.contains("ra\tWORK\tsrc/lib.rs"), "repo ra file row: {stdout}");
    assert!(block.contains("rb\tWORK\tsrc/lib.rs"), "repo rb file row: {stdout}");
    assert!(block.contains("(2 rows)"), "both repos' files, distinct by repo: {stdout}");
}

/// Two config repos that share a path with byte-identical content keep DISTINCT
/// located rows in `_where_bytes`. The bytes hash to the same FileId/StringId/
/// span/path, so before `of_located` folded `repo`, both repos collapsed to one
/// row (and a retract of either would prune the other's). One row per repo now.
#[test]
fn byte_identical_files_across_repos_keep_distinct_located_rows() {
    let d = std::env::temp_dir().join("cfg_wb_repo_test");
    let _ = fs::remove_dir_all(&d);
    // identical bytes in both repos at the same path
    for slug in ["ra", "rb"] {
        let r = d.join(slug);
        fs::create_dir_all(r.join("src")).unwrap();
        fs::write(r.join("src/lib.rs"), "struct Auth;\n").unwrap();
    }
    fs::write(d.join("cfg.toml"), format!("\
        [[repos]]\n\
        slug = \"ra\"\n\
        root = \"{a}\"\n\
        [[repos]]\n\
        slug = \"rb\"\n\
        root = \"{b}\"\n",
        a = d.join("ra").display(), b = d.join("rb").display())).unwrap();
    fs::write(d.join("p.dl"), "\
        rel sym(name: text, path: file).\n\
        sym(name, path) <- scan(\"*\", \"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /struct (?<name>\\w+)/, line).\n\
        ? sym(name, path).\n").unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--root", d.join("ra").to_str().unwrap(), "--db", d.join("db").to_str().unwrap()])
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "run failed: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));

    let conn = rusqlite::Connection::open(d.join("db")).unwrap();
    let repos: Vec<String> = conn
        .prepare("SELECT w.repo FROM _where_bytes w JOIN _strings s ON s.id = w.string_id \
                  WHERE s.content = 'Auth' ORDER BY w.repo")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(repos, vec!["ra".to_string(), "rb".to_string()],
        "one located row per repo, attributed by slug: got {repos:?}");
}
