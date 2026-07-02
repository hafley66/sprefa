//! The std/flow.dl preset: shared flow_edge base, flow_summary propagation
//! models, and the field-sensitive arg_field_flow view.
//!
//! Three gates:
//!   1. SUMMARY: an unmodeled external callee propagates every arg to its
//!      result (the lift's blanket edge). Asserting `flow_summary(name, pos)`
//!      cuts every OTHER slot — the negative half is what the model buys.
//!   2. SANITIZER: `flow_sanitizer(name)` cuts ALL arg->result flow — the
//!      zero-slot instance of the same model.
//!   3. FIELD: a value stored into field F of a composite passed as an
//!      argument reaches the callee's read of the SAME field name, and not a
//!      read of a different field (arg_field_flow).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flow_std_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Split dl stdout into one section per `?` query (header + its data rows).
fn sections(out: &str) -> Vec<String> {
    let mut secs: Vec<String> = Vec::new();
    for line in out.lines() {
        if line.starts_with("? ") {
            secs.push(String::new());
        }
        if let Some(cur) = secs.last_mut() {
            if !cur.is_empty() {
                cur.push('\n');
            }
            cur.push_str(line);
        }
    }
    secs
}

fn rows(sec: &str) -> Vec<Vec<String>> {
    sec.lines()
        .filter(|l| l.starts_with("  ") && l.contains('\t') && !l.contains("(0 rows)"))
        .map(|l| l.trim_start().split('\t').map(|s| s.trim_end().to_string()).collect())
        .collect()
}

fn node_id(nodes: &[Vec<String>], kind: &str, var: &str, out: &str) -> String {
    let hits: Vec<&Vec<String>> = nodes
        .iter()
        .filter(|r| r.len() >= 4 && r[1] == kind && r[2] == var)
        .collect();
    assert_eq!(hits.len(), 1, "expected one {kind}/{var} node, got {}:\n{out}", hits.len());
    hits[0][0].clone()
}

/// The fixture: `libwrap` has no body in the corpus, so its call's flow is
/// whatever the model says. `secret` and `benign` both feed it; `s` receives
/// the result.
const SUMMARY_SRC: &str = "fn caller(secret: i32, benign: i32) {\n    let r = libwrap(secret, benign);\n    let s = r;\n}\n";

const SUMMARY_PROG_BASE: &str = r#"
use "std/flow.dl".
rel src(p: file).
src(p) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /./, l).
rel flow_reach(from: text, to: text).
flow_reach(a, b) <- closure(flow_edge).
? flow_reach(from, to).
? df_node(id, kind, var, fn, file, line).
"#;

fn reach_and_nodes(out: &str) -> (HashSet<(String, String)>, Vec<Vec<String>>) {
    let secs = sections(out);
    assert!(secs.len() >= 2, "expected 2 query sections:\n{out}");
    let mut reach = HashSet::new();
    for r in rows(&secs[0]) {
        reach.insert((r[0].clone(), r[1].clone()));
    }
    (reach, rows(&secs[1]))
}

#[test]
fn unmodeled_external_call_propagates_every_arg() {
    let d = sandbox("unmodeled");
    fs::write(d.join("src/lib.rs"), SUMMARY_SRC).unwrap();
    let (code, out, err) = run(&d, SUMMARY_PROG_BASE);
    assert_eq!(code, 0, "must not error:\n{err}");
    let (reach, nodes) = reach_and_nodes(&out);
    let secret = node_id(&nodes, "param", "secret", &out);
    let benign = node_id(&nodes, "param", "benign", &out);
    let s = node_id(&nodes, "let_bind", "s", &out);
    // The blanket over-approximation: both args reach the result's consumer.
    assert!(reach.contains(&(secret.clone(), s.clone())), "secret must reach s unmodeled:\n{out}");
    assert!(reach.contains(&(benign.clone(), s.clone())), "benign must reach s unmodeled:\n{out}");
}

