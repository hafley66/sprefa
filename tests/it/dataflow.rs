//! The lift-to-node dataflow family (`df_node` / `df_edge`), wired as a lazy
//! indexer like CALL_RELS. The Rust extractor (syn) lifts each value-bearing
//! position to a node and emits local value-flow edges; the engine stores them
//! and `df_reaches(a,b) <- closure(df_edge)` walks the lifted graph transitively
//! on the SAME SCC engine the call/type/module graphs already use.
//!
//! The gate test proves the model end to end on a macro-free chain:
//!   fn f(name) { let s = greet(name); let u = echo(&s); use_up(&u); }
//! `name` (param) must REACH the read of `u` several hops later — but the pair
//! is NOT a direct `df_edge`. That asymmetry (absent from base edges, present in
//! the closure) is the proof: the lift produced a real graph and the shared
//! closure engine is doing the transitive walk, not just echoing base rows.
//!
//! Sprefa `?` queries are single-relation (no inline joins), and a closure head
//! may only be read in a rule with a literal-pinned endpoint. So the proof runs
//! three direct queries and joins their result tables in Rust.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sprefa_v5::{db, engine::Engine, lex, parse};

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dataflow_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args([
            "--db",
            dir.join("db").to_str().unwrap(),
        ])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Split dl stdout into one String per `?` query, in order. Each section is the
/// header line plus its indented data rows.
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

/// Parse a section's indented tab-separated rows into Vec<Vec<String>>.
fn rows(sec: &str) -> Vec<Vec<String>> {
    sec.lines()
        .filter(|l| !l.starts_with("? ") && l.contains('\t') && !l.contains("(0 rows)"))
        .map(|l| {
            l.trim_start()
                .split('\t')
                .map(|s| s.trim_end().to_string())
                .collect()
        })
        .collect()
}

/// The two DATAFLOW_RELS are reserved, matching CALL_RELS / TYPE_RELS.
#[test]
fn dataflow_rels_are_reserved() {
    let d = sandbox("reserved");
    for rel in ["df_node", "df_node_repo", "df_edge", "loop_over", "allocates", "nest", "df_param", "df_arg", "df_field"] {
        let prog = format!("rel {rel}(a: text).\n? {rel}(\"x\").\n");
        let (code, _out, err) = run(&d, &prog);
        assert_ne!(code, 0, "{rel} must be reserved (expected error):\n{err}");
        assert!(
            err.contains("reserved-name") && err.contains(&format!("relation `{rel}`"))
                && err.contains("pick another name"),
            "{rel} parse-tier reservation/fix missing:\n{err}"
        );
    }
}

/// A language whose extract_dataflow is still the empty default keeps the wiring
/// live: the lazy indexer runs, relations are queryable and empty, and
/// closure(df_edge) is a legal (empty) edge rel. (Rust/Kotlin/TS all override
/// now; this is the reservation + lazy-trigger smoke test for any not-yet-
/// implemented front-end that returns the empty default.)
#[test]
fn dataflow_lazy_gate_smoke() {
    let d = sandbox("empty");
    fs::create_dir_all(d.join("src")).unwrap();
    // No .rs/.kt/.ts file scanned -> df_node/df_edge stay empty, closure is legal.
    fs::write(d.join("src/notes.txt"), "not source\n").unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.txt\", path, rev), match(path, rev, /./, line).\n",
        "rel df_reaches(from: text, to: text).\n",
        "df_reaches(a, b) <- closure(df_edge).\n",
        "? df_node(id, kind, var, fn, file, line).\n",
        "? df_reaches(a, b).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "no-source scan must not error:\n{err}");
    assert!(out.contains("(0 rows)"), "expected zero-row footers:\n{out}");
}

/// THE GATE. The lift produces a node for every value-bearing position; local
/// flow edges link them; and `closure(df_edge)` walks the chain so the `name`
/// param REACHES the read of `u` several hops later. That pair is absent from
/// `df_edge` (not a base edge) yet present in `df_reaches` (transitive) — the
/// asymmetry that proves the closure engine is doing real work on the lifted graph.
#[test]
fn rust_lift_closes_transitively() {
    let d = sandbox("rust");
    fs::create_dir_all(d.join("src")).unwrap();
    // Macro-free so syn sees the real Expr children (macros are token streams,
    // which the lift mints a node for but cannot chase into).
    //   chain in f: name(param) -> greet(name) -> s -> echo(&s) -> u -> use_up(&u)
    fs::write(
        d.join("src/lib.rs"),
        "fn greet(g: &str) -> String { String::new() }\n\
         fn echo(x: &str) -> String { String::new() }\n\
         fn use_up(v: &str) {}\n\
         fn f(name: &str) {\n    \
             let s = greet(name);\n    \
             let u = echo(&s);\n    \
             use_up(&u);\n\
         }\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        "rel df_reaches(from: text, to: text).\n",
        "df_reaches(a, b) <- closure(df_edge).\n",
        "? df_node(id, kind, var, fn, file, line).\n",
        "? df_edge(from, to).\n",
        "? df_reaches(from, to).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "Rust lift must not error:\n{err}");

    let secs = sections(&out);
    assert!(secs.len() >= 3, "expected 3 query sections:\n{out}");

    // Section 0: df_node -> map id -> (kind, var).
    let mut nodes: HashMap<String, (String, String)> = HashMap::new();
    for r in rows(&secs[0]) {
        assert!(r.len() >= 3, "df_node row too short: {r:?}");
        nodes.insert(r[0].clone(), (r[1].clone(), r[2].clone()));
    }
    // Section 1: df_edge -> (from, to) set.
    let mut edges: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[1]) {
        assert!(r.len() >= 2, "df_edge row too short: {r:?}");
        edges.insert((r[0].clone(), r[1].clone()));
    }
    // Section 2: df_reaches -> (from, to) set.
    let mut reaches: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[2]) {
        assert!(r.len() >= 2, "df_reaches row too short: {r:?}");
        reaches.insert((r[0].clone(), r[1].clone()));
    }

    // The lift produced the source node: exactly one param named `name`.
    let name_params: Vec<&String> = nodes
        .iter()
        .filter(|(_, (k, v))| k == "param" && v == "name")
        .map(|(id, _)| id)
        .collect();
    assert_eq!(name_params.len(), 1, "expected one param `name`:\n{out}");
    let name_id = name_params[0].clone();

    // And the sink node: exactly one var_read of `u`.
    let u_reads: Vec<&String> = nodes
        .iter()
        .filter(|(_, (k, v))| k == "var_read" && v == "u")
        .map(|(id, _)| id)
        .collect();
    assert_eq!(u_reads.len(), 1, "expected one var_read `u`:\n{out}");
    let u_id = u_reads[0].clone();

    // There is a real graph to walk, not an empty one.
    assert!(!edges.is_empty(), "df_edge is empty — lift produced no edges:\n{out}");

    // DECISIVE: the (name, u_read) pair is NOT a base edge ...
    assert!(
        !edges.contains(&(name_id.clone(), u_id.clone())),
        "name->u must NOT be a direct df_edge (closure earns its keep):\n{out}"
    );
    // ... yet closure reaches it: name flows to u across the chain.
    assert!(
        reaches.contains(&(name_id.clone(), u_id.clone())),
        "name must REACH u transitively via closure(df_edge):\n{out}"
    );
}

