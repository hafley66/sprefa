//! `Extends` and `ExtendsAll` (plans/v8/2026-09-30-extends.test-plan.md,
//! cases 1-18 and a scale case) through the public compiler on all five
//! engines. The walk reads `edge_snapshot`, last round's edges, at comptime,
//! so the cases read `compiler_rows`; cases 16-17 also run the rules at
//! runtime over `edge_snapshot` seeds.

use super::_9e_generic_rules::{render_row, ENGINES};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn compile(fixture: &str, engine: &str) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    compile_path(&root.join("fixtures/extends").join(fixture), engine)
}

fn compile_path(source: &Path, engine: &str) -> Value {
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(source)
        .env("DL8_ENGINE", engine)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{engine}: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let program: Value = serde_json::from_slice(&compiled.stdout).unwrap();
    assert_eq!(program["diagnostics"], json!([]), "{engine}");
    program
}

/// Rendered compiler rows of `relations`, sorted. A trailing `reason` value
/// is a prelude variant node whose rendering carries reader offsets; it is
/// cut.
fn select(program: &Value, relations: &[&str]) -> Vec<String> {
    select_rows(&program["compiler_rows"], &program["program"]["names"], relations)
}

fn select_rows(rows: &Value, names: &Value, relations: &[&str]) -> Vec<String> {
    let rendered: BTreeSet<String> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| render_row(row, names))
        .filter(|row| relations.iter().any(|relation| row.starts_with(&format!("{relation}: "))))
        .map(|row| match row.split_once(", ref(application(owner(prelude") {
            Some((kept, _)) => kept.to_owned(),
            None => row,
        })
        .collect();
    rendered.into_iter().collect()
}

/// Cases 1-15: 1 exact, 2 extra fields, 3 missing, 4 other field type, 5 empty
/// contract, 6 field order, 7 asymmetry, 8 same product field, 9 covariant
/// product field, 10 generic instance field, 11 recursive types, 12 every
/// product with `title: str`, 13 all pass, 14 one fails, 15 empty list.
#[test]
fn extends_cases_agree_on_every_engine() {
    let extends = vec![
        "Extends: ref(Book), ref(Titled), ref(Book)",
        "Extends: ref(Chain), ref(Node), ref(Chain)",
        "Extends: ref(Named), ref(Named), ref(Named)",
        "Extends: ref(Node), ref(Node), ref(Node)",
        "Extends: ref(OwnedByUser), ref(OwnerNamedContract), ref(OwnedByUser)",
        "Extends: ref(OwnedByUser), ref(OwnerUserContract), ref(OwnedByUser)",
        "Extends: ref(Post), ref(Titled), ref(Post)",
        "Extends: ref(Reordered), ref(Named), ref(Reordered)",
        "Extends: ref(StatusHolder), ref(StatusContract), ref(StatusHolder)",
        "Extends: ref(Titled), ref(Titled), ref(Titled)",
        "Extends: ref(User), ref(Aged), ref(User)",
        "Extends: ref(User), ref(Empty), ref(User)",
        "Extends: ref(User), ref(Named), ref(User)",
    ];
    let lists = vec![
        "ExtendsAll: ref(User), application(tuple, []), ref(User)",
        "ExtendsAll: ref(User), ref(application(tuple, [ref(Named), ref(Aged)])), ref(User)",
        "extends_all_checked: ref(User), ref(User)",
        "titled_type: ref(Book)",
        "titled_type: ref(Post)",
        "titled_type: ref(Titled)",
    ];
    // The failures at the asking site, contract `Titled` aside (case 12 asks
    // every product with a field).
    let diagnostics = vec![
        "extends_diagnostic: ref(Aged), ref(Named), const(name)",
        "extends_diagnostic: ref(Named), ref(Aged), const(age)",
        "extends_diagnostic: ref(Named), ref(User), const(age)",
        "extends_diagnostic: ref(NumericName), ref(Named), const(name)",
        "extends_diagnostic: ref(TaggedChain), ref(Tagged), const(next)",
        "extends_diagnostic: ref(TaggedChain), ref(Tagged), const(tag)",
    ];
    for engine in ENGINES {
        let program = compile("0_cases.dl7", engine);
        assert_eq!(select(&program, &["Extends"]), extends, "{engine}");
        assert_eq!(
            select(&program, &["ExtendsAll", "extends_all_checked", "titled_type"]),
            lists,
            "{engine}"
        );
        let failures: Vec<String> = select(&program, &["extends_diagnostic"])
            .into_iter()
            .filter(|row| !row.contains("ref(Titled)"))
            .collect();
        assert_eq!(failures, diagnostics, "{engine}");
    }
}