#[test]
fn flow_summary_cuts_unsummarized_slots() {
    let d = sandbox("summary");
    fs::write(d.join("src/lib.rs"), SUMMARY_SRC).unwrap();
    // Model: only slot 1 (benign) flows through libwrap.
    let prog = format!("flow_summary(\"libwrap\", 1).\n{SUMMARY_PROG_BASE}");
    let (code, out, err) = run(&d, &prog);
    assert_eq!(code, 0, "must not error:\n{err}");
    let (reach, nodes) = reach_and_nodes(&out);
    let secret = node_id(&nodes, "param", "secret", &out);
    let benign = node_id(&nodes, "param", "benign", &out);
    let s = node_id(&nodes, "let_bind", "s", &out);
    // The kept slot still flows ...
    assert!(reach.contains(&(benign.clone(), s.clone())), "benign (slot 1) must still reach s:\n{out}");
    // ... and the DECISIVE negative: the unsummarized slot is cut.
    assert!(!reach.contains(&(secret.clone(), s.clone())), "secret (slot 0) must NOT reach s under the model:\n{out}");
}

#[test]
fn flow_sanitizer_cuts_all_arg_flow() {
    let d = sandbox("sanitizer");
    fs::write(d.join("src/lib.rs"), SUMMARY_SRC).unwrap();
    let prog = format!("flow_sanitizer(\"libwrap\").\n{SUMMARY_PROG_BASE}");
    let (code, out, err) = run(&d, &prog);
    assert_eq!(code, 0, "must not error:\n{err}");
    let (reach, nodes) = reach_and_nodes(&out);
    let secret = node_id(&nodes, "param", "secret", &out);
    let benign = node_id(&nodes, "param", "benign", &out);
    let s = node_id(&nodes, "let_bind", "s", &out);
    assert!(!reach.contains(&(secret.clone(), s.clone())), "secret must NOT reach s past a sanitizer:\n{out}");
    assert!(!reach.contains(&(benign.clone(), s.clone())), "benign must NOT reach s past a sanitizer:\n{out}");
}

/// PER-CALL-SITE GATE (run per language). One caller invokes TWO callees:
/// f(secret) and g(benign), with x = f(..) and y = g(..). The old per-caller
/// hop joined every call node in the caller to every callee the caller
/// resolves to, so secret leaked into g's param and f's return leaked into y.
/// The call_node + call_name pin cuts both. Positives prove the pinned hops
/// still fire; negatives are the context refinement. Kotlin in the matrix
/// also proves the df line-base normalization (the call_node equality join is
/// what the pin rides).
#[test]
fn call_sites_do_not_cross_talk_per_language() {
    let rust = "fn f(p: i32) -> i32 { let q = p; q }\n\
                fn g(r: i32) -> i32 { let s = r; s }\n\
                fn caller(secret: i32, benign: i32) {\n    let x = f(secret);\n    let y = g(benign);\n    let u = x;\n    let w = y;\n}\n";
    let ts = "function f(p: number): number { const q = p; return q; }\n\
              function g(r: number): number { const s = r; return s; }\n\
              function caller(secret: number, benign: number): void {\n    const x = f(secret);\n    const y = g(benign);\n    const u = x;\n    const w = y;\n}\n";
    let kotlin = "fun f(p: Int): Int { val q = p; return q }\n\
                  fun g(r: Int): Int { val s = r; return s }\n\
                  fun caller(secret: Int, benign: Int) {\n    val x = f(secret)\n    val y = g(benign)\n    val u = x\n    val w = y\n}\n";

    for (lang, ext, srcfile) in [("rust", "rs", rust), ("ts", "ts", ts), ("kotlin", "kt", kotlin)] {
        let d = sandbox(&format!("persite_{lang}"));
        fs::write(d.join(format!("src/lib.{ext}")), srcfile).unwrap();
        let prog = SUMMARY_PROG_BASE.replace("src/**/*.rs", &format!("src/**/*.{ext}"));
        let (code, out, err) = run(&d, &prog);
        assert_eq!(code, 0, "[{lang}] must not error:\n{err}");
        let (reach, nodes) = reach_and_nodes(&out);
        let secret = node_id(&nodes, "param", "secret", &out);
        let benign = node_id(&nodes, "param", "benign", &out);
        let p = node_id(&nodes, "param", "p", &out);
        let r = node_id(&nodes, "param", "r", &out);
        let u = node_id(&nodes, "let_bind", "u", &out);
        let w = node_id(&nodes, "let_bind", "w", &out);

        // Positives: each call site still crosses into ITS callee and back.
        assert!(reach.contains(&(secret.clone(), p.clone())), "[{lang}] secret must reach f's p:\n{out}");
        assert!(reach.contains(&(benign.clone(), r.clone())), "[{lang}] benign must reach g's r:\n{out}");
        assert!(reach.contains(&(secret.clone(), u.clone())), "[{lang}] secret must round-trip f into u:\n{out}");
        assert!(reach.contains(&(benign.clone(), w.clone())), "[{lang}] benign must round-trip g into w:\n{out}");
        // DECISIVE negatives: no cross-callee forward leak ...
        assert!(!reach.contains(&(secret.clone(), r.clone())), "[{lang}] secret must NOT leak into g's r:\n{out}");
        assert!(!reach.contains(&(benign.clone(), p.clone())), "[{lang}] benign must NOT leak into f's p:\n{out}");
        // ... and no cross-callee return leak.
        assert!(!reach.contains(&(secret.clone(), w.clone())), "[{lang}] f's return must NOT land in y/w:\n{out}");
        assert!(!reach.contains(&(benign.clone(), u.clone())), "[{lang}] g's return must NOT land in x/u:\n{out}");
    }
}

