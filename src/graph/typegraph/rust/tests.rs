//! Rust extractor test suite. Split out of rust/mod.rs to keep the
//! production file under the 1,500-line budget.
#![cfg(test)]

use super::*;

#[test]
fn extracts_fields_variants_impls_and_generics() {
    let src = r#"
        trait Identity {}
        trait Store {}
        struct Id;
        struct Meta<T>(T);
        struct User<T: Identity> { id: Id, meta: Option<Meta<T>> }
        enum Event { Created(User<Id>), Deleted { id: Id } }
        impl<T: Identity> Store for User<T> {}
    "#;
    let got = edges(src);
    assert!(got.contains(&TypeEdge {
        from: "User".into(),
        to: "Id".into(),
        kind: "field"
    }));
    assert!(got.contains(&TypeEdge {
        from: "User".into(),
        to: "Identity".into(),
        kind: "generic"
    }));
    assert!(got.contains(&TypeEdge {
        from: "Event".into(),
        to: "Event::Created".into(),
        kind: "variant"
    }));
    assert!(got.contains(&TypeEdge {
        from: "Event::Created".into(),
        to: "User".into(),
        kind: "field"
    }));
    assert!(got.contains(&TypeEdge {
        from: "User".into(),
        to: "Store".into(),
        kind: "impl"
    }));
}

#[test]
fn rust_entities_kinds_and_arrow_types() {
    let src = "\
pub struct Engine { db: Db }
pub enum Mode { A, B }
pub trait Sink {}
pub fn run(e: Engine, n: usize) -> Report { todo!() }
impl Engine {
pub fn tick(&self, db: Db) -> Result { todo!() }
}
";
    let es = rust_entities("src/engine.rs", src);
    let by = |name: &str| es.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("missing {name}: {es:?}"));
    assert_eq!(by("Engine").kind, EntityKind::Struct);
    assert_eq!(by("Mode").kind, EntityKind::Enum);
    assert_eq!(by("Sink").kind, EntityKind::Trait);
    assert_eq!(by("run").kind, EntityKind::Function);
    assert_eq!(by("Engine").line, 1);
    assert_eq!(by("run").line, 4);
    // free fn arrow type, receiver excluded on the method
    let run = by("run").ty.as_ref().unwrap();
    assert_eq!(run.params[0], vec![TypeRef::Named("Engine".into())]);
    assert!(run.params[1].is_empty(), "usize is primitive: {run:?}");
    assert_eq!(run.ret, vec![TypeRef::Named("Report".into())]);
    let tick = by("tick");
    assert_eq!(tick.kind, EntityKind::Method);
    assert_eq!(tick.parent.as_deref(), Some("src/engine.rs::struct::Engine"));
    let tty = tick.ty.as_ref().unwrap();
    assert_eq!(tty.params, vec![vec![TypeRef::Named("Db".into())]], "self dropped: {tty:?}");
}

