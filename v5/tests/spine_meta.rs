use rusqlite::Connection;
use sprefa_v5::spine::{content_hash_hex, FileId, StringId, WhereBytes, WhereBytesId, ZERO_HASH_HEX};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const SRC: &str = "struct AuthService;\n";

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("spine_meta_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args([
            "--root",
            dir.to_str().unwrap(),
            "--db",
            dir.join("db").to_str().unwrap(),
        ])
        .output()
        .expect("run dl");
    assert!(
        out.status.success(),
        "dl failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn run_out(dir: &Path, prog: &str) -> (i32, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args([
            "--root",
            dir.to_str().unwrap(),
            "--db",
            dir.join("db").to_str().unwrap(),
        ])
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {:?} failed:\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn spine_meta_tables_are_created_with_sentinels() {
    let d = sandbox("sentinels");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    let prog = r#"
rel symbol(name: text, path: file).
symbol(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
? symbol(name, path).
"#;
    run(&d, prog);

    let conn = Connection::open(d.join("db")).unwrap();
    let string_row: (String, String, String) = conn
        .query_row(
            "SELECT id, content, norm FROM _strings WHERE id = '0'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        string_row,
        (StringId::EMPTY.to_string(), String::new(), String::new())
    );

    let file_row: (String, String, String, i64) = conn
        .query_row(
            "SELECT id, content_hash, path, size FROM _files WHERE id = '0'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        file_row,
        (
            FileId::SYNTHETIC.to_string(),
            ZERO_HASH_HEX.to_string(),
            String::new(),
            0
        )
    );

    let content_row: (String, String, String, i64) = conn
        .query_row(
            "SELECT id, content_hash, path, size FROM _files WHERE id = ?1",
            [FileId::of_bytes(SRC.as_bytes()).to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        content_row,
        (
            FileId::of_bytes(SRC.as_bytes()).to_string(),
            content_hash_hex(SRC.as_bytes()),
            "src/a.rs".to_string(),
            SRC.len() as i64
        )
    );

    let where_row: (String, String, String, i64, i64, String, String) = conn
        .query_row(
            "SELECT id, string_id, file_id, lo, hi, repo, rev FROM _where_bytes WHERE id = '0'",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        where_row,
        (
            WhereBytesId::SYNTHETIC.to_string(),
            StringId::EMPTY.to_string(),
            FileId::SYNTHETIC.to_string(),
            0,
            0,
            "0".to_string(),
            "0".to_string()
        )
    );

    let symbol_row: (String, String, String) = conn
        .query_row(
            "SELECT id, content, norm FROM _strings WHERE id = ?1",
            [StringId::of("AuthService").to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        symbol_row,
        (
            StringId::of("AuthService").to_string(),
            "AuthService".to_string(),
            "authservice".to_string()
        )
    );
}

#[test]
fn regex_captures_are_located_in_where_bytes() {
    let d = sandbox("where_bytes");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    let prog = r#"
rel symbol(name: text, path: file).
symbol(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
? symbol(name, path).
"#;
    run(&d, prog);

    let conn = Connection::open(d.join("db")).unwrap();
    let file_id = FileId::of_bytes(SRC.as_bytes());
    let string_id = StringId::of("AuthService");
    // "struct AuthService;\n" → "AuthService" spans bytes [7, 18).
    let lo = SRC.find("AuthService").unwrap() as i64;
    let hi = lo + "AuthService".len() as i64;
    let expect_id = WhereBytesId::of(WhereBytes {
        string: string_id,
        file: file_id,
        lo: lo as u32,
        hi: hi as u32,
        ..Default::default()
    });

    let row: (String, String, String, i64, i64, String, String) = conn
        .query_row(
            "SELECT id, string_id, file_id, lo, hi, repo, rev FROM _where_bytes WHERE id = ?1",
            [expect_id.to_string()],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        row,
        (
            expect_id.to_string(),
            string_id.to_string(),
            file_id.to_string(),
            lo,
            hi,
            "0".to_string(),
            "0".to_string(),
        )
    );
}

#[test]
fn sg_captures_are_located_in_where_bytes() {
    let d = sandbox("where_bytes_sg");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    // ast-grep metavar `$N` binds the `AuthService` identifier node; its byte
    // range must land in _where_bytes the same as the regex backend.
    let prog = r#"
rel sym(name: text, path: file).
sym(N, path) <- scan("WORK", "src/**/*.rs", path, rev), sg(path, rev, :rust, "struct $N;", line).
? sym(name, path).
"#;
    run(&d, prog);

    let conn = Connection::open(d.join("db")).unwrap();
    let lo = SRC.find("AuthService").unwrap() as i64;
    let hi = lo + "AuthService".len() as i64;
    let expect_id = WhereBytesId::of(WhereBytes {
        string: StringId::of("AuthService"),
        file: FileId::of_bytes(SRC.as_bytes()),
        lo: lo as u32,
        hi: hi as u32,
        ..Default::default()
    });
    let span: (i64, i64) = conn
        .query_row(
            "SELECT lo, hi FROM _where_bytes WHERE id = ?1",
            [expect_id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(span, (lo, hi));
}

#[test]
fn git_rev_captures_are_located_against_blob_file_id() {
    let d = sandbox("where_bytes_git");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    git(&d, &["init"]);
    git(&d, &["add", "src/a.rs"]);
    git(
        &d,
        &[
            "-c", "user.name=sprefa-test",
            "-c", "user.email=sprefa-test@example.invalid",
            "commit", "-m", "init",
        ],
    );
    let oid = git(&d, &["hash-object", "src/a.rs"]);
    let file_id = FileId::from_content_address(&oid, SRC.len() as i64).unwrap();

    let prog = r#"
rel sym(name: text, path: file).
sym(name, path) <- scan("HEAD", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
? sym(name, path).
"#;
    run(&d, prog);

    let conn = Connection::open(d.join("db")).unwrap();
    let lo = SRC.find("AuthService").unwrap() as i64;
    let hi = lo + "AuthService".len() as i64;
    // The located span is keyed on the git blob OID file id, so it joins _files.
    let expect_id = WhereBytesId::of(WhereBytes {
        string: StringId::of("AuthService"),
        file: file_id,
        lo: lo as u32,
        hi: hi as u32,
        ..Default::default()
    });
    let got: (String, i64, i64) = conn
        .query_row(
            "SELECT file_id, lo, hi FROM _where_bytes WHERE id = ?1",
            [expect_id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(got, (file_id.to_string(), lo, hi));
}

#[test]
fn string_and_ref_relations_are_queryable() {
    let d = sandbox("ref_query");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    // The program never scans on its own; `located` joins the built-in `string`
    // and `ref` relations, which a prior source rule must populate. Use one rule
    // for both: extract symbols (fills _strings/_where_bytes), then query refs.
    let prog = r#"
rel symbol(name: text, path: file).
rel located(text: text, lo: int, hi: int).
symbol(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
located(t, lo, hi) <- string(s, t, _), ref(s, _, lo, hi).
? located(t, lo, hi).
"#;
    let (code, out) = run_out(&d, prog);
    assert_eq!(code, 0, "dl failed: {out}");
    let lo = SRC.find("AuthService").unwrap();
    let hi = lo + "AuthService".len();
    assert!(
        out.contains(&format!("AuthService\t{lo}\t{hi}")),
        "expected located(AuthService,{lo},{hi}): {out}"
    );
    assert!(out.contains("(1 rows)"), "expected exactly one ref: {out}");
}

#[test]
fn editing_a_file_retracts_its_stale_located_spans() {
    let d = sandbox("where_bytes_retract");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    let prog = r#"
rel symbol(name: text, path: file).
symbol(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
located(t, lo, hi) <- string(s, t, _), ref(s, _, lo, hi).
rel located(text: text, lo: int, hi: int).
? located(t, lo, hi).
"#;
    let (code, out) = run_out(&d, prog);
    assert_eq!(code, 0, "cold run failed: {out}");
    assert!(out.contains("AuthService"), "cold: AuthService located: {out}");

    // Rename the struct; the old located span for AuthService must drop and the
    // new one for BillingService must appear. Tick only the changed file.
    fs::write(d.join("src/a.rs"), "struct BillingService;\n").unwrap();
    let _ = Command::new(DL)
        .arg(d.join("p.dl"))
        .args([
            "--root", d.to_str().unwrap(),
            "--db", d.join("db").to_str().unwrap(),
            "--changed", "src/a.rs",
        ])
        .output()
        .expect("run dl --changed");

    let conn = Connection::open(d.join("db")).unwrap();
    let stale: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM _where_bytes w JOIN _strings s ON s.id = w.string_id
             WHERE s.content = 'AuthService'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stale, 0, "stale AuthService span must be retracted");
    let fresh: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM _where_bytes w JOIN _strings s ON s.id = w.string_id
             WHERE s.content = 'BillingService'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fresh, 1, "renamed BillingService span must be present");
}

#[test]
fn committed_git_blobs_populate_spine_files_without_readback_hashing() {
    let d = sandbox("git_blob");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    git(&d, &["init"]);
    git(&d, &["add", "src/a.rs"]);
    git(
        &d,
        &[
            "-c",
            "user.name=sprefa-test",
            "-c",
            "user.email=sprefa-test@example.invalid",
            "commit",
            "-m",
            "init",
        ],
    );
    let oid = git(&d, &["hash-object", "src/a.rs"]);
    let file_id = FileId::from_content_address(&oid, SRC.len() as i64).unwrap();

    let prog = r#"
rel symbol(name: text, path: file).
symbol(name, path) <- scan("HEAD", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
? symbol(name, path).
"#;
    run(&d, prog);

    let conn = Connection::open(d.join("db")).unwrap();
    let content_row: (String, String, String, i64) = conn
        .query_row(
            "SELECT id, content_hash, path, size FROM _files WHERE id = ?1",
            [file_id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        content_row,
        (
            file_id.to_string(),
            oid,
            "src/a.rs".to_string(),
            SRC.len() as i64
        )
    );
}