/// COLLECTION GATE (run per language). `xs.map(<lambda>)` — the receiver's
/// element enters the lambda's param and the lambda's result becomes the
/// call's result, but ONLY when the flow-collections preset asserts the
/// flow_lambda facts. The negative half (no preset -> no element hop) is what
/// distinguishes the fact-driven hop from a blanket edge.
#[test]
fn collection_lambda_hops_are_fact_driven_per_language() {
    let rust = "fn go(xs: Vec<i32>) {\n    let out = xs.map(|x| x + 1);\n    let sink = out;\n}\n";
    let ts = "function go(xs: number[]): void {\n    const out = xs.map((x) => x + 1);\n    const sink = out;\n}\n";
    let kotlin = "fun go(xs: List<Int>) {\n    val out = xs.map { it + 1 }\n    val sink = out\n}\n";

    for (lang, ext, srcfile, lam_var) in [
        ("rust", "rs", rust, "x"),
        ("ts", "ts", ts, "x"),
        ("kotlin", "kt", kotlin, "it"),
    ] {
        // With the preset: element hop + result hop both fire.
        let d = sandbox(&format!("coll_{lang}"));
        fs::write(d.join(format!("src/lib.{ext}")), srcfile).unwrap();
        let prog = format!(
            "use \"std/flow-collections.dl\".\n\
             rel src(p: file).\n\
             src(p) <- scan(\"WORK\", \"src/**/*.{ext}\", p, rev), match(p, rev, /./, l).\n\
             rel flow_reach(from: text, to: text).\n\
             flow_reach(a, b) <- closure(flow_edge).\n\
             ? flow_reach(from, to).\n\
             ? df_node(id, kind, var, fn, file, line).\n"
        );
        let (code, out, err) = run(&d, &prog);
        assert_eq!(code, 0, "[{lang}] must not error:\n{err}");
        let (reach, nodes) = reach_and_nodes(&out);
        let xs = node_id(&nodes, "param", "xs", &out);
        let lam_param = nodes.iter()
            .find(|r| r.len() >= 4 && r[1] == "param" && r[2] == lam_var && r[3].contains("::closure::"))
            .unwrap_or_else(|| panic!("[{lang}] lifted lambda param:\n{out}"))[0].clone();
        let sink = node_id(&nodes, "let_bind", "sink", &out);
        assert!(
            reach.contains(&(xs.clone(), lam_param.clone())),
            "[{lang}] element hop: xs must reach the lambda param:\n{out}"
        );
        assert!(
            reach.contains(&(lam_param.clone(), sink.clone())),
            "[{lang}] result hop: the lambda param must reach sink via its return:\n{out}"
        );

        // DECISIVE negative: same corpus, bare std/flow.dl (no facts) — the
        // element never enters the lambda.
        let d2 = sandbox(&format!("coll_neg_{lang}"));
        fs::write(d2.join(format!("src/lib.{ext}")), srcfile).unwrap();
        let prog2 = prog.replace("std/flow-collections.dl", "std/flow.dl");
        let (code2, out2, err2) = run(&d2, &prog2);
        assert_eq!(code2, 0, "[{lang}] must not error:\n{err2}");
        let (reach2, nodes2) = reach_and_nodes(&out2);
        let xs2 = node_id(&nodes2, "param", "xs", &out2);
        let lam_param2 = nodes2.iter()
            .find(|r| r.len() >= 4 && r[1] == "param" && r[2] == lam_var && r[3].contains("::closure::"))
            .unwrap_or_else(|| panic!("[{lang}] lifted lambda param:\n{out2}"))[0].clone();
        assert!(
            !reach2.contains(&(xs2, lam_param2)),
            "[{lang}] without the preset xs must NOT reach the lambda param:\n{out2}"
        );
    }
}

