//! christmas #9 / decision 3: the trailing optional `id` arg on `sg` and `json`,
//! consistent with `ast`'s 7th-arg id. The id binds the located spine id of the
//! whole match (sg: captures' min..max byte range; json: the value's byte span),
//! and resolves through BOTH `ref(id, sid, file, lo, hi)` AND `string(sid, text,
//! norm)` to the matched source bytes (rides the step-1 intern fix).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sg_json_id_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run_json(dir: &PathBuf, prog: &str) -> Vec<serde_json::Value> {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap(), "--query-json"])
        .output().expect("run dl");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout)
        .lines().filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad json line `{l}`: {e}")))
        .collect()
}

fn q<'a>(recs: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    recs.iter().find(|r| r["query"] == name).unwrap_or_else(|| panic!("no query {name} in {recs:?}"))
}

#[test]
fn sg_whole_match_id_resolves_through_ref_and_string() {
    let d = sandbox("sg");
    fs::write(d.join("src/a.rs"), "fn alpha() {}\n").unwrap();

    // `id` is the trailing 5th sg output (line, col, end_line, end_col, id),
    // bound to the captures' min-lo..max-hi byte range (decision 3). The pattern
    // `fn $N() {}` captures only `$N` (= `alpha`, bytes 3..8), so the id's span
    // is that metavar range — and resolves to its text `alpha`.
    let prog = r#"
rel hit(id: text).
rel located(lo: int, hi: int).
rel texted(t: text).
hit(id) <- scan("WORK","src/**/*.rs",path,rev),
  sg(path, rev, :rust, "fn $N() {}", _l, _c, _el, _ec, id).
located(lo, hi) <- hit(id), ref(id, _s, _f, lo, hi).
texted(t) <- hit(id), ref(id, sid, _f, _lo, _hi), string(sid, t, _n).
? located(lo, hi).
? texted(t).
"#;
    let recs = run_json(&d, prog);
    let loc = q(&recs, "located");
    assert_eq!(loc["count"], 1, "sg id resolves through ref: {loc}");
    assert_eq!(loc["rows"][0][0].as_i64().unwrap(), 3, "lo (captures' min = $N start)");
    assert_eq!(loc["rows"][0][1].as_i64().unwrap(), 8, "hi (captures' max = $N end)");

    let txt = q(&recs, "texted");
    assert_eq!(txt["count"], 1, "sg id resolves through string: {txt}");
    assert_eq!(txt["rows"][0][0], "alpha",
        "sg id -> ref -> string recovers the captures' span source bytes: {txt}");
}

#[test]
fn json_value_id_resolves_through_ref_and_string() {
    let d = sandbox("json");
    fs::write(d.join("conf.yaml"), "spec:\n  image: \"registry.io/app:v3\"\n").unwrap();

    // `id` is the trailing 5th json arg: json(path, rev, jpath, out, id). It
    // binds the located id of the matched value's byte span.
    let prog = r#"
rel hit(v: text, id: text).
rel located(lo: int, hi: int).
rel texted(t: text).
hit(v, id) <- scan("WORK","*.yaml",path,rev),
  json(path, rev, "spec.image", v, id).
located(lo, hi) <- hit(_v, id), ref(id, _s, _f, lo, hi).
texted(t) <- hit(_v, id), ref(id, sid, _f, _lo, _hi), string(sid, t, _n).
? located(lo, hi).
? texted(t).
"#;
    let recs = run_json(&d, prog);
    let loc = q(&recs, "located");
    assert_eq!(loc["count"], 1, "json id resolves through ref: {loc}");
    let lo = loc["rows"][0][0].as_i64().unwrap();
    let hi = loc["rows"][0][1].as_i64().unwrap();
    assert!(hi > lo, "json value span is non-empty: {loc}");

    let txt = q(&recs, "texted");
    assert_eq!(txt["count"], 1, "json id resolves through string: {txt}");
    // datapath strips quotes from the bound value, but the located span covers
    // the raw source slice including quotes; both go through the same intern.
    let resolved = txt["rows"][0][0].as_str().unwrap();
    assert!(resolved.contains("registry.io/app:v3"),
        "json id -> ref -> string recovers the value's source bytes: {txt}");
}