/// TS/JS class methods were a documented gap: `ts_flow_stmt` had no
/// `ClassDeclaration` arm, so a class method's body never reached the
/// dataflow walk even though the type/call extraction saw it fine (see
/// docs/df-coverage.md, "known gaps"). The method's param and its let-bound
/// call result must both mint `df_node` rows scoped under the `Widget.render`
/// fn sym — the same sym `ts_class_call_defs`/`ts_class_entity` already mint
/// for the method — and `name` must REACH `label` transitively via
/// `closure(df_edge)`, the same asymmetry proof `rust_lift_closes_transitively`
/// uses: not a direct edge (it routes through a `var_read` and the call's
/// `call_res`), yet reachable.
#[test]
fn ts_class_method_body_flows_like_a_function() {
    let d = sandbox("ts_class");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/widget.ts"),
        "function format(value: string): string { return value; }\n\
         class Widget {\n    \
             render(name: string): string {\n        \
                 const label = format(name);\n        \
                 return label;\n    \
             }\n\
         }\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.ts\", path, rev), match(path, rev, /./, line).\n",
        "rel df_reaches(from: text, to: text).\n",
        "df_reaches(a, b) <- closure(df_edge).\n",
        "? df_node(id, kind, var, fn, file, line).\n",
        "? df_edge(from, to).\n",
        "? df_reaches(from, to).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "TS class lift must not error:\n{err}");

    let secs = sections(&out);
    assert!(secs.len() >= 3, "expected 3 query sections:\n{out}");

    let mut nodes: HashMap<String, (String, String, String)> = HashMap::new();
    for r in rows(&secs[0]) {
        assert!(r.len() >= 4, "df_node row too short: {r:?}");
        nodes.insert(r[0].clone(), (r[1].clone(), r[2].clone(), r[3].clone()));
    }
    let mut edges: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[1]) {
        assert!(r.len() >= 2, "df_edge row too short: {r:?}");
        edges.insert((r[0].clone(), r[1].clone()));
    }
    let mut reaches: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[2]) {
        assert!(r.len() >= 2, "df_reaches row too short: {r:?}");
        reaches.insert((r[0].clone(), r[1].clone()));
    }

    // The method's param `name` must be scoped under Widget.render's fn sym.
    let name_params: Vec<&String> = nodes
        .iter()
        .filter(|(_, (kind, var, fn_sym))| kind == "param" && var == "name" && fn_sym.contains("Widget.render"))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(name_params.len(), 1, "expected one param `name` under Widget.render:\n{out}");
    let name_id = name_params[0].clone();

    // The body's `const label = format(name)` must mint a let_bind under the
    // same fn scope.
    let label_binds: Vec<&String> = nodes
        .iter()
        .filter(|(_, (kind, var, fn_sym))| kind == "let_bind" && var == "label" && fn_sym.contains("Widget.render"))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(label_binds.len(), 1, "expected one let_bind `label` under Widget.render:\n{out}");
    let label_id = label_binds[0].clone();

    // There is a real graph to walk, not an empty one.
    assert!(!edges.is_empty(), "df_edge is empty for the method body:\n{out}");

    // name -> label is NOT a direct df_edge (it routes through a var_read and
    // format(name)'s call_res) ...
    assert!(
        !edges.contains(&(name_id.clone(), label_id.clone())),
        "name->label must NOT be a direct df_edge:\n{out}"
    );
    // ... yet closure reaches it: the method body's flow is a connected chain.
    assert!(
        reaches.contains(&(name_id, label_id)),
        "name must REACH label transitively through the method body:\n{out}"
    );
}

/// THE TAINT GATE. Exact dataflow says `m = a + 1` is NOT `a` (a new value), so
/// `a` would not reach `m`. Taint tracking propagates `a` THROUGH the operation
/// into `m`, so the source param `q` reaches the sink argument `m`. That reach,
/// routed via a `binop` node, is the proof the lift is doing taint-style
/// propagation through operations — the defining feature of taint vs dataflow.
/// Run on Rust, Kotlin, and TS to show the model generalizes across front-ends.
#[test]
fn taint_propagates_through_operations_per_language() {
    let rust = "fn sink(v: i32) {}\nfn go(q: i32) {\n    let a = q;\n    let m = a + 1;\n    sink(m);\n}\n";
    let kotlin = "fun sink(v: Int) {}\nfun go(q: Int) {\n    val a = q\n    val m = a + 1\n    sink(m)\n}\n";
    let ts = "function sink(v: number): void {}\nfunction go(q: number): void {\n    const a = q;\n    const m = a + 1;\n    sink(m);\n}\n";

    for (lang, ext, src) in [("rust", "rs", rust), ("kotlin", "kt", kotlin), ("ts", "ts", ts)] {
        let d = sandbox(&format!("taint_{lang}"));
        fs::create_dir_all(d.join("src")).unwrap();
        fs::write(d.join(format!("src/lib.{ext}")), src).unwrap();
        let prog = format!(
            "rel seen(path: file).\n\
             seen(path) <- scan(\"WORK\", \"src/**/*.{ext}\", path, rev), match(path, rev, /./, line).\n\
             rel df_reaches(from: text, to: text).\n\
             df_reaches(a, b) <- closure(df_edge).\n\
             ? df_node(id, kind, var, fn, file, line).\n\
             ? df_edge(from, to).\n\
             ? df_reaches(from, to).\n",
        );
        let (code, out, err) = run(&d, &prog);
        assert_eq!(code, 0, "[{lang}] must not error:\n{err}");

        let secs = sections(&out);
        assert!(secs.len() >= 3, "[{lang}] expected 3 query sections:\n{out}");
        let mut nodes: HashMap<String, (String, String)> = HashMap::new();
        for r in rows(&secs[0]) {
            nodes.insert(r[0].clone(), (r[1].clone(), r[2].clone()));
        }
        let mut edges: HashSet<(String, String)> = HashSet::new();
        for r in rows(&secs[1]) {
            edges.insert((r[0].clone(), r[1].clone()));
        }
        let mut reaches: HashSet<(String, String)> = HashSet::new();
        for r in rows(&secs[2]) {
            reaches.insert((r[0].clone(), r[1].clone()));
        }

        // the source: param `q`. the sink: the read of `m`. the operation: a
        // binop — except TS, where `+` mints its own `concat` kind (string-
        // values arc item 2: any-operand `+` qualifies, numeric included).
        let q_id = single(&nodes, "param", "q", &out, lang);
        let m_id = single(&nodes, "var_read", "m", &out, lang);
        let op_kind = if lang == "ts" { "concat" } else { "binop" };
        let binop = single(&nodes, op_kind, "", &out, lang);

        // q is NOT directly bound to m — the only route between them is the binop.
        assert!(
            !edges.contains(&(q_id.clone(), m_id.clone())),
            "[{lang}] q->m must NOT be a direct edge:\n{out}"
        );
        // q reaches the binop, the binop reaches m, and so q reaches m. The path
        // traverses the operation: exact dataflow would stop at `a + 1`.
        assert!(reaches.contains(&(q_id.clone(), binop.clone())), "[{lang}] q must reach the binop:\n{out}");
        assert!(reaches.contains(&(binop.clone(), m_id.clone())), "[{lang}] binop must reach m:\n{out}");
        assert!(reaches.contains(&(q_id.clone(), m_id.clone())), "[{lang}] q must reach m THROUGH the binop (taint):\n{out}");
    }
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").current_dir(dir).args(args).output().expect("git").status.success();
    assert!(ok, "git {args:?} in {}", dir.display());
}

