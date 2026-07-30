//! Proof of Win H: plain JavaScript (`.js`/`.jsx`) rides the same oxc-backed
//! `TsTypes` front-end as `.ts`/`.tsx` (see `typegraph::TsTypes::matches` and
//! `typegraph::source_type_for`), so a JS-only codebase gets `type_entity`,
//! `call_edge`, `df_node`/`df_field`, and `doc_comment` rows for free, the
//! same families a TS codebase gets. `type_link`/`type_sig` stay thin (a JS
//! file carries no type annotations to resolve), which is expected, not a bug.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("type_graph_js_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.js", path, rev), match(path, rev, /./, line).
seen(path) <- scan("WORK", "src/**/*.jsx", path, rev), match(path, rev, /./, line).
? type_entity(repo, sym, name, kind, parent, file, line).
? call_edge(caller, callee, kind).
? df_node(id, kind, var, callee_fn, file, line, _).
? df_field(id, field, value).
? doc_comment(repo, sym, line, text).
"#;

#[test]
fn plain_js_and_jsx_populate_every_extraction_family() {
    let d = sandbox("gate");
    fs::create_dir_all(d.join("src")).unwrap();
    // A doc'd class + a plain function, referenced cross-file.
    fs::write(
        d.join("src/model.js"),
        "/** Widget model. */\n\
         export class Widget {\n    \
             constructor(part) { this.part = part; }\n\
         }\n\n\
         export function helper() {\n    \
             return 42;\n\
         }\n",
    )
    .unwrap();
    fs::write(
        d.join("src/app.js"),
        "import { helper } from './model.js';\n\n\
         export function run() {\n    \
             return helper();\n\
         }\n",
    )
    .unwrap();
    // A JSX component usage in a bare .jsx file (not .tsx): the df_field
    // component-prop lift (Win H's JSX assertion).
    fs::write(
        d.join("src/card.jsx"),
        "export function Card({ title }) {\n    \
             return <div>{title}</div>;\n\
         }\n\n\
         export function App() {\n    \
             return <Card title=\"hi\" />;\n\
         }\n",
    )
    .unwrap();

    let (code, out, err) = run(&d, PROG);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");

    // type_entity: a class and a function in .js, both functions in .jsx.
    assert!(
        out.contains("src/model.js::class::Widget\tWidget\tclass"),
        "class entity: {out}"
    );
    assert!(
        out.contains("src/model.js::function::helper\thelper\tfunction"),
        "fn entity: {out}"
    );
    assert!(
        out.contains("src/app.js::function::run\trun\tfunction"),
        "cross-file fn entity: {out}"
    );
    assert!(
        out.contains("src/card.jsx::function::Card\tCard\tfunction"),
        "jsx fn entity: {out}"
    );
    assert!(
        out.contains("src/card.jsx::function::App\tApp\tfunction"),
        "jsx fn entity: {out}"
    );

    // call_edge across two plain-.js files: app.js's run() calls model.js's helper().
    assert!(
        out.contains("src/app.js::function::run\t") && {
            let calls = out.split("? call_edge").nth(1).unwrap_or("");
            calls.contains("src/app.js::function::run\t")
                && calls.contains("src/model.js::function::helper\tcall")
        },
        "cross-file call_edge run->helper: {out}"
    );

    // df_node: at least a ret + call_res node inside run(), proving the
    // dataflow lift runs over plain .js the same as .ts.
    let df_nodes = out
        .split("? df_node")
        .nth(1)
        .unwrap_or("")
        .split("? df_field")
        .next()
        .unwrap_or("");
    assert!(
        df_nodes
            .lines()
            .any(|line| line.contains("src/app.js") && line.contains("\tret\t")),
        "df_node ret in .js: {df_nodes}"
    );
    assert!(
        df_nodes
            .lines()
            .any(|line| line.contains("src/app.js") && line.contains("\tcall_res\t")),
        "df_node call_res in .js: {df_nodes}"
    );

    // df_field: the JSX <Card title="hi" /> usage lifts as a `new` node with a
    // `title` field pointing at a `lit` value node, in a bare .jsx file.
    let df_fields = out
        .split("? df_field")
        .nth(1)
        .unwrap_or("")
        .split("? doc_comment")
        .next()
        .unwrap_or("");
    assert!(
        df_fields
            .lines()
            .any(|line| line.contains("card.jsx") && line.contains("\ttitle\t")),
        "jsx df_field title prop: {df_fields}"
    );

    // doc_comment: the /** Widget model. */ block resolves to the Widget entity.
    let docs = out.split("? doc_comment").nth(1).unwrap_or("");
    assert!(
        docs.contains("src/model.js::class::Widget") && docs.contains("Widget model."),
        "doc_comment: {docs}"
    );
}
