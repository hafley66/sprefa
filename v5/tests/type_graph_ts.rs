//! Proof of the TypeScript/TSX side of the built-in `type_edge` relation: the
//! oxc-backed extractor emits edges in the shared kind vocabulary (field |
//! variant | impl | generic), so closure queries written for Rust work
//! unchanged on a TS codebase.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("type_graph_ts_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
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
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.{ts,tsx}", path, rev), match(path, rev, /./, line).
rel type_reaches(a: text, b: text).
type_reaches(a, b) <- closure(type_edge).
? type_edge(f, t, k).
? type_reaches(a, b).
"#;

#[test]
fn ts_type_edges_and_closure() {
    let d = sandbox("ts");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/model.ts"),
        r#"
export interface Entity { id: Id }
interface Pricing {}
export class Repo<T extends Entity> implements Pricing {
    cache: Cache<T>
    constructor(private db: Db) {}
}
export type Event = Created | Deleted
export enum Color { Red, Green }
"#,
    )
    .unwrap();
    fs::write(
        d.join("src/card.tsx"),
        r#"
interface CardProps { item: Entity }
export function Card({ item }: CardProps) { return <div>{item.id}</div> }
"#,
    )
    .unwrap();

    let (code, out, err) = run(&d, PROG);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("Entity\tId\tfield"), "interface property edge: {out}");
    assert!(out.contains("Repo\tEntity\tgeneric"), "type-param bound edge: {out}");
    assert!(out.contains("Repo\tPricing\timpl"), "implements edge: {out}");
    assert!(out.contains("Repo\tDb\tfield"), "ctor parameter-property edge: {out}");
    assert!(out.contains("Event\tCreated\tvariant"), "union variant edge: {out}");
    assert!(out.contains("Color\tColor::Red\tvariant"), "enum member edge: {out}");
    assert!(out.contains("CardProps\tEntity\tfield"), "tsx interface edge: {out}");

    // closure: Repo -> Entity -> Id without a direct edge
    let reaches = out.split("? type_reaches").nth(1).unwrap_or("");
    assert!(
        reaches.contains("Repo\tId"),
        "closure should walk bound + field edges: {out}"
    );
}

#[test]
fn kts_still_routes_to_kotlin() {
    // '%.ts' LIKE-matches '.kts' in the selection; the dispatch must still
    // hand .kts to the Kotlin extractor, not the TS one.
    let d = sandbox("kts");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/build.gradle.kts"),
        "class Wire(val store: Store)\n",
    )
    .unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.kts", path, rev), match(path, rev, /./, line).
? type_edge(f, t, k).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("Wire\tStore\tfield"), "kotlin field edge from .kts: {out}");
}

const ENTITY_PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.{ts,tsx}", path, rev), match(path, rev, /./, line).
? type_entity(sym, name, kind, parent, file, line).
? type_sig(sym, slot, pos, ref).
? type_link(src, dst, kind).
"#;

#[test]
fn ts_entities_sig_and_resolved_links() {
    // Two files: a model with the data types, a view module whose function
    // consumes them. Resolution must bind the view's refs to the model's defs.
    let d = sandbox("entities");
    fs::create_dir_all(d.join("src/core")).unwrap();
    fs::write(
        d.join("src/core/model.ts"),
        "export type Model = { id: Id }\nexport type View = { ok: boolean }\n",
    )
    .unwrap();
    fs::write(
        d.join("src/core/views.ts"),
        "import { Model, View } from './model'\nexport function cone(m: Model): View { return run() }\n",
    )
    .unwrap();

    let (code, out, err) = run(&d, ENTITY_PROG);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");

    let entity = out.split("? type_sig").next().unwrap_or("");
    let sig = out.split("? type_sig").nth(1).unwrap_or("").split("? type_link").next().unwrap_or("");
    let link = out.split("? type_link").nth(1).unwrap_or("");

    // entity table: kind + declaration line, sem-style symbol
    assert!(entity.contains("src/core/core/model.ts::alias::Model\tModel\talias")
        || entity.contains("src/core/model.ts::alias::Model\tModel\talias"), "entity row: {entity}");
    assert!(entity.contains("::function::cone\tcone\tfunction"), "function entity: {entity}");

    // arrow type, resolved across files: cone's param + ret point at model.ts defs
    assert!(sig.contains("::function::cone\tparam\t0\t") && sig.contains("model.ts::alias::Model"),
        "param resolved to def: {sig}");
    assert!(sig.contains("::function::cone\tret\t0\t") && sig.contains("model.ts::alias::View"),
        "return resolved to def: {sig}");

    // SCIP-resolved link graph: owner sym -> resolved target sym
    assert!(link.contains("::function::cone\t") && link.contains("model.ts::alias::Model\tparam"),
        "resolved link: {link}");
}