/// D5b regression: the dataflow family must read a CONFIG repo's working tree,
/// exactly like the type/call families. Before the fix `refresh_dataflow_rels`
/// read every file from `self.root` (the --root), so a config repo whose root
/// differs was read at the wrong path (missing -> empty) or a git blob, never
/// its working tree: a WORK-only struct + ctor produced zero `df_node`/`df_field`
/// rows. type/call read via `repo_roots().get(repo)` and saw the same edit fine.
///
/// Setup: `--root` is `host` (a boring committed repo). `cfg` is a SEPARATE
/// config repo with a committed `src/widget.rs` (a struct, no ctor), then a
/// working-tree-only edit APPENDS a fn constructing that struct with a field
/// literal. The ctor is a `new` df_node and the field is a `df_field` — both
/// present ONLY in the working tree. They must appear, attributed to cfg's file.
#[test]
fn config_repo_work_edit_produces_dataflow_rows() {
    let d = sandbox("cfg_work_df");
    let host = d.join("host");
    let cfg = d.join("cfg");

    // host: the --root. A real committed repo with an unrelated file, so a bug
    // that reads self.root/path would read THIS tree, not cfg's.
    fs::create_dir_all(host.join("src")).unwrap();
    fs::write(host.join("src/lib.rs"), "fn main() {}\n").unwrap();
    git(&host, &["init", "-q"]);
    git(&host, &["config", "user.email", "t@t"]);
    git(&host, &["config", "user.name", "t"]);
    git(&host, &["add", "-A"]);
    git(&host, &["commit", "-qm", "x"]);

    // cfg: committed a struct with NO constructor. The uniquely-named path means
    // the buggy self.root read resolves to a nonexistent host/src/widget.rs.
    fs::create_dir_all(cfg.join("src")).unwrap();
    fs::write(cfg.join("src/widget.rs"), "pub struct Widget { pub part: i64 }\n").unwrap();
    git(&cfg, &["init", "-q"]);
    git(&cfg, &["config", "user.email", "t@t"]);
    git(&cfg, &["config", "user.name", "t"]);
    git(&cfg, &["add", "-A"]);
    git(&cfg, &["commit", "-qm", "x"]);

    // WORK-only edit: append a fn constructing Widget with a field literal. The
    // ctor (`new` node) and the `part` field exist ONLY in the working tree —
    // the committed rev has neither.
    fs::write(
        cfg.join("src/widget.rs"),
        "pub struct Widget { pub part: i64 }\n\
         pub fn mk() -> Widget { Widget { part: 7 } }\n",
    )
    .unwrap();

    fs::write(
        d.join("cfg.toml"),
        format!(
            "[[repos]]\n\
             slug = \"host\"\n\
             root = \"{host}\"\n\
             [[repos]]\n\
             slug = \"cfg\"\n\
             root = \"{cfg}\"\n",
            host = host.display(),
            cfg = cfg.display(),
        ),
    )
    .unwrap();

    // Fan over every config repo at WORK; querying df_node/df_field opts the
    // dataflow family in over the whole scanned set.
    fs::write(
        d.join("p.dl"),
        "rel seen(p: file).\n\
         seen(p) <- scan(\"*\", \"WORK\", \"src/**/*.rs\", p, rev).\n\
         ? df_node(id, kind, var, fn, file, line).\n\
         ? df_field(id, field, value).\n",
    )
    .unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--no-daemon", "--db", d.join("db").to_str().unwrap()])
        .current_dir(host)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output()
        .expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "run failed: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));

    let secs = sections(&stdout);
    assert!(secs.len() >= 2, "expected df_node + df_field sections:\n{stdout}");
    let node_rows = rows(&secs[0]);
    let field_rows = rows(&secs[1]);

    // DECISIVE: the WORK-only ctor is a `new` df_node for Widget, attributed to
    // cfg's file. Absent before the fix (df read host/src/widget.rs -> empty).
    let widget_ctor: Vec<&Vec<String>> = node_rows
        .iter()
        .filter(|r| r.len() >= 6 && r[1] == "new" && r[2] == "Widget" && r[4] == "src/widget.rs")
        .collect();
    assert!(
        !widget_ctor.is_empty(),
        "expected a `new` df_node for Widget from cfg's WORK edit:\n{stdout}"
    );

    // ... and the WORK-only field literal is a df_field with field `part`.
    assert!(
        field_rows.iter().any(|r| r.len() >= 3 && r[1] == "part"),
        "expected a df_field for the WORK-only `part` field literal:\n{stdout}"
    );
}

fn single(nodes: &HashMap<String, (String, String)>, kind: &str, var: &str, out: &str, lang: &str) -> String {
    let hits: Vec<&String> = nodes
        .iter()
        .filter(|(_, (k, v))| k == kind && v == var)
        .map(|(id, _)| id)
        .collect();
    assert_eq!(hits.len(), 1, "[{lang}] expected one {kind}/{var:?} node, got {}:\n{out}", hits.len());
    hits[0].clone()
}

