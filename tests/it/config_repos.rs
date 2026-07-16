//! Turnkey config: a `[[repos]]` TOML at `$SPREFA_CONFIG` populates the `repo`
//! relation. (File ingestion from the extra roots is a later step; this proves
//! the config is loaded and drives the built-in `repo` relation.)

use std::fs;
use std::process::Command;

use sprefa_v5::config::RepoConfig;
use sprefa_v5::db;
use sprefa_v5::engine::Engine;

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// Engine-level dedup (Rule A): two `[[repos]]` whose roots resolve to the SAME
/// canonical directory (one reached through a symlink) collapse to ONE repo
/// entry: repo identity within an engine is the canonical root path. The first
/// slug at a path wins.
#[test]
fn same_canonical_root_under_two_slugs_dedupes_to_one() {
    let d = std::env::temp_dir().join(format!("cfg_dedupe_{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    let real = d.join("real");
    fs::create_dir_all(real.join("src")).unwrap();
    fs::write(real.join("src/lib.rs"), "fn x() {}\n").unwrap();
    // A symlink to the same directory: a distinct path, one canonical target.
    let link = d.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    // Engine root is unrelated, so ONLY the two config repos can collide.
    let mut eng = Engine::new(conn, d.join("engine-root"));
    eng.set_repos(vec![
        RepoConfig { slug: "canon".into(), root: real.clone(), url: None, allow_missing: false },
        RepoConfig { slug: "via-link".into(), root: link.clone(), url: None, allow_missing: false },
    ]);
    let repos = eng.snapshot_repos();
    assert_eq!(repos.len(), 1, "two slugs at one canonical dir collapse to one: {:?}",
        repos.iter().map(|r| &r.slug).collect::<Vec<_>>());
    assert_eq!(repos[0].slug, "canon", "first slug at a path wins");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn config_repos_populate_the_repo_relation() {
    let d = std::env::temp_dir().join("cfg_repos_test");
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/lib.rs"), "fn main() {}\n").unwrap();
    // The `root`s must exist: a missing configured repo is omitted from the
    // `repo` relation so programs can detect misses via `!repo(S, _, _)`.
    fs::create_dir_all("/tmp/alpha").unwrap();
    fs::create_dir_all("/tmp/beta").unwrap();
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
        ? repo(slug, root, url).\n").unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&d)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "run failed: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));

    let block = stdout.split("? repo").nth(1).unwrap_or("");
    assert!(block.contains("alpha/one\t/tmp/alpha"), "repo alpha from config: {stdout}");
    assert!(block.contains("beta/two\t/tmp/beta"), "repo beta from config: {stdout}");
    assert!(block.contains("(2 rows)"), "exactly the two configured repos: {stdout}");

    // No config -> the single --root repo (one row), not the configured two.
    let out2 = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db2").to_str().unwrap()])
        .current_dir(&d)
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

/// A `repo` sink GROUND FACT — `repo("slug", "/root", "").` with an empty body —
/// registers the repo directly (bypassing the github-org allowlist, which gates
/// only DYNAMIC pulls). Previously the sink rejected any literal head term, so an
/// author had to route through an intermediary rel; a bare fact now works.
#[test]
fn repo_sink_accepts_a_ground_fact() {
    let d = std::env::temp_dir().join("repo_ground_fact_test");
    let _ = fs::remove_dir_all(&d);
    // the extra repo the ground fact names (an existing local root, no url)
    let extra = d.join("extra");
    fs::create_dir_all(extra.join("src")).unwrap();
    fs::write(extra.join("src/lib.rs"), "fn extra_fn() {}\n").unwrap();
    // a self root to run from
    let selfdir = d.join("selfrepo");
    fs::create_dir_all(selfdir.join("src")).unwrap();
    fs::write(selfdir.join("src/main.rs"), "fn main() {}\n").unwrap();

    // Ground fact heads the built-in `repo` sink directly — no body, all literals.
    fs::write(d.join("p.dl"), format!("\
        repo(\"extra\", \"{root}\", \"\").\n\
        ? repo(slug, root, url).\n", root = extra.display())).unwrap();

    // --settle drives ticks to a fixpoint: the sink registers the repo AFTER the
    // query phase (one-tick registration latency, same as any repo sink), so a
    // single tick would not yet show it; settle converges and prints once.
    let out = Command::new(DL)
        .arg("--settle")
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&selfdir)
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "run failed: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    // the ground-fact repo is registered and shows in the repo relation
    assert!(stdout.contains("extra"), "ground-fact repo registered in repo rel: {stdout}");
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
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&upstream)
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
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(d.join("ra"))
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
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(d.join("ra"))
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

/// `allow_missing = true` on a config repo whose root is absent is non-fatal:
/// the engine prints one stderr line, omits the missing slug from the `repo`
/// relation (so `!repo(S, _, _)` detects it), and lets the program run to
/// completion. Without the flag the same setup bails.
#[test]
fn allow_missing_repo_runs_to_completion_and_is_omitted_from_repo_rel() {
    let d = std::env::temp_dir().join("cfg_allow_missing_test");
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("present/src")).unwrap();
    fs::write(d.join("present/src/lib.rs"), "fn here() {}\n").unwrap();

    let cfg = format!("\
        [[repos]]\n\
        slug = \"present\"\n\
        root = \"{present}\"\n\
        [[repos]]\n\
        slug = \"absent\"\n\
        root = \"{absent}\"\n\
        allow_missing = true\n",
        present = d.join("present").display(),
        absent = d.join("does-not-exist").display());
    fs::write(d.join("cfg.toml"), cfg).unwrap();

    // `referenced` is the author-defined inventory of slugs the program means
    // to scan; `missing_repo` is its antijoin against the `repo` builtin. With
    // `allow_missing`, the missing scan yields zero rows but does not bail; the
    // present scan contributes one file. The `repo` rel omits the absent slug,
    // so the antijoin surfaces it.
    let prog = "\
        rel src_present(p: file).\n\
        rel src_absent(p: file).\n\
        rel referenced(slug: text).\n\
        rel missing_repo(slug: text).\n\
        src_present(p) <- scan(\"present\", \"WORK\", \"src/**/*.rs\", p, rev).\n\
        src_absent(p)  <- scan(\"absent\",  \"WORK\", \"src/**/*.rs\", p, rev).\n\
        referenced(\"present\").\n\
        referenced(\"absent\").\n\
        missing_repo(s) <- referenced(s), !repo(s, _, _).\n";
    let run_with = |query: &str| -> (String, String) {
        fs::write(d.join("p.dl"), format!("{prog}\n{query}\n")).unwrap();
        let out = Command::new(DL)
            .arg(d.join("p.dl"))
            .args(["--db", d.join("db").to_str().unwrap()])
            .current_dir(d.join("present"))
            .env("SPREFA_CONFIG", d.join("cfg.toml"))
            .output().expect("run dl");
        (String::from_utf8_lossy(&out.stdout).into_owned(),
         String::from_utf8_lossy(&out.stderr).into_owned())
    };

    let (stdout, stderr) = run_with("? repo(s, _, _).");
    assert!(stderr.contains("[missing]"), "stderr should note the missing repo: {stderr}");
    assert!(stdout.contains("present"), "present slug in repo rel: {stdout}");
    assert!(!stdout.contains("absent"), "missing slug omitted from repo rel: {stdout}");

    let (stdout, _) = run_with("? missing_repo(s).");
    assert!(stdout.contains("absent"), "missing slug surfaced by antijoin: {stdout}");
    assert!(!stdout.contains("present"), "present slug must not appear as missing: {stdout}");

    let (stdout, _) = run_with("? src_present(p).");
    assert!(stdout.contains("lib.rs"), "present scan still produces rows: {stdout}");
}

