use std::collections::{HashMap, HashSet};

use facet::{Def, Facet, Type, UserType};
use serde_json::json;

#[derive(Facet)]
struct User {
    id: String,
    profile: Profile,
    orders: Vec<Order>,
    metadata: HashMap<String, String>,
}

#[derive(Facet)]
struct Profile {
    avatar: Option<String>,
    tags: Vec<String>,
}

#[derive(Facet)]
struct Order {
    id: String,
    total: i64,
}

#[derive(Facet)]
struct Page<T> {
    items: Vec<T>,
    next: Option<String>,
}

#[derive(Debug, Clone)]
enum Fact {
    Type { name: String, kind: String },
    Field { owner: String, name: String, ty: String },
    Path { owner: String, path: String, ty: String },
    Collection { owner: String, path: String, kind: String, ty: String },
}

fn type_name(shape: &'static facet::Shape) -> String {
    shape.type_name().to_string()
}

fn type_kind(shape: &'static facet::Shape) -> &'static str {
    match shape.ty {
        Type::User(UserType::Struct(_)) => "record",
        Type::User(UserType::Enum(_)) => "enum",
        Type::User(UserType::Union(_)) => "union",
        Type::User(UserType::Opaque) => "opaque",
        Type::Primitive(_) => "primitive",
        Type::Sequence(_) => "sequence",
        Type::Pointer(_) => "pointer",
        Type::Undefined => "undefined",
    }
}

fn lower(root: &'static facet::Shape) -> Vec<Fact> {
    let mut facts = Vec::new();
    let mut seen_types = HashSet::new();
    lower_shape(root, &mut facts, &mut seen_types);
    let root_name = type_name(root);
    lower_paths(root, &root_name, "", &mut facts, 0);
    facts
}

fn lower_shape(
    shape: &'static facet::Shape,
    facts: &mut Vec<Fact>,
    seen_types: &mut HashSet<String>,
) {
    let name = type_name(shape);
    if !seen_types.insert(name.clone()) {
        return;
    }

    facts.push(Fact::Type {
        name,
        kind: type_kind(shape).to_owned(),
    });

    match shape.def {
        Def::Map(map) => {
            lower_shape(map.k, facts, seen_types);
            lower_shape(map.v, facts, seen_types);
        }
        Def::List(list) => lower_shape(list.t, facts, seen_types),
        Def::Array(array) => lower_shape(array.t, facts, seen_types),
        Def::Option(option) => lower_shape(option.t, facts, seen_types),
        Def::Result(result) => {
            lower_shape(result.t, facts, seen_types);
            lower_shape(result.e, facts, seen_types);
        }
        Def::Set(set) => lower_shape(set.t, facts, seen_types),
        Def::Slice(slice) => lower_shape(slice.t, facts, seen_types),
        Def::NdArray(array) => lower_shape(array.t, facts, seen_types),
        Def::Pointer(pointer) => {
            if let Some(pointee) = pointer.pointee {
                lower_shape(pointee, facts, seen_types);
            }
        }
        _ => {}
    }

    if let Type::User(UserType::Struct(struct_type)) = shape.ty {
        for field in struct_type.fields {
            let field_shape = field.shape.get();
            facts.push(Fact::Field {
                owner: type_name(shape),
                name: field.name.to_owned(),
                ty: type_name(field_shape),
            });
            lower_shape(field_shape, facts, seen_types);
        }
    }
}

fn lower_paths(
    shape: &'static facet::Shape,
    root_name: &str,
    prefix: &str,
    facts: &mut Vec<Fact>,
    depth: usize,
) {
    if depth > 32 {
        return;
    }

    match shape.def {
        Def::Option(option) => lower_paths(option.t, root_name, prefix, facts, depth + 1),
        Def::List(list) => {
            facts.push(Fact::Collection {
                owner: root_name.to_owned(),
                path: prefix.to_owned(),
                kind: "array".to_owned(),
                ty: type_name(list.t),
            });
            lower_paths(list.t, root_name, &format!("{prefix}[*]"), facts, depth + 1);
        }
        Def::Array(array) => {
            facts.push(Fact::Collection {
                owner: root_name.to_owned(),
                path: prefix.to_owned(),
                kind: format!("array[{}]", array.n),
                ty: type_name(array.t),
            });
            lower_paths(array.t, root_name, &format!("{prefix}[*]"), facts, depth + 1);
        }
        Def::Map(map) => {
            facts.push(Fact::Collection {
                owner: root_name.to_owned(),
                path: prefix.to_owned(),
                kind: "map".to_owned(),
                ty: type_name(map.v),
            });
            lower_paths(map.v, root_name, &format!("{prefix}{{key}}"), facts, depth + 1);
        }
        Def::Set(set) => lower_paths(set.t, root_name, &format!("{prefix}[*]"), facts, depth + 1),
        Def::Slice(slice) => lower_paths(slice.t, root_name, &format!("{prefix}[*]"), facts, depth + 1),
        Def::NdArray(array) => lower_paths(array.t, root_name, &format!("{prefix}[*]"), facts, depth + 1),
        Def::Pointer(pointer) => {
            if let Some(pointee) = pointer.pointee {
                lower_paths(pointee, root_name, prefix, facts, depth + 1);
            }
        }
        Def::Result(result) => {
            lower_paths(result.t, root_name, prefix, facts, depth + 1);
            lower_paths(result.e, root_name, prefix, facts, depth + 1);
        }
        _ => {
            if !prefix.is_empty() {
                facts.push(Fact::Path {
                    owner: root_name.to_owned(),
                    path: prefix.to_owned(),
                    ty: type_name(shape),
                });
            }
        }
    }

    if let Type::User(UserType::Struct(struct_type)) = shape.ty {
        for field in struct_type.fields {
            let field_shape = field.shape.get();
            let path = if prefix.is_empty() {
                field.name.to_owned()
            } else {
                format!("{prefix}.{}", field.name)
            };
            lower_paths(field_shape, root_name, &path, facts, depth + 1);
        }
    }
}

fn path_facts<'a>(facts: &'a [Fact], needle: &str) -> Vec<&'a Fact> {
    facts
        .iter()
        .filter(|fact| matches!(fact, Fact::Path { path, .. } if path.contains(needle)))
        .collect()
}