/// THE CONTROL-FLOW GATE. T1 closed the loop/branch/closure holes so the lift
/// walks into loop bodies; T2 adds the `loop_over` relation and a flag rule that
/// joins loop spans against the lifted graph. This test reproduces the prog_rels
/// shape that cost us a real O(F*R) regression: a helper called once per loop
/// iteration with an argument that is a function parameter (loop-invariant), not
/// the loop variable. The flag rule must surface exactly that call.
///
/// The rule is a 2-hop join over plain `df_edge` (no closure needed): the param
/// node feeds a `var_read`, which feeds the `call_res`. A function parameter is
/// loop-invariant by definition, so param -> var_read -> call_res inside a loop
/// span, with the param name != loop variable, is the cheap proxy for the real
/// reaching-defs test we deliberately did not build.
#[test]
fn loop_invariant_call_flags_recomputation_per_iteration() {
    let d = sandbox("loopinv");
    fs::create_dir_all(d.join("src")).unwrap();
    // The prog_rels bug shape, distilled. `make_rels(prog)` is called inside the
    // loop over `rules`; `prog` is a param of `work` (loop-invariant), not the
    // loop variable `item`. T1 makes the loop body walkable; without it this
    // call_res would not exist at all.
    fs::write(
        d.join("src/lib.rs"),
        "fn make_rels(prog: &[i32]) -> Vec<i32> { Vec::new() }\n\
         fn work(rules: &[i32], prog: &[i32]) {\n    \
             for item in rules {\n        \
                 let rels = make_rels(prog);\n        \
                 let _ = rels;\n    \
             }\n\
         }\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        // the flag rule: a call_res inside a loop span whose direct argument is
        // a function param (2-hop path param -> var_read -> call_res), where the
        // param is not the loop variable.
        "rel lic(file: path, fn: text, loop_start: int, call_line: int, var: text).\n",
        "lic(file, fn, ls, cl, pname) <-\n",
        "    loop_over(file, ls, le, lvar, col, fn),\n",
        "    df_node(call_id, \"call_res\", _, fn, file, cl),\n",
        "    cl >= ls, cl <= le,\n",
        "    df_edge(vr, call_id),\n",
        "    df_edge(param, vr),\n",
        "    df_node(param, \"param\", pname, fn, _, _),\n",
        "    df_node(vr, \"var_read\", _, fn, _, _),\n",
        "    pname != lvar.\n",
        "? loop_over(file, start, end, var, collection, fn).\n",
        "? lic(file, fn, loop_start, call_line, var).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "loop flag rule must not error:\n{err}");

    let secs = sections(&out);
    assert!(secs.len() >= 2, "expected 2 query sections:\n{out}");

    // loop_over: exactly one loop, in `work`, with loop variable `item`.
    let loops = rows(&secs[0]);
    assert!(!loops.is_empty(), "expected a loop_over row — T1 must record the loop:\n{out}");
    assert!(
        loops.iter().any(|r| r.len() >= 6 && r[3] == "item" && r[5].contains("::work")),
        "expected one loop in `work` with var item:\n{out}"
    );

    // DECISIVE: the flag rule fires on `make_rels(prog)` inside the loop, with
    // var prog (the loop-invariant param). This is the prog_rels shape.
    let lics = rows(&secs[1]);
    assert!(
        !lics.is_empty(),
        "loop_invariant_call must fire — without T1 the loop body was a hole:\n{out}"
    );
    assert!(
        lics.iter().any(|r| r.len() >= 5 && r[1].contains("::work") && r[4] == "prog"),
        "expected lic on `work` flagging make_rels(prog) with var prog:\n{out}"
    );
}

/// THE PRECISION GATE. The broad rule (above) flags any loop-invariant arg; this
/// rule excludes calls that ALSO take a loop-carried input, isolating calls whose
/// EVERY input is loop-invariant — a pure recomputation of the same value each
/// iteration, i.e. the exact prog_rels waste. On the buggy fixture it fires
/// exactly once (make_rels in `work`); on a hoisted (fixed) version it fires zero
/// times. That 1-vs-0 discrimination is the difference between a suspect list
/// and a bug finder.
#[test]
fn strict_rule_isolates_pure_recomputation() {
    let d = sandbox("strict");
    fs::create_dir_all(d.join("src")).unwrap();
    // Buggy: make_rels(prog) recomputed per iteration; prog is the sole input and
    // is loop-invariant, so the call is fully loop-invariant -> flagged.
    fs::write(
        d.join("src/lib.rs"),
        "fn make_rels(prog: &[i32]) -> Vec<i32> { Vec::new() }\n\
         fn work(rules: &[i32], prog: &[i32]) {\n    \
             for item in rules {\n        \
                 let rels = make_rels(prog);\n        \
                 let _ = rels;\n    \
             }\n\
         }\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        // a call is loop-carried if any input derives from a def inside its loop.
        "rel lcc(call_id: text).\n",
        "lcc(c) <- df_edge(d, a), df_edge(a, c), df_node(d, _, _, fn, file, dl),\n",
        "       loop_over(file, ls, le, _, _, fn), dl >= ls, dl <= le.\n",
        // strict: allocating callee, call in loop, a loop-invariant input, and
        // NO loop-carried input at all -> pure recomputation.
        "rel lic_strict(file: path, fn: text, ls: int, cl: int, callee: text).\n",
        // call_name/call_site syms are repo-qualified (364de80); allocates (a
        // dataflow rel) stays bare. The bare sym `asym` is a suffix of the
        // qualified `csym`, so bridge the two sym-spaces with a containment test
        // (replace removes `asym` from `csym` iff it occurs).
        "lic_strict(file, fn, ls, cl, csym) <-\n",
        "    loop_over(file, ls, le, lvar, col, fn),\n",
        "    df_node(call_id, \"call_res\", _, fn, file, cl), cl >= ls, cl <= le,\n",
        "    call_site(_, caller, ctext, file, cl), call_name(csym, ctext),\n",
        "    allocates(asym), stripped = replace(csym, asym, \"\"), stripped != csym,\n",
        "    df_edge(def, arg), df_edge(arg, call_id),\n",
        "    df_node(def, _, _, fn, _, dl), dl < ls,\n",
        "    !lcc(call_id).\n",
        "? lic_strict(file, fn, ls, cl, callee).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "strict rule must not error:\n{err}");
    let secs = sections(&out);
    let hits = rows(&secs[0]);
    // DECISIVE: exactly one suspect, make_rels in `work`. No false positives.
    assert_eq!(
        hits.len(),
        1,
        "strict rule must isolate make_rels(prog) as the ONLY fully-loop-invariant call:\n{out}"
    );
    assert!(
        hits[0].len() >= 5 && hits[0][1].contains("::work") && hits[0][4].contains("make_rels"),
        "the single hit must be make_rels in work:\n{out}"
    );

    // Hoisted (fixed): move make_rels out of the loop -> zero hits.
    fs::write(
        d.join("src/lib.rs"),
        "fn make_rels(prog: &[i32]) -> Vec<i32> { Vec::new() }\n\
         fn work(rules: &[i32], prog: &[i32]) {\n    \
             let rels = make_rels(prog);\n    \
             for item in rules {\n        \
                 let _ = &rels;\n    \
             }\n\
         }\n",
    )
    .unwrap();
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "fixed version must not error:\n{err}");
    let secs = sections(&out);
    let hits = rows(&secs[0]);
    assert!(
        hits.is_empty(),
        "after hoisting, no fully-loop-invariant allocating call should remain:\n{out}"
    );
}

/// THE NEST GATE. A call inside two nested loops must produce exactly two
/// `nest` rows for that call: depth 1 (outer loop) and depth 2 (inner loop).
/// The outer loop starts earlier in the source so it sorts first; the rank is
/// the depth. The shape is what `nest ⨝ call_edge` composes into symbolic
/// Big-O ("depth-2 over C") without resolving trip counts. Asserting the
/// 1+2 depth pair on a single doubly-nested call is the decisive check that
/// the post-pass correctly walks (call_res, enclosing loops).
#[test]
fn nest_depth_records_loop_nesting_per_call() {
    let d = sandbox("nest");
    fs::create_dir_all(d.join("src")).unwrap();
    // Two nested loops in `go`, one call `make(o)` in the inner body. `make`
    // itself calls `Vec::new()` but that call sits in no loop -> zero nest rows
    // for it. So total nest rows = 2, both for the inner call.
    fs::write(
        d.join("src/lib.rs"),
        "fn make(x: i32) -> Vec<i32> { Vec::new() }\n\
         fn go(outer: &[i32], inner: &[i32]) {\n    \
             for o in outer {\n        \
                 for i in inner {\n            \
                     let v = make(o);\n            \
                     let _ = v;\n        \
                 }\n    \
             }\n\
         }\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        "? nest(call_id, loop_id, depth, collection).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "nest extraction must not error:\n{err}");

    let secs = sections(&out);
    assert!(!secs.is_empty(), "expected nest section:\n{out}");
    let nests = rows(&secs[0]);

    // DECISIVE: exactly two nest rows, both for the inner call, depths {1, 2}.
    assert_eq!(
        nests.len(),
        2,
        "expected exactly 2 nest rows (depth 1 + depth 2 for make(o) in the nested loop):\n{out}"
    );
    let depths: HashSet<&str> = nests.iter()
        .filter_map(|r| r.get(2).map(|s| s.as_str()))
        .collect();
    assert!(
        depths.contains("1") && depths.contains("2"),
        "expected depths {{1, 2}} for the doubly-nested call, got {depths:?}:\n{out}"
    );
    // both rows share the same call_id (one call site, two enclosing loops)
    let call_ids: HashSet<&str> = nests.iter()
        .filter_map(|r| r.first().map(|s| s.as_str()))
        .collect();
    assert_eq!(
        call_ids.len(),
        1,
        "both nest rows must reference the same call_id:\n{out}"
    );
    // and the loop_ids differ (outer vs inner)
    let loop_ids: HashSet<&str> = nests.iter()
        .filter_map(|r| r.get(1).map(|s| s.as_str()))
        .collect();
    assert_eq!(
        loop_ids.len(),
        2,
        "nest rows must reference two distinct loop_ids (outer + inner):\n{out}"
    );
}

/// THE COMPOSITION GATE. The whole point of `nest` is to compose over
/// `call_edge` into a symbolic cost shape: callee X reachable (transitively)
/// from a depth-N call site is "depth-N over C" without resolving trip counts.
/// Built in user-space Datalog, no engine change. The engine restricts closure
/// reads in rule bodies to pinned endpoints, so the join splits in three: a
/// `direct_cost` rule (no closure in body), the `call_reaches` closure as its
/// own query, and the transitive join in Rust -- the same shape
/// `rust_lift_closes_transitively` uses to prove df_reaches.
///
/// Chain under test: `go` calls `middle` inside a doubly-nested loop, `middle`
/// calls `leaf`. `direct_cost(middle, 2)` must hold (direct call from the
/// depth-2 site); the transitive join must yield `cost(leaf, 2)` via
/// call_reaches(middle, leaf). That leaf row is the proof the depth carried
/// across a call hop.
#[test]
fn nest_composes_over_call_edge_into_symbolic_cost() {
    let d = sandbox("nest_compose");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.rs"),
        "fn leaf(x: i32) -> i32 { x + 1 }\n\
         fn middle(x: i32) -> i32 { leaf(x) }\n\
         fn go(outer: &[i32], inner: &[i32]) {\n    \
             for o in outer {\n        \
                 for i in inner {\n            \
                     let v = middle(o);\n            \
                     let _ = v;\n        \
                 }\n    \
             }\n\
         }\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        // 2-col view of call_edge so closure() can walk it; call_edge itself
        // is 3-col (caller, callee, kind) and a closure head must be 2-col.
        "rel call_pair(a: text, b: text).\n",
        "call_pair(a, b) <- call_edge(a, b, _).\n",
        "rel call_reaches(a: text, b: text).\n",
        "call_reaches(a, b) <- closure(call_pair).\n",
        // direct: the callee at the depth-N call site has cost N. The closure
        // is NOT read here -- the engine blocks unpinned closure reads in rule
        // bodies, and direct_cost is precisely the seed the transitive join
        // propagates.
        "rel direct_cost(callee: text, depth: int).\n",
        // df_node.fn is the bare sym; call_site.caller is repo-qualified
        // (364de80), so the two no longer unify -- join the call_res node to its
        // call_site on (file, line) instead.
        "direct_cost(callee, depth) <-\n",
        "    nest(call_id, _, depth, _),\n",
        "    df_node(call_id, \"call_res\", _, _, file, line),\n",
        "    call_site(_, _, callee_text, file, line),\n",
        "    call_name(callee, callee_text).\n",
        "? call_reaches(caller, callee).\n",
        "? direct_cost(callee, depth).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "composition must not error:\n{err}");

    let secs = sections(&out);
    assert!(secs.len() >= 2, "expected 2 query sections:\n{out}");

    // Section 0: call_reaches -> (caller, callee) set.
    let mut reaches: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[0]) {
        assert!(r.len() >= 2, "call_reaches row too short: {r:?}");
        reaches.insert((r[0].clone(), r[1].clone()));
    }
    // Section 1: direct_cost -> callee -> depth.
    let mut direct: HashMap<String, String> = HashMap::new();
    for r in rows(&secs[1]) {
        assert!(r.len() >= 2, "direct_cost row too short: {r:?}");
        direct.insert(r[0].clone(), r[1].clone());
    }

    // DECISIVE 1: direct_cost(middle, 2) -- the call from the depth-2 site.
    let mid_cost = direct.iter()
        .find(|(s, _)| s.contains("::middle"))
        .map(|(_, d)| d.clone())
        .unwrap_or_default();
    assert_eq!(
        mid_cost, "2",
        "expected direct_cost(middle, 2) -- direct call from the depth-2 site:\n{out}"
    );

    // DECISIVE 2: leaf is NOT in direct_cost (no loop calls leaf directly) ...
    assert!(
        !direct.keys().any(|s| s.contains("::leaf")),
        "leaf must not be in direct_cost (no loop calls it directly):\n{out}"
    );
    // ... so the only route to a leaf cost is the transitive join. Find middle's
    // sym, then any call_reaches(middle, leaf) carries depth 2 to leaf.
    let mid_sym = direct.keys()
        .find(|s| s.contains("::middle"))
        .cloned()
        .unwrap_or_default();
    assert!(!mid_sym.is_empty(), "no middle sym found:\n{out}");
    let leaf_reached_from_mid: Vec<&(String, String)> = reaches.iter()
        .filter(|(a, b)| a == &mid_sym && b.contains("::leaf"))
        .collect();
    assert!(
        !leaf_reached_from_mid.is_empty(),
        "expected call_reaches(middle, leaf) to carry depth across the hop:\n{out}"
    );
    // That reach is the symbolic cost shape: leaf is "depth-2 over C" via middle.
}

