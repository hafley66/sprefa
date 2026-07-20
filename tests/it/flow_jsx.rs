//! JSX prop flow (`examples/flow-jsx.dl`).
//!
//! `<Card title={secret}/>` desugars to jsx(Card, {title: secret}): the lift
//! emits the element as a `new` df_node with a df_field per prop, the call-site
//! extractor records the usage as a call (so call_edge resolves caller -> Card),
//! and destructured props params bind by PROPERTY name. flow-jsx.dl joins the
//! three by name into `prop_edge`.
//!
//! THE GATE: App passes `secret` as `title` and `other` as `note`; Card only
//! declares `title`. prop_edge must connect secret's read to Card's `title`
//! param and must NOT invent a `note` edge — the name match is the precision.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const PROG: &str = include_str!("../../examples/flow-jsx.dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flow_jsx_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run(dir: &Path) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), PROG).unwrap();
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
        .filter(|l| !l.starts_with("? ") && l.contains('\t') && !l.contains("(0 rows)"))
        .map(|l| {
            l.strip_prefix("  ")
                .unwrap_or(l)
                .split('\t')
                .map(|s| s.trim_end().to_string())
                .collect()
        })
        .collect()
}

/// Section order in flow-jsx.dl: jsx_use, prop_edge.
#[test]
fn jsx_props_flow_into_the_component_by_name() {
    let d = sandbox("gate");
    fs::write(
        d.join("src/app.tsx"),
        "function Card({title}: {title: string}) {\n    \
             const shown = title;\n    \
             return <div label={shown} />;\n\
         }\n\
         function App(secret: string, other: string) {\n    \
             return <Card title={secret} note={other} />;\n\
         }\n",
    )
    .unwrap();
    let (code, out, err) = run(&d);
    assert_eq!(code, 0, "must not error:\n{err}");

    let secs = sections(&out);
    assert!(secs.len() >= 2, "expected 2 query sections:\n{out}");

    // jsx_use: the Card usage records both props; the host <div> records its
    // label prop too (the lift covers host elements, resolution skips them).
    let uses = rows(&secs[0]);
    assert!(
        uses.iter().any(|r| r.len() >= 2 && r[0] == "Card" && r[1] == "title"),
        "expected jsx_use(Card, title):\n{out}"
    );
    assert!(
        uses.iter().any(|r| r.len() >= 2 && r[0] == "div" && r[1] == "label"),
        "expected jsx_use(div, label):\n{out}"
    );

    // prop_edge: secret's read flows into Card's `title` param node.
    let props = rows(&secs[1]);
    assert!(
        props.iter().any(|r| {
            r.len() >= 4
                && r[1] == "Card"
                && r[2] == "title"
                && r[0].contains(":var_read")
                && r[3].contains(":param")
        }),
        "expected prop_edge(secret_read, Card, title, title_param):\n{out}"
    );
    // DECISIVE negative: `note` names no param of Card — no edge invented.
    assert!(
        !props.iter().any(|r| r.len() >= 3 && r[2] == "note"),
        "prop_edge must NOT connect the undeclared `note` prop:\n{out}"
    );
    // And nothing resolved to the host element.
    assert!(
        !props.iter().any(|r| r.len() >= 2 && r[1] == "div"),
        "host elements must not resolve as components:\n{out}"
    );
}