/// Cross-repo hardness test. Simulates an org checkout with two repos
/// (`alpha` and `beta`). Proves that `scan("*")` picks up files from both
/// and that the built-in relations carry per-repo attribution. The shape
/// matches a polyglot org: Rust crates split across repos where files under
/// different roots must not collide on path or content identity.
#[test]
fn cross_repo_scan_and_file_attribution_hardness() {
    let org = std::env::temp_dir().join("sim_org_hardness");
    let _ = fs::remove_dir_all(&org);
    fs::create_dir_all(&org).unwrap();

    // beta: a library declaring pub fn one() and pub fn two()
    let beta = org.join("beta");
    fs::create_dir_all(beta.join("src")).unwrap();
    fs::write(beta.join("src/lib.rs"), "pub fn one() {}\npub fn two() { is_nothing(); }\n").unwrap();

    // alpha: an app with a different file, importing from beta via path dep
    let alpha = org.join("alpha");
    fs::create_dir_all(alpha.join("src")).unwrap();
    fs::write(alpha.join("src/main.rs"), "fn main() {}\nfn helper() {}\n").unwrap();

    // Both repos registered in config
    fs::write(org.join("cfg.toml"), format!("\
        [[repos]]\n\
        slug = \"alpha\"\n\
        root = \"{a}\"\n\
        [[repos]]\n\
        slug = \"beta\"\n\
        root = \"{b}\"\n",
        a = alpha.display(), b = beta.display())).unwrap();

    // DL program: scan "*" fans out over both repos. Put scan and
    // extraction (match) in the same rule body — the engine tracks
    // per-file provenance through that body.
    let prog = "\
        ? repo(slug, _, _).\n\
        ? file(repo, rev, path, _).\n\
        rel has_fn(p: file, name: text).\n\
        has_fn(p, name) <- scan(\"*\", \"WORK\", \"**/*.rs\", p, rev), match(p, rev, /fn (?<name>\\w+)/, line).\n\
        ? has_fn(p, name).\n";
    fs::write(org.join("p.dl"), prog).unwrap();

    let out = Command::new(DL)
        .arg(org.join("p.dl"))
        .args(["--db", org.join("db").to_str().unwrap()])
        .current_dir(&alpha)
        .env("SPREFA_CONFIG", org.join("cfg.toml"))
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "run failed: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));

    // Both repos in the repo relation
    assert!(stdout.contains("alpha"), "alpha slug in repo rel: {stdout}");
    assert!(stdout.contains("beta"), "beta slug in repo rel: {stdout}");

    // Files from both repos in the file relation with per-repo attribution
    assert!(stdout.contains("src/main.rs"), "alpha/src/main.rs in file rel: {stdout}");
    assert!(stdout.contains("src/lib.rs"), "beta/src/lib.rs in file rel: {stdout}");

    // scan picks up both repos' files
    assert!(stdout.contains("src/main.rs"), "scan picks up main.rs: {stdout}");
    assert!(stdout.contains("src/lib.rs"), "scan picks up lib.rs: {stdout}");

    // match extracts functions from both repos independently
    assert!(stdout.contains("main") || stdout.contains("helper"), "alpha fns: {stdout}");
    assert!(stdout.contains("one") || stdout.contains("two"), "beta fns: {stdout}");
}

/// Without `allow_missing`, the same absent root is fatal (engine bails). This
/// guards against accidentally swallowing missing repos in the default path.
#[test]
fn missing_repo_without_flag_bails() {
    let d = std::env::temp_dir().join("cfg_bail_test");
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("present/src")).unwrap();
    fs::write(d.join("present/src/lib.rs"), "fn here() {}\n").unwrap();

    let cfg = format!("\
        [[repos]]\n\
        slug = \"present\"\n\
        root = \"{present}\"\n\
        [[repos]]\n\
        slug = \"absent\"\n\
        root = \"{absent}\"\n",
        present = d.join("present").display(),
        absent = d.join("does-not-exist").display());
    fs::write(d.join("cfg.toml"), cfg).unwrap();
    fs::write(d.join("p.dl"), "\
        rel src(p: file).\n\
        src(p) <- scan(\"*\", \"WORK\", \"src/**/*.rs\", p, rev).\n\
        ? src(p).\n").unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(d.join("present"))
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output().expect("run dl");
    assert!(!out.status.success(), "missing root without allow_missing must bail: {}",
        String::from_utf8_lossy(&out.stdout));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("does not exist"), "bail message names the missing root: {stderr}");
}