/// Branch-tail flow: a value-position `if`/`match` must carry each branch's
/// tail value through to the binding. Pre-fix, `let x = if c { produce() }
/// else { fallback() }` dead-ended both call results inside their branches
/// (the arch_df unifier starvation): the `if` node had no incoming edges, so
/// nothing produced inside a branch ever reached `x`.
#[test]
fn rust_lift_carries_branch_tails_into_bindings() {
    let d = sandbox("branchtails");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.rs"),
        "fn produce() -> i64 { 1 }\n\
         fn fallback() -> i64 { 0 }\n\
         fn pick(k: i64) -> i64 { k }\n\
         fn consume(v: i64) {}\n\
         fn orchestrate(flag: bool, k: i64) {\n    \
             let x = if flag { produce() } else { fallback() };\n    \
             consume(x);\n    \
             let y = match k {\n        \
                 0 => pick(k),\n        \
                 _ => fallback(),\n    \
             };\n    \
             consume(y);\n\
         }\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        "rel df_reaches(from: text, to: text).\n",
        "df_reaches(a, b) <- closure(df_edge).\n",
        "? df_node(id, kind, var, fn, file, line).\n",
        "? df_reaches(from, to).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "Rust lift must not error:\n{err}");

    let secs = sections(&out);
    assert!(secs.len() >= 2, "expected 2 query sections:\n{out}");

    let mut nodes: HashMap<String, (String, String, String)> = HashMap::new();
    for r in rows(&secs[0]) {
        assert!(r.len() >= 6, "df_node row too short: {r:?}");
        nodes.insert(r[0].clone(), (r[1].clone(), r[2].clone(), r[5].clone()));
    }
    let mut reaches: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[1]) {
        assert!(r.len() >= 2, "df_reaches row too short: {r:?}");
        reaches.insert((r[0].clone(), r[1].clone()));
    }

    // Source nodes: the call_res on the `if` line (produce()/fallback() calls,
    // line 6) and on the match arms (lines 9/10). Sinks: let_bind x and y.
    let call_res_on = |line: &str| -> Vec<String> {
        nodes
            .iter()
            .filter(|(_, (k, _, l))| k == "call_res" && l == line)
            .map(|(id, _)| id.clone())
            .collect()
    };
    let bind_of = |var: &str| -> String {
        let hits: Vec<String> = nodes
            .iter()
            .filter(|(_, (k, v, _))| k == "let_bind" && v == var)
            .map(|(id, _)| id.clone())
            .collect();
        assert_eq!(hits.len(), 1, "expected one let_bind `{var}`:\n{out}");
        hits[0].clone()
    };

    let x_id = bind_of("x");
    let if_calls = call_res_on("6");
    assert_eq!(if_calls.len(), 2, "expected produce+fallback call_res on the if line:\n{out}");
    for c in &if_calls {
        assert!(
            reaches.contains(&(c.clone(), x_id.clone())),
            "if-branch tail call_res {c} must reach let_bind x:\n{out}"
        );
    }

    let y_id = bind_of("y");
    let arm_calls: Vec<String> = call_res_on("9").into_iter().chain(call_res_on("10")).collect();
    assert_eq!(arm_calls.len(), 2, "expected pick+fallback call_res on the match arms:\n{out}");
    for c in &arm_calls {
        assert!(
            reaches.contains(&(c.clone(), y_id.clone())),
            "match-arm tail call_res {c} must reach let_bind y:\n{out}"
        );
    }
}