fn fact_json(fact: &Fact) -> serde_json::Value {
    match fact {
        Fact::Type { name, kind } => json!({"type": name, "kind": kind}),
        Fact::Field { owner, name, ty } => json!({"owner": owner, "field": name, "ty": ty}),
        Fact::Path { owner, path, ty } => json!({"owner": owner, "path": path, "ty": ty}),
        Fact::Collection { owner, path, kind, ty } => {
            json!({"owner": owner, "path": path, "kind": kind, "ty": ty})
        }
    }
}

fn main() {
    let user_facts = lower(User::SHAPE);
    let page_facts = lower(Page::<User>::SHAPE);

    println!("== Facet shapes ==");
    println!("root: {}", type_name(User::SHAPE));
    println!("generic application: {}", type_name(Page::<User>::SHAPE));

    println!("\n== relation facts ==");
    for fact in user_facts.iter().filter(|fact| {
        matches!(fact, Fact::Type { .. } | Fact::Field { .. } | Fact::Collection { .. })
    }) {
        println!("{fact:?}");
    }

    println!("\n== path query: array element ids ==");
    for fact in path_facts(&user_facts, "orders[*].id") {
        println!("{fact:?}");
    }

    println!("\n== path query: map values ==");
    for fact in path_facts(&user_facts, "metadata{key}") {
        println!("{fact:?}");
    }

    let json_graph = json!({
        "user": user_facts.iter().map(fact_json).collect::<Vec<_>>(),
        "page_user": page_facts.iter().map(fact_json).collect::<Vec<_>>(),
    });
    println!("\n== json graph ==");
    println!("{}", serde_json::to_string_pretty(&json_graph).unwrap());
}