/// Conditional / logical / parenthesized prop values dataflow through to the
/// component. `title={ok ? secret : fallback}` must carry BOTH branch reads;
/// `subtitle={secret ?? backup}` carries both `??` operands; `note={(guarded
/// && secret)}` carries the `&&` VALUE (`secret`) through the parens but NOT
/// the guard (`guarded`). Without the expression arms these prop values
/// dead-end at an unlinked `expr` node and nothing reaches the param.
#[test]
fn jsx_conditional_and_logical_prop_values_flow_through() {
    let d = sandbox("exprs");
    fs::write(
        d.join("src/app.tsx"),
        "function Card({title, subtitle, note, label, opt, items}: any) {\n    \
             return <div>{title}{subtitle}{note}{label}{opt}{items}</div>;\n\
         }\n\
         function App(secret: string, fallback: string, backup: string, ok: boolean, guarded: boolean, bag: any, first: string) {\n    \
             return <Card title={ok ? secret : fallback} subtitle={secret ?? backup} note={(guarded && secret)} label={`hi ${secret}`} opt={bag?.secret} items={[first, secret]} />;\n\
         }\n",
    )
    .unwrap();

    // Self-contained reach program: the JSX prop hop from flow-jsx.dl, unioned
    // into std/flow.dl's flow_edge, then a seeded transitive reach so a named
    // source read reaching a resolved prop target proves the branch flowed.
    let prog = r#"
use "std/flow.dl".
rel src(p: file).
src(p) <- scan("WORK", "src/**/*.{ts,tsx}", p, rev).

rel prop_edge(value: text, comp: text, prop: text, target: text).
prop_edge(value, comp, prop, target) <-
    df_node(elem, "new", comp, caller_bare, _, _, _),
    df_field(elem, prop, value),
    call_edge_bare(caller_bare, callee_bare, _, callee_q),
    call_name(callee_q, comp),
    df_node(target, "param", prop, callee_bare, _, _, _).
flow_edge(value, target) <- prop_edge(value, _, _, target).

rel reach(from: text, to: text).
reach(a, b) <- flow_edge(a, b).
reach(a, c) <- reach(a, b), flow_edge(b, c).

rel src_read(id: text, name: text).
src_read(id, name) <- df_node(id, "var_read", name, _, _, _, _).

rel prop_reaches(name: text, comp: text, prop: text).
prop_reaches(name, comp, prop) <-
    src_read(from, name),
    reach(from, target),
    prop_edge(_, comp, prop, target).

? prop_reaches(name, comp, prop).
"#;
    fs::write(d.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&d)
        .output()
        .expect("run dl");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(code, 0, "must not error:\n{stderr}");

    let secs = sections(&stdout);
    let reaches = rows(&secs[0]);
    let hit = |name: &str, prop: &str| {
        reaches.iter().any(|r| r.len() >= 3 && r[0] == name && r[1] == "Card" && r[2] == prop)
    };
    // Conditional: both branches reach the title prop.
    assert!(hit("secret", "title"), "conditional consequent must flow to title:\n{stdout}");
    assert!(hit("fallback", "title"), "conditional alternate must flow to title:\n{stdout}");
    // `??`: both operands reach subtitle.
    assert!(hit("secret", "subtitle"), "?? left must flow to subtitle:\n{stdout}");
    assert!(hit("backup", "subtitle"), "?? right must flow to subtitle:\n{stdout}");
    // `(guarded && secret)`: the value (`secret`) flows through the parens.
    assert!(hit("secret", "note"), "&& value must flow through parens to note:\n{stdout}");
    // `` `hi ${secret}` ``: the interpolation flows through the template.
    assert!(hit("secret", "label"), "template interpolation must flow to label:\n{stdout}");
    // `bag?.secret`: optional chaining is transparent to the base object.
    assert!(hit("bag", "opt"), "optional-chain base must flow to opt:\n{stdout}");
    // `[first, secret]`: both array elements flow through.
    assert!(hit("first", "items"), "array element must flow to items:\n{stdout}");
    assert!(hit("secret", "items"), "array element must flow to items:\n{stdout}");
    // DECISIVE negative: `&&` left is a truthiness guard, not a value.
    assert!(!hit("guarded", "note"), "&& guard must NOT flow as a value:\n{stdout}");
}

/// The member-read target shape: a component that keeps `props` whole and
/// reads `props.title` still receives the flow (df_node member var carries
/// the accessed name; the second prop_edge rule matches it).
#[test]
fn jsx_props_reach_member_reads_too() {
    let d = sandbox("member");
    fs::write(
        d.join("src/app.tsx"),
        "function Panel(props: {title: string}) {\n    \
             const shown = props.title;\n    \
             return <span text={shown} />;\n\
         }\n\
         function App(secret: string) {\n    \
             return <Panel title={secret} />;\n\
         }\n",
    )
    .unwrap();
    let (code, out, err) = run(&d);
    assert_eq!(code, 0, "must not error:\n{err}");

    let secs = sections(&out);
    let props = rows(&secs[1]);
    assert!(
        props.iter().any(|r| {
            r.len() >= 4 && r[1] == "Panel" && r[2] == "title" && r[3].contains(":member")
        }),
        "expected prop_edge into Panel's props.title member read:\n{out}"
    );
}