/// FIELD GATE. `render({title: v, extra: w})` — v is stored under "title", the
/// callee reads BOTH a member (`opts.title`) and a destructured param shape in
/// a second callee. v must hit the same-named read and not the other field's.
#[test]
fn arg_field_flow_matches_same_named_reads_only() {
    let d = sandbox("field");
    fs::write(
        d.join("src/lib.ts"),
        "function render(opts: { title: string; extra: string }): void {\n\
             const t = opts.title;\n\
             const e = opts.extra;\n\
         }\n\
         function caller(v: string, w: string): void {\n\
             const r = render({ title: v, extra: w });\n\
         }\n",
    )
    .unwrap();
    let prog = r#"
use "std/flow.dl".
rel src(p: file).
src(p) <- scan("WORK", "src/**/*.ts", p, rev), match(p, rev, /./, l).
? arg_field_flow(value, field, call, target).
? df_node(id, kind, var, fn, file, line).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "must not error:\n{err}");
    let secs = sections(&out);
    assert!(secs.len() >= 2, "expected 2 query sections:\n{out}");
    let flows: Vec<Vec<String>> = rows(&secs[0]);
    let nodes = rows(&secs[1]);

    let v = node_id(&nodes, "var_read", "v", &out);
    let w = node_id(&nodes, "var_read", "w", &out);
    // Member reads carry the accessed name in var.
    let title_read = node_id(&nodes, "member", "title", &out);
    let extra_read = node_id(&nodes, "member", "extra", &out);

    let has = |val: &str, fld: &str, tgt: &str| {
        flows.iter().any(|r| r.len() >= 4 && r[0] == val && r[1] == fld && r[3] == tgt)
    };
    // Positive: each stored value reaches the read of ITS field.
    assert!(has(&v, "title", &title_read), "v must reach the title read:\n{out}");
    assert!(has(&w, "extra", &extra_read), "w must reach the extra read:\n{out}");
    // DECISIVE negatives: no cross-field conflation in the view.
    assert!(!has(&v, "extra", &extra_read), "v must NOT reach the extra read:\n{out}");
    assert!(!has(&v, "title", &extra_read), "v/title must not target the extra read:\n{out}");
    assert!(!has(&w, "title", &title_read), "w must NOT reach the title read:\n{out}");
}