/// Case 18: fifty products and one ask. The walk reaches the asked pair and
/// its one shared field, nothing else.
#[test]
fn extends_checks_only_the_asked_pair() {
    let expected = vec![
        "Extends: ref(User), ref(Named), ref(User)",
        "candidate: ref(User), ref(Named)",
        "candidate: ref(primitive(str)), ref(primitive(str))",
    ];
    for engine in ENGINES {
        let program = compile("1_demand.dl7", engine);
        let rows = select(&program, &["Extends", "candidate", "fails_at", "fails"]);
        assert_eq!(rows, expected, "{engine}");
    }
}

fn scratch(tag: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join(format!("target/extends-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn eval(program: &Value, directory: &Path, engine: &str) -> Value {
    let path = directory.join("program.json");
    std::fs::write(&path, serde_json::to_vec(program).unwrap()).unwrap();
    let evaluated = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("eval")
        .arg(&path)
        .env("DL8_ENGINE", engine)
        .output()
        .unwrap();
    assert!(
        evaluated.status.success(),
        "{engine}: {}",
        String::from_utf8_lossy(&evaluated.stderr)
    );
    let closure: Value = serde_json::from_slice(&evaluated.stdout).unwrap();
    assert_eq!(closure["diagnostics"], json!([]), "{engine}");
    closure
}

const USER: &str = "(User:\n   (* (name: str)\n      (age: int)))";
const USER_WITHOUT_NAME: &str = "(User:\n   (* (age: int)))";
const AGED: &str = "(Aged:\n   (* (age: int)))";
const AGED_WITH_NAME: &str = "(Aged:\n   (* (age: int)\n      (name: str)))";

/// Cases 16 and 17 at comptime: an edit is a recompile, and the `Extends`
/// rows move by the one pair the edit touches.
#[test]
fn extends_rows_follow_a_field_edit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(root.join("fixtures/extends/2_edit.dl7")).unwrap();
    assert!(source.contains(USER) && source.contains(AGED));
    let directory = scratch("edit");
    let retracted = directory.join("16_retract.dl7");
    std::fs::write(&retracted, source.replace(USER, USER_WITHOUT_NAME)).unwrap();
    let added = directory.join("17_add.dl7");
    std::fs::write(&added, source.replace(AGED, AGED_WITH_NAME)).unwrap();
    let user = "Extends: ref(User), ref(Named), ref(User)";
    let aged = "Extends: ref(Aged), ref(Named), ref(Aged)";
    for engine in ENGINES {
        let base = select(&compile("2_edit.dl7", engine), &["Extends"]);
        assert_eq!(base, vec![user], "{engine} base");
        let after = select(&compile_path(&retracted, engine), &["Extends"]);
        assert_eq!(after, Vec::<String>::new(), "{engine} case 16");
        let after = select(&compile_path(&added, engine), &["Extends"]);
        assert_eq!(after, vec![aged, user], "{engine} case 17");
    }
    std::fs::remove_dir_all(directory).unwrap();
}

/// The `:` rows of a compile as `edge_snapshot` rows.
fn snapshot_rows(program: &Value) -> Vec<Value> {
    let colon = json!({"f": "ref", "args": [{"f": "kernel", "args": [{"a": ":"}]}]});
    let snapshot = json!({"f": "ref", "args": [{"f": "kernel", "args": [{"a": "edge_snapshot"}]}]});
    program["compiler_rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["args"][0] == colon)
        .map(|row| json!({"rel": snapshot, "args": row["args"][1]}))
        .collect()
}

/// Cases 16 and 17 at runtime. The program carries no type graph, so the
/// field rows go in as `edge_snapshot` seeds; ir:dd and ir:sqlite then take
/// `-User.name` and `+Aged.name` as two frontiers of one live engine, and the
/// other engines evaluate the seeds as edited.
#[test]
fn extends_rows_follow_snapshot_frontiers() {
    let user = "Extends: ref(User), ref(Named), ref(User)";
    let aged = "Extends: ref(Aged), ref(Named), ref(Aged)";
    let directory = scratch("frontiers");
    for engine in ENGINES {
        let mut program = compile("2_edit.dl7", engine);
        let names = program["program"]["names"].clone();
        let snapshots = snapshot_rows(&program);
        let field = |owner: &str, name: &str| {
            snapshots
                .iter()
                .find(|row| {
                    super::_9e_generic_rules::render(&row["args"][0], &names)
                        == format!("ref({owner})")
                        && row["args"][1] == json!({"f": "const", "args": [{"a": name}]})
                })
                .cloned()
                .unwrap()
        };
        let user_name = field("User", "name");
        let mut aged_name = field("Aged", "age");
        aged_name["args"][1] = json!({"f": "const", "args": [{"a": "name"}]});
        aged_name["args"][2] = user_name["args"][2].clone();
        aged_name["args"][3] = json!({"f": "const", "args": [1]});
        program["program"]["seeds"]
            .as_array_mut()
            .unwrap()
            .extend(snapshots.iter().cloned());
        let closure = eval(&program, &directory, engine);
        assert_eq!(select_rows(&closure["closure"], &names, &["Extends"]), vec![user], "{engine}");

        let steps: Vec<Vec<String>> = if engine.starts_with("ir:") {
            let frontiers = directory.join("frontiers.json");
            let change = |row: &Value, weight: i64| {
                json!({"rel": row["rel"], "args": row["args"], "w": weight})
            };
            let body = json!([
                {"changes": [change(&user_name, -1)]},
                {"changes": [change(&aged_name, 1)]}
            ]);
            std::fs::write(&frontiers, serde_json::to_vec(&body).unwrap()).unwrap();
            let run = Command::new(env!("CARGO_BIN_EXE_dl8"))
                .arg("run")
                .arg(directory.join("program.json"))
                .arg("--frontiers")
                .arg(&frontiers)
                .env("DL8_ENGINE", engine)
                .output()
                .unwrap();
            assert!(run.status.success(), "{engine}: {}", String::from_utf8_lossy(&run.stderr));
            let run: Value = serde_json::from_slice(&run.stdout).unwrap();
            assert_eq!(run["diagnostics"], json!([]), "{engine}");
            run["frontiers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|frontier| {
                    assert_eq!(frontier["closure"]["diagnostics"], json!([]), "{engine}");
                    select_rows(&frontier["closure"]["closure"], &names, &["Extends"])
                })
                .collect()
        } else {
            let seeds = program["program"]["seeds"].as_array_mut().unwrap();
            seeds.retain(|seed| *seed != user_name);
            let retracted = eval(&program, &directory, engine);
            program["program"]["seeds"].as_array_mut().unwrap().push(aged_name.clone());
            let added = eval(&program, &directory, engine);
            [retracted, added]
                .iter()
                .map(|closure| select_rows(&closure["closure"], &names, &["Extends"]))
                .collect()
        };
        assert_eq!(steps, vec![vec![], vec![aged.to_owned()]], "{engine}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}

/// Deterministic draws for the scale program.
struct Draw(u64);

impl Draw {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % bound
    }
}

/// A field's target: a primitive, or another product by index.
#[derive(Clone, PartialEq)]
enum Target {
    Primitive(&'static str),
    Product(usize),
}

/// Two hundred products over ten field names, fifty asks. Field targets are
/// `str`, `int`, or one of the first four products, so nested and cyclic
/// pairs occur. The first forty products carry `f0` alone and contracts come
/// from the first sixty, so both answers occur.
fn scale_program() -> (String, Vec<Vec<(usize, Target)>>, Vec<(usize, usize)>) {
    let mut draw = Draw(7);
    let mut products = Vec::new();
    for index in 0..200 {
        let mut fields = Vec::new();
        for name in 0..10 {
            if name > 0 && (index < 40 || draw.next(3) != 0) {
                continue;
            }
            let target = match draw.next(6) {
                0 => Target::Product(draw.next(4) as usize),
                1 | 2 => Target::Primitive("int"),
                _ => Target::Primitive("str"),
            };
            fields.push((name, target));
        }
        products.push(fields);
    }
    let mut asks = Vec::new();
    while asks.len() < 50 {
        let ask = (draw.next(200) as usize, draw.next(60) as usize);
        if !asks.contains(&ask) {
            asks.push(ask);
        }
    }
    let mut text = String::new();
    for (index, fields) in products.iter().enumerate() {
        text.push_str(&format!("(P{index:03}:\n   (*"));
        for (name, target) in fields {
            let target = match target {
                Target::Primitive(primitive) => primitive.to_string(),
                Target::Product(product) => format!("P{product:03}"),
            };
            text.push_str(&format!("\n      (f{name}: {target})"));
        }
        text.push_str("))\n\n");
    }
    for (source, contract) in &asks {
        text.push_str(&format!("(asked P{source:03} P{contract:03})\n"));
    }
    (text, products, asks)
}

/// The oracle: every contract field has a same-named source field whose
/// target extends the contract's. A pair met again on the walk holds.
fn oracle_extends(
    products: &[Vec<(usize, Target)>],
    source: &Target,
    contract: &Target,
    walking: &mut Vec<(usize, usize)>,
) -> bool {
    match (source, contract) {
        (_, Target::Primitive(_)) => source == contract,
        (Target::Primitive(_), Target::Product(contract)) => products[*contract].is_empty(),
        (Target::Product(source), Target::Product(contract)) => {
            if walking.contains(&(*source, *contract)) {
                return true;
            }
            walking.push((*source, *contract));
            let holds = products[*contract].iter().all(|(name, wanted)| {
                products[*source]
                    .iter()
                    .find(|(have, _)| have == name)
                    .is_some_and(|(_, target)| oracle_extends(products, target, wanted, walking))
            });
            walking.pop();
            holds
        }
    }
}

/// Scale: 200 products, 50 asks, exact rows against the oracle on every
/// engine, each compile under ten seconds.
#[test]
fn extends_at_scale_matches_the_oracle() {
    let (text, products, asks) = scale_program();
    let directory = scratch("scale");
    let source = directory.join("scale.dl7");
    std::fs::write(&source, text).unwrap();
    let expected: BTreeSet<String> = asks
        .iter()
        .filter(|(source, contract)| {
            let (source, contract) = (Target::Product(*source), Target::Product(*contract));
            oracle_extends(&products, &source, &contract, &mut Vec::new())
        })
        .map(|(source, contract)| {
            format!("Extends: ref(P{source:03}), ref(P{contract:03}), ref(P{source:03})")
        })
        .collect();
    let expected: Vec<String> = expected.into_iter().collect();
    assert!(expected.len() >= 5 && expected.len() < 45, "{}", expected.len());
    for engine in ENGINES {
        let started = Instant::now();
        let program = compile_path(&source, engine);
        let elapsed = started.elapsed();
        println!("{engine}: {elapsed:?}, {} of {} asks hold", expected.len(), asks.len());
        assert!(elapsed < Duration::from_secs(10), "{engine}: {elapsed:?}");
        assert_eq!(select(&program, &["Extends"]), expected, "{engine}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}