#[test]
fn rust_lift_ctors_args_fields_members() {
    let src = "struct Cfg { host: i32, port: i32 }\n\
               fn go(h: i32, items: Vec<i32>) {\n    \
                   let c = Cfg { host: h, port: 1 };\n    \
                   let x = c.host;\n    \
                   let w = Wrap(x);\n    \
                   let n = items.len();\n    \
                   eat(n, x);\n\
               }\n";
    let df = RustTypes.extract_dataflow("f.rs", src);

    // struct literal and tuple-struct ctor are `new` nodes with type names.
    let cfg = dnode(&df, "new", "Cfg").id.clone();
    let wrap = dnode(&df, "new", "Wrap").id.clone();
    // struct-literal fields land in df_field by name.
    let h_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "h").unwrap().id.clone();
    assert!(has_field(&df, &cfg, "host", &h_read), "{:?}", df.fields);
    assert!(df.fields.iter().any(|(i, f, _)| i == &cfg && f == "port"), "{:?}", df.fields);
    // `.host` is a member read carrying the field name.
    let member = dnode(&df, "member", "host");
    assert!(df.edges.iter().any(|e| e.to == member.id), "member has a base edge");
    // tuple-struct ctor arg at slot 0.
    let x_reads: Vec<&DfNode> = df.nodes.iter().filter(|n| n.kind == "var_read" && n.var == "x").collect();
    assert!(x_reads.iter().any(|x| has_arg(&df, &wrap, 0, &x.id)), "{:?}", df.args);
    // method receiver at slot -1: items.len().
    let items_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "items").unwrap();
    assert!(df.args.iter().any(|(_, p, a)| *p == -1 && a == &items_read.id), "{:?}", df.args);
    // eat(n, x): slots 0 and 1 on the same call.
    let n_read = df.nodes.iter().find(|n| n.kind == "var_read" && n.var == "n").unwrap();
    let eat_call = df.args.iter().find(|(_, p, a)| *p == 0 && a == &n_read.id).map(|(c, _, _)| c.clone())
        .expect("eat call with n at slot 0");
    assert!(df.args.iter().any(|(c, p, a)| c == &eat_call && *p == 1
        && x_reads.iter().any(|x| a == &x.id)), "{:?}", df.args);
}

#[test]
fn rust_inline_closure_lifts_as_own_scope() {
    let src = "fn go(xs: Vec<i32>) {\n    let out = xs.map(|x| x + 1);\n}\n";
    let df = RustTypes.extract_dataflow("f.rs", src);
    assert_lambda_lifted(&df, 0, "x");
    // capture still resolves: the shared scope links an outer read.
    let src2 = "fn go(k: i32, xs: Vec<i32>) {\n    let out = xs.map(|x| x + k);\n}\n";
    let df2 = RustTypes.extract_dataflow("f.rs", src2);
    let k_param = df2.nodes.iter().find(|n| n.kind == "param" && n.var == "k").unwrap();
    let k_read = df2.nodes.iter().find(|n| n.kind == "var_read" && n.var == "k").unwrap();
    assert!(
        df2.edges.iter().any(|e| e.from == k_param.id && e.to == k_read.id),
        "capture edge: {:?}", df2.edges
    );
}

#[test]
fn rust_const_str_mints_entity_and_df_lit() {
    let src = "const HOME: &str = \"/home\";\nfn go() { let _ = HOME; }\n";
    let facts = RustTypes.extract("f.rs", src);
    let ent = facts.entities.iter().find(|e| e.name == "HOME").expect("const entity");
    assert_eq!(ent.kind, EntityKind::Const);
    let row = facts.consts.iter().find(|c| c.sym == ent.sym).expect("const_value row");
    assert_eq!(row.text, "/home");
    assert_eq!(row.kind, "lit");

    let df = RustTypes.extract_dataflow("f.rs", "fn go() { let x = \"/home\"; }\n");
    assert!(df.lits.iter().any(|(_, text, kind)| text == "/home" && *kind == "lit"), "{:?}", df.lits);
}

#[test]
fn rust_bundle_matches_independent_extractors_and_honors_mask() {
    let src = r#"
        /// Increment a value.
        pub fn inc(input: i64) -> i64 {
            let next = input + 1;
            next
        }
    "#;
    let all = RustTypes.extract_bundle("f.rs", src, AnalysisMask::ALL);
    assert_eq!(all.types, Some(RustTypes.extract("f.rs", src)));
    assert_eq!(all.calls, Some(RustTypes.extract_calls("f.rs", src)));
    assert_eq!(all.dataflow, Some(RustTypes.extract_dataflow("f.rs", src)));

    let types_only = RustTypes.extract_bundle(
        "f.rs",
        src,
        AnalysisMask { types: true, calls: false, dataflow: false },
    );
    assert!(types_only.types.is_some());
    assert!(types_only.calls.is_none());
    assert!(types_only.dataflow.is_none());
}
