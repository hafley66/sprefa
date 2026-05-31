use rusqlite::Connection;
use sprefa_v5::spine::{FileId, StringId, WhereBytesId, ZERO_HASH_HEX};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

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

#[test]
fn spine_meta_tables_are_created_with_sentinels() {
    let d = sandbox("sentinels");
    fs::write(d.join("src/a.rs"), "fn alpha() {}\n").unwrap();
    let prog = r#"
rel hit(path: file).
hit(path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /fn/, line).
? hit(path).
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
}