/// Loop break-value flow: `loop { ... break EXPR; ... }` is Rust's only
/// value-yielding loop, so `let x = loop { ... break v; ... }` must carry v
/// through to x the same way an if/match tail does above. Pre-fix, `break`
/// fell into flow_expr's conservative default arm — the break's own value
/// expression was never even walked, so `produce()`/`fallback()` inside the
/// loop dead-ended: no node reached the `loop` expr, so nothing reached the
/// let_bind `outcome`.
#[test]
fn rust_lift_carries_loop_break_tails_into_bindings() {
    let d = sandbox("breaktails");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.rs"),
        "fn produce() -> i64 { 1 }\n\
         fn fallback() -> i64 { 0 }\n\
         fn consume(value: i64) {}\n\
         fn orchestrate(flag: bool) {\n    \
             let outcome = loop {\n        \
                 if flag {\n            \
                     break produce();\n        \
                 }\n        \
                 break fallback();\n    \
             };\n    \
             consume(outcome);\n\
         }\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        "rel df_reaches(from: text, to: text).\n",
        "df_reaches(a, b) <- closure(df_edge).\n",
        "? df_node(id, kind, var, fn, file, line).\n",
        "? df_reaches(from, to).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "Rust lift must not error:\n{err}");

    let secs = sections(&out);
    assert!(secs.len() >= 2, "expected 2 query sections:\n{out}");

    let mut nodes: HashMap<String, (String, String, String)> = HashMap::new();
    for r in rows(&secs[0]) {
        assert!(r.len() >= 6, "df_node row too short: {r:?}");
        nodes.insert(r[0].clone(), (r[1].clone(), r[2].clone(), r[5].clone()));
    }
    let mut reaches: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[1]) {
        assert!(r.len() >= 2, "df_reaches row too short: {r:?}");
        reaches.insert((r[0].clone(), r[1].clone()));
    }

    let call_res_on = |line: &str| -> Vec<String> {
        nodes
            .iter()
            .filter(|(_, (k, _, l))| k == "call_res" && l == line)
            .map(|(id, _)| id.clone())
            .collect()
    };
    let bind_of = |var: &str| -> String {
        let hits: Vec<String> = nodes
            .iter()
            .filter(|(_, (k, v, _))| k == "let_bind" && v == var)
            .map(|(id, _)| id.clone())
            .collect();
        assert_eq!(hits.len(), 1, "expected one let_bind `{var}`:\n{out}");
        hits[0].clone()
    };

    // produce() sits on line 7 (inside the `if`), fallback() on line 9 (the
    // loop's other break) — both must reach let_bind `outcome` on line 5.
    let outcome_id = bind_of("outcome");
    let break_calls: Vec<String> = call_res_on("7").into_iter().chain(call_res_on("9")).collect();
    assert_eq!(
        break_calls.len(), 2,
        "expected produce+fallback call_res on the break lines:\n{out}"
    );
    for c in &break_calls {
        assert!(
            reaches.contains(&(c.clone(), outcome_id.clone())),
            "loop break-value call_res {c} must reach let_bind outcome:\n{out}"
        );
    }
}

/// Labeled break-value flow: `break 'outer v` inside a loop nested under the
/// labeled `loop` must route v to the OUTER loop's node, not dead-end at the
/// inner (unlabeled) loop it lexically sits inside. Proves the `loop_breaks`
/// stack resolves a labeled break against its named frame rather than always
/// the innermost one.
#[test]
fn rust_lift_carries_labeled_loop_break_tails_into_bindings() {
    let d = sandbox("labeledbreaktails");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.rs"),
        "fn produce() -> i64 { 1 }\n\
         fn consume(value: i64) {}\n\
         fn orchestrate(flag: bool) {\n    \
             let outcome = 'outer: loop {\n        \
                 loop {\n            \
                     break 'outer produce();\n        \
                 }\n    \
             };\n    \
             consume(outcome);\n\
         }\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        "rel df_reaches(from: text, to: text).\n",
        "df_reaches(a, b) <- closure(df_edge).\n",
        "? df_node(id, kind, var, fn, file, line).\n",
        "? df_reaches(from, to).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "Rust lift must not error:\n{err}");

    let secs = sections(&out);
    assert!(secs.len() >= 2, "expected 2 query sections:\n{out}");

    let mut nodes: HashMap<String, (String, String, String)> = HashMap::new();
    for r in rows(&secs[0]) {
        assert!(r.len() >= 6, "df_node row too short: {r:?}");
        nodes.insert(r[0].clone(), (r[1].clone(), r[2].clone(), r[5].clone()));
    }
    let mut reaches: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[1]) {
        assert!(r.len() >= 2, "df_reaches row too short: {r:?}");
        reaches.insert((r[0].clone(), r[1].clone()));
    }

    let call_res_on = |line: &str| -> Vec<String> {
        nodes
            .iter()
            .filter(|(_, (k, _, l))| k == "call_res" && l == line)
            .map(|(id, _)| id.clone())
            .collect()
    };
    let bind_of = |var: &str| -> String {
        let hits: Vec<String> = nodes
            .iter()
            .filter(|(_, (k, v, _))| k == "let_bind" && v == var)
            .map(|(id, _)| id.clone())
            .collect();
        assert_eq!(hits.len(), 1, "expected one let_bind `{var}`:\n{out}");
        hits[0].clone()
    };

    // produce() sits on line 6 (`break 'outer produce();`), let_bind `outcome`
    // on line 4 (the OUTER `loop` binding).
    let outcome_id = bind_of("outcome");
    let break_calls = call_res_on("6");
    assert_eq!(break_calls.len(), 1, "expected one produce() call_res on the labeled break line:\n{out}");
    assert!(
        reaches.contains(&(break_calls[0].clone(), outcome_id.clone())),
        "labeled break-value call_res {} must reach let_bind outcome through the outer loop:\n{out}",
        break_calls[0]
    );
}

// ============================================================================
// D5.4 rev-twin composite key: df_node_rev.id must equal df_node.id.
//
// `salt_rev` folded rev into the id via string concat (`format!("{rev}\u{1}{id}")`),
// minting a SEPARATE `_strings` population per rev twin. Measured against the
// live root db 2026-07-20: 0 of 283,127 df_node rows share an id with any of
// the 297,620 df_node_rev rows. The fix makes rev a real trailing column and
// `(id, rev)` the primary key, so df_node_rev.id is the SAME interned value
// as df_node.id. These two tests prove: (1) the cross-table join by id now
// returns rows, and (2) two revs still keep their OWN rows rather than
// collapsing into one — the property salt_rev existed to provide, now
// delivered by the composite key instead of string folding.
// ============================================================================

fn git_rev(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `.dl` program scanning both the committed HEAD and the WORK tree,
/// referencing `df_node_rev` so the whole dataflow family (`rels_used` over
/// DATAFLOW_RELS, decls.rs:1076) lights up and populates across both revs.
const REV_TWIN_PROG: &str = r#"
rel diff_pair(base_rev: text, head_rev: text).
diff_pair("HEAD", "WORK").

rel seen(path: file).
seen(path) <- diff_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, rev).
seen(path) <- diff_pair(base_ref, _), scan(base_ref, "src/**/*.rs", path, rev).

? df_node_rev(id, kind, var, fn, file, line, rev).
"#;

/// Commit a shared `alpha`/`greet` pair as HEAD, then append a WORK-only
/// `beta` fn. `alpha`/`greet` sit on the SAME lines in both revs (byte-
/// identical), so their `file:line:col` ids are shared across HEAD and WORK;
/// `beta` is new content only WORK has.
fn write_rev_twin_fixture(dir: &Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    let common = "pub fn greet(name: &str) -> String { String::new() }\n\
                  pub fn alpha(name: &str) -> String {\n    \
                      let s = greet(name);\n    \
                      s\n\
                  }\n";
    fs::write(dir.join("src/a.rs"), common).unwrap();
    git_rev(dir, &["init", "-q"]);
    git_rev(dir, &["config", "user.email", "t@example.com"]);
    git_rev(dir, &["config", "user.name", "T"]);
    git_rev(dir, &["add", "."]);
    git_rev(dir, &["commit", "-q", "-m", "base"]);
    // WORK: common content untouched (same lines, same ids) + a WORK-only fn.
    let mut work = common.to_string();
    work.push_str(
        "pub fn beta(name: &str) -> String {\n    \
             let t = greet(name);\n    \
             t\n\
         }\n",
    );
    fs::write(dir.join("src/a.rs"), work).unwrap();
}

/// THE RED TEST. `rel_df_node JOIN rel_df_node_rev ON id` must return a
/// non-zero count: `alpha`/`greet` are scanned at both HEAD and WORK with
/// identical content, so df_node's raw id and df_node_rev's id must be the
/// SAME interned value for those nodes. Before the fix this join is 0 —
/// df_node_rev.id is `salt_rev(raw_id, rev)`, a string-concat mint landing in
/// a `_strings` population disjoint from the raw id.
#[test]
fn df_node_rev_id_matches_df_node_raw_id() {
    let d = sandbox("rev_twin_join");
    write_rev_twin_fixture(&d);

    let prog = parse::parse(lex::lex(REV_TWIN_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    for _ in 0..4 {
        eng.tick(&prog, true).unwrap();
    }

    // Sanity: both revs actually populated df_node_rev.
    let revs = eng.query_sql("SELECT DISTINCT rev FROM rel_df_node_rev_txt", &[]).unwrap();
    assert_eq!(revs.len(), 2, "df_node_rev must span HEAD + WORK: {revs:?}");

    let node_rows = eng.count_rows("df_node").unwrap();
    let node_rev_rows = eng.count_rows("df_node_rev").unwrap();
    assert!(node_rows > 0, "df_node must be non-empty");
    assert!(node_rev_rows > 0, "df_node_rev must be non-empty");

    let joined = eng
        .query_sql(
            "SELECT COUNT(*) FROM rel_df_node_txt n JOIN rel_df_node_rev_txt r ON n.id = r.id",
            &[],
        )
        .unwrap();
    let joined_count = joined[0][0].as_i64().unwrap();
    assert!(
        joined_count > 0,
        "df_node JOIN df_node_rev ON id must be non-empty (df_node={node_rows} rows, \
         df_node_rev={node_rev_rows} rows, joined=0 means df_node_rev.id is still a \
         salted, disjoint population)"
    );
}

/// THE DISJOINTNESS TEST. `alpha`/`greet` are byte-identical at the same
/// lines in HEAD and WORK, so their ids are SHARED (the INTERSECT below is
/// non-empty — the fix's whole point). `beta` exists only in WORK, so its ids
/// appear under the base rev NEVER. And every shared id still keeps two
/// SEPARATE rows (one per rev), not one row collapsed by an id-only key: two
/// revs stay disjoint rows via the `(id, rev)` composite key, the property
/// `salt_rev` existed to provide, now delivered by rev being a real column
/// instead of a string fold.
///
/// `rev` here is the resolved value the `scan("HEAD", ...)` ref lowers to
/// (the commit's real SHA, not the literal text "HEAD" — the same thing
/// `graph_diff_rev.rs`'s `head_rev`/`base_rev` derived rels capture), so the
/// base side is selected as `rev NOT LIKE '%+'` rather than a literal `'HEAD'`.
#[test]
fn df_node_rev_keeps_revs_disjoint_by_rev_column() {
    let d = sandbox("rev_twin_disjoint");
    write_rev_twin_fixture(&d);

    let prog = parse::parse(lex::lex(REV_TWIN_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    for _ in 0..4 {
        eng.tick(&prog, true).unwrap();
    }

    let shared = eng
        .query_sql(
            "SELECT COUNT(*) FROM (\
               SELECT id FROM rel_df_node_rev_txt WHERE rev NOT LIKE '%+' \
               INTERSECT \
               SELECT id FROM rel_df_node_rev_txt WHERE rev LIKE '%+')",
            &[],
        )
        .unwrap()[0][0]
        .as_i64()
        .unwrap();
    assert!(shared > 0, "alpha/greet nodes must be shared ids across the base rev and WORK");

    let work_only = eng
        .query_sql(
            "SELECT COUNT(*) FROM (\
               SELECT id FROM rel_df_node_rev_txt WHERE rev LIKE '%+' \
               EXCEPT \
               SELECT id FROM rel_df_node_rev_txt WHERE rev NOT LIKE '%+')",
            &[],
        )
        .unwrap()[0][0]
        .as_i64()
        .unwrap();
    assert!(work_only > 0, "beta's nodes are WORK-only and must not leak into the base rev's rows");

    // Every shared id keeps ONE row PER rev: total rows == HEAD rows + WORK
    // rows. If revs collapsed into one row per id (an (id)-only key instead
    // of (id, rev)) this equality would fail.
    let head_rows = eng
        .query_sql("SELECT COUNT(*) FROM rel_df_node_rev_txt WHERE rev NOT LIKE '%+'", &[])
        .unwrap()[0][0]
        .as_i64()
        .unwrap();
    let work_rows = eng
        .query_sql("SELECT COUNT(*) FROM rel_df_node_rev_txt WHERE rev LIKE '%+'", &[])
        .unwrap()[0][0]
        .as_i64()
        .unwrap();
    let total_rows = eng.count_rows("df_node_rev").unwrap();
    assert_eq!(
        total_rows,
        head_rows + work_rows,
        "shared ids must keep two separate rows (one per rev), not collapse into one"
    );
}

/// THE STRINGS RECEIPT. `salt_rev` minted a `format!("{rev}\u{1}{id}")` string
/// per twin id column, per row — before this arc, this exact fixture (2 revs,
/// 5 twin id-bearing columns) landed a `_strings` row containing the U+0001
/// separator for every one of those. The fix removes `salt_rev` entirely, so
/// this asserts the structural invariant directly against a fixed, checked-in
/// fixture rather than a hand-run count: zero `_strings` rows may contain the
/// separator, on any corpus, ever again. This is the deterministic, re-runnable
/// receipt for the "0 of N rows contain U+0001" measurement.
#[test]
fn corpus_strings_never_contain_the_salt_separator() {
    let d = sandbox("no_salt_strings");
    write_rev_twin_fixture(&d);

    let prog = parse::parse(lex::lex(REV_TWIN_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    for _ in 0..4 {
        eng.tick(&prog, true).unwrap();
    }

    let total_strings = eng
        .query_sql("SELECT COUNT(*) FROM _strings", &[])
        .unwrap()[0][0]
        .as_i64()
        .unwrap();
    assert!(total_strings > 0, "fixture must actually intern strings");

    let salted = eng
        .query_sql(
            "SELECT COUNT(*) FROM _strings WHERE content LIKE '%' || char(1) || '%'",
            &[],
        )
        .unwrap()[0][0]
        .as_i64()
        .unwrap();
    assert_eq!(
        salted, 0,
        "no _strings row may contain the U+0001 salt separator (salt_rev is gone): \
         {salted} salted of {total_strings} total"
    );
}

/// THE WITHOUT ROWID REGRESSION GUARD, applied to a rel this arc actually
/// touched. `df_node_repo_rev` (3 columns: id, repo, rev — all interned
/// INTEGER, no `key(...)`, `pk_never_null: true` in decls.rs) already
/// qualified for `WITHOUT ROWID` under `wants_without_rowid` BEFORE this arc
/// (salting only changed the id column's VALUE, never its type/count/key
/// shape) and must still qualify after desalting. Follows the same
/// methodology as tests/it/storage_diet_without_rowid.rs
/// `vouched_builtin_junction_gets_without_rowid_and_no_autoindex`: a real
/// dataflow lift through the engine, then read `sqlite_master` directly for
/// the `WITHOUT ROWID` DDL suffix and the absence of a duplicate full-row PK
/// autoindex.
#[test]
fn df_node_repo_rev_keeps_without_rowid_after_desalting() {
    let d = sandbox("repo_rev_without_rowid");
    write_rev_twin_fixture(&d);

    let prog = parse::parse(lex::lex(REV_TWIN_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    for _ in 0..4 {
        eng.tick(&prog, true).unwrap();
    }

    let rows = eng.count_rows("df_node_repo_rev").unwrap();
    assert!(rows > 0, "fixture must actually populate df_node_repo_rev, not just declare it");

    let table_sql = eng
        .query_sql(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'rel_df_node_repo_rev'",
            &[],
        )
        .unwrap()[0][0]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        table_sql.to_uppercase().ends_with("WITHOUT ROWID"),
        "df_node_repo_rev is a vouched Rust-authored 3-col all-integer no-key rel, \
         expected WITHOUT ROWID after desalting: {table_sql}"
    );

    let autoindex_count = eng
        .query_sql(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND tbl_name = 'rel_df_node_repo_rev' \
             AND name LIKE 'sqlite_autoindex_%'",
            &[],
        )
        .unwrap()[0][0]
        .as_i64()
        .unwrap();
    assert_eq!(
        autoindex_count, 0,
        "WITHOUT ROWID table must carry no duplicate full-row PK autoindex"
    );
}

// ============================================================================
// The KEY-SHAPE proof (dataflow-composite-key arc): the non-rev df_node and
// df_lit writers must dedup on the FULL declared PRIMARY KEY, not on `id`
// alone. `id` interns `file:line:col:kind`, so a position whose enclosing fn
// or bound var (or literal text) changed between the committed rev and WORK
// shares an id but is a DISTINCT row. id-only dedup dropped the second and
// left the table narrower than `SELECT DISTINCT <key cols>`, which is what
// blocked collapsing df_node/df_lit to a view over their _rev twins.
// ============================================================================

/// Commit `f(name)`/`g() { "aaaa" }` as HEAD, then edit the WORK tree so the
/// SAME byte positions carry `f(zzzz)`/`g() { "bbbb" }`. `name`->`zzzz` and
/// `"aaaa"`->`"bbbb"` are equal-length, so every node's `file:line:col:kind`
/// id is byte-identical across the two revs; only `var` (df_node) and `text`
/// (df_lit) diverge. That is the exact id-collision-with-divergent-payload
/// shape the full-row key must keep distinct.
fn write_divergent_rev_fixture(dir: &Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    let base = "pub fn f(name: &str) -> &str { name }\n\
                pub fn g() -> &'static str { \"aaaa\" }\n";
    fs::write(dir.join("src/a.rs"), base).unwrap();
    git_rev(dir, &["init", "-q"]);
    git_rev(dir, &["config", "user.email", "t@example.com"]);
    git_rev(dir, &["config", "user.name", "T"]);
    git_rev(dir, &["add", "."]);
    git_rev(dir, &["commit", "-q", "-m", "base"]);
    // WORK: identical byte layout, only the identifiers/literal text change.
    let work = "pub fn f(zzzz: &str) -> &str { zzzz }\n\
                pub fn g() -> &'static str { \"bbbb\" }\n";
    fs::write(dir.join("src/a.rs"), work).unwrap();
}

const DIVERGENT_PROG: &str = r#"
rel diff_pair(base_rev: text, head_rev: text).
diff_pair("HEAD", "WORK").

rel seen(path: file).
seen(path) <- diff_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, rev).
seen(path) <- diff_pair(base_ref, _), scan(base_ref, "src/**/*.rs", path, rev).

? df_node(id, kind, var, fn, file, line).
? df_lit(id, text, kind).
"#;

fn one(eng: &mut Engine, sql: &str) -> i64 {
    eng.query_sql(sql, &[]).unwrap()[0][0].as_i64().unwrap()
}

/// df_node keeps BOTH `name` and `zzzz` for the single shared param id, and the
/// table row count equals `SELECT DISTINCT id,kind,var,fn,file,line`. On the
/// old id-only dedup the second rev's row was dropped, so the shared id carried
/// exactly one row and `name`/`zzzz` never coexisted.
#[test]
fn df_node_keeps_divergent_var_across_revs() {
    let d = sandbox("divergent_node");
    write_divergent_rev_fixture(&d);
    let prog = parse::parse(lex::lex(DIVERGENT_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    for _ in 0..4 {
        eng.tick(&prog, true).unwrap();
    }

    // Both revs actually landed (via df_node_rev, which carries rev).
    let revs = one(
        &mut eng,
        "SELECT COUNT(*) FROM (SELECT DISTINCT rev FROM rel_df_node_rev_txt)",
    );
    assert_eq!(revs, 2, "fixture must span HEAD + WORK");

    // Both divergent var names survive under fn `f` (old code kept only one).
    let distinct_f_vars = one(
        &mut eng,
        "SELECT COUNT(*) FROM (SELECT DISTINCT var FROM rel_df_node_txt \
         WHERE kind = 'param' AND fn LIKE '%function::f')",
    );
    assert_eq!(
        distinct_f_vars, 2,
        "param of fn f must keep BOTH `name` and `zzzz` across revs"
    );

    // At least one id carries multiple rows (the shared param/var_read ids).
    let shared_ids = one(
        &mut eng,
        "SELECT COUNT(*) FROM (SELECT id FROM rel_df_node GROUP BY id HAVING COUNT(*) >= 2)",
    );
    assert!(
        shared_ids >= 1,
        "a shared id must keep >1 row when var diverges across revs; got {shared_ids}"
    );

    // THE INVARIANT: table rowcount == SELECT DISTINCT over the declared PK
    // columns. Before the fix these differed (dedup narrower than the PK);
    // now they agree, so df_node is safe to collapse to a DISTINCT view.
    let total = one(&mut eng, "SELECT COUNT(*) FROM rel_df_node");
    let distinct = one(
        &mut eng,
        "SELECT COUNT(*) FROM (SELECT DISTINCT id, kind, var, fn, file, line FROM rel_df_node)",
    );
    assert_eq!(
        total, distinct,
        "df_node dedup key must equal the declared PRIMARY KEY (view-DISTINCT)"
    );
}

/// df_lit keeps BOTH `"aaaa"` and `"bbbb"` for the single shared lit id, and the
/// table row count equals `SELECT DISTINCT id,text,kind`.
#[test]
fn df_lit_keeps_divergent_text_across_revs() {
    let d = sandbox("divergent_lit");
    write_divergent_rev_fixture(&d);
    let prog = parse::parse(lex::lex(DIVERGENT_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    for _ in 0..4 {
        eng.tick(&prog, true).unwrap();
    }

    // Both literal texts survive at the shared lit id (old code kept one).
    let distinct_texts = one(
        &mut eng,
        "SELECT COUNT(*) FROM (SELECT DISTINCT text FROM rel_df_lit_txt \
         WHERE text IN ('aaaa', 'bbbb'))",
    );
    assert_eq!(
        distinct_texts, 2,
        "df_lit must keep BOTH `aaaa` and `bbbb` across revs"
    );

    let shared_lit_ids = one(
        &mut eng,
        "SELECT COUNT(*) FROM (SELECT id FROM rel_df_lit GROUP BY id HAVING COUNT(*) >= 2)",
    );
    assert!(
        shared_lit_ids >= 1,
        "a shared lit id must keep >1 row when text diverges; got {shared_lit_ids}"
    );

    let total = one(&mut eng, "SELECT COUNT(*) FROM rel_df_lit");
    let distinct = one(
        &mut eng,
        "SELECT COUNT(*) FROM (SELECT DISTINCT id, text, kind FROM rel_df_lit)",
    );
    assert_eq!(
        total, distinct,
        "df_lit dedup key must equal the declared PRIMARY KEY (view-DISTINCT)"
    );
}
