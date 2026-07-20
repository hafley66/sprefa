use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};

use ena::unify::{EqUnifyValue, InPlaceUnificationTable, UnifyKey};
use la_arena::{Arena, Idx};
use lasso::{Rodeo, Spur};
use miette::{miette, Result};
use serde_json::{json, Value};

struct MeasuringAllocator;

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: MeasuringAllocator = MeasuringAllocator;

unsafe impl GlobalAlloc for MeasuringAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc(layout);
        if !pointer.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            let live = LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        System.dealloc(pointer, layout);
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let result = System.realloc(pointer, layout, new_size);
        if !result.is_null() {
            if new_size >= layout.size() {
                let delta = new_size - layout.size();
                let live = LIVE_BYTES.fetch_add(delta, Ordering::Relaxed) + delta;
                PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
            } else {
                LIVE_BYTES.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        result
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TypeId(u32);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DeclId(u32);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum TypeNode {
    String,
    Int,
    Bool,
    Any,
    Param(Spur),
    Record(DeclId),
    Array(TypeId),
    Map(TypeId),
    Optional(TypeId),
    Union(Vec<TypeId>),
    Apply {
        constructor: DeclId,
        args: Vec<TypeId>,
    },
}

#[derive(Clone, Debug)]
struct Field {
    name: Spur,
    ty: TypeId,
}

#[derive(Clone, Debug)]
struct Decl {
    name: Spur,
    params: Vec<Spur>,
    fields: Vec<Field>,
}

#[derive(Default)]
struct TypeStore {
    names: Rodeo,
    decls: Arena<Decl>,
    nodes: Vec<TypeNode>,
    interned: HashMap<TypeNode, TypeId>,
    declarations: BTreeMap<String, DeclId>,
}

impl TypeStore {
    fn node(&mut self, node: TypeNode) -> TypeId {
        if let Some(id) = self.interned.get(&node) {
            return *id;
        }
        let id = TypeId(self.nodes.len() as u32);
        self.interned.insert(node.clone(), id);
        self.nodes.push(node);
        id
    }

    fn name(&mut self, value: &str) -> Spur {
        self.names.get_or_intern(value)
    }

    fn decl(&mut self, name: &str, params: &[&str]) -> DeclId {
        let name_id = self.name(name);
        let param_ids = params.iter().map(|param| self.name(param)).collect();
        let id = self.decls.alloc(Decl {
            name: name_id,
            params: param_ids,
            fields: Vec::new(),
        });
        let id = DeclId(id.into_raw().into());
        self.declarations.insert(name.to_string(), id);
        id
    }

    fn primitive(&mut self, name: &str, params: &[Spur]) -> Result<TypeId> {
        Ok(match name {
            "string" => self.node(TypeNode::String),
            "int" => self.node(TypeNode::Int),
            "bool" => self.node(TypeNode::Bool),
            "any" => self.node(TypeNode::Any),
            _ if params.iter().any(|param| self.names.resolve(param) == name) => {
                let param = self.name(name);
                self.node(TypeNode::Param(param))
            }
            _ => {
                let decl = *self
                    .declarations
                    .get(name)
                    .ok_or_else(|| miette!("unknown type {name}"))?;
                self.node(TypeNode::Record(decl))
            }
        })
    }

    fn parse_expr(&mut self, value: &Value, params: &[Spur]) -> Result<TypeId> {
        if let Some(name) = value.as_str() {
            return self.primitive(name, params);
        }
        let object = value
            .as_object()
            .ok_or_else(|| miette!("type expression must be a string or object"))?;
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| miette!("type expression is missing type"))?;
        match kind {
            "array" => self
                .parse_expr(&object["items"], params)
                .map(|ty| self.node(TypeNode::Array(ty))),
            "map" => self
                .parse_expr(&object["values"], params)
                .map(|ty| self.node(TypeNode::Map(ty))),
            "optional" => self
                .parse_expr(&object["inner"], params)
                .map(|ty| self.node(TypeNode::Optional(ty))),
            "union" => object["members"]
                .as_array()
                .ok_or_else(|| miette!("union members must be an array"))?
                .iter()
                .map(|member| self.parse_expr(member, params))
                .collect::<Result<Vec<_>>>()
                .map(|mut members| {
                    members.sort_unstable_by_key(|id| id.0);
                    members.dedup();
                    self.node(TypeNode::Union(members))
                }),
            "ref" => self.primitive(object["name"].as_str().unwrap_or_default(), params),
            "apply" => {
                let constructor = object["constructor"]
                    .as_str()
                    .and_then(|name| self.declarations.get(name).copied())
                    .ok_or_else(|| miette!("unknown type constructor"))?;
                let args = object["args"]
                    .as_array()
                    .ok_or_else(|| miette!("application args must be an array"))?
                    .iter()
                    .map(|arg| self.parse_expr(arg, params))
                    .collect::<Result<Vec<_>>>()?;
                Ok(self.node(TypeNode::Apply { constructor, args }))
            }
            other => Err(miette!("unknown type expression {other}")),
        }
    }

    fn fields(&self, ty: TypeId) -> Vec<(String, TypeId)> {
        let Some(TypeNode::Record(decl)) = self.nodes.get(ty.0 as usize) else {
            return Vec::new();
        };
        let decl = &self.decls[Idx::from_raw(decl.0.into())];
        decl.fields
            .iter()
            .map(|field| (self.names.resolve(&field.name).to_string(), field.ty))
            .collect()
    }

    fn paths(&self, ty: TypeId) -> Vec<(String, TypeId)> {
        let mut result = Vec::new();
        self.walk_paths(ty, String::new(), &mut result);
        result
    }

    fn walk_paths(&self, ty: TypeId, prefix: String, result: &mut Vec<(String, TypeId)>) {
        match self.nodes.get(ty.0 as usize) {
            Some(TypeNode::Record(_)) => {
                for (name, child) in self.fields(ty) {
                    let path = if prefix.is_empty() {
                        name
                    } else {
                        format!("{prefix}.{name}")
                    };
                    self.walk_paths(child, path, result);
                }
            }
            Some(TypeNode::Array(child)) => {
                self.walk_paths(*child, format!("{prefix}[*]"), result);
            }
            Some(TypeNode::Map(child)) => {
                self.walk_paths(*child, format!("{prefix}{{key}}"), result);
            }
            Some(TypeNode::Optional(child)) => self.walk_paths(*child, prefix, result),
            Some(TypeNode::Union(members)) => {
                for member in members {
                    self.walk_paths(*member, prefix.clone(), result);
                }
            }
            Some(TypeNode::Apply { constructor, args }) => {
                let decl = &self.decls[Idx::from_raw(constructor.0.into())];
                for field in &decl.fields {
                    let name = self.names.resolve(&field.name);
                    let path = if prefix.is_empty() {
                        name.to_string()
                    } else {
                        format!("{prefix}.{name}")
                    };
                    self.walk_paths_applied(field.ty, path, &decl.params, args, result);
                }
            }
            Some(_) => result.push((prefix, ty)),
            None => {}
        }
    }

    fn walk_paths_applied(
        &self,
        ty: TypeId,
        prefix: String,
        params: &[Spur],
        args: &[TypeId],
        result: &mut Vec<(String, TypeId)>,
    ) {
        match self.nodes.get(ty.0 as usize) {
            Some(TypeNode::Param(param)) => {
                if let Some(index) = params.iter().position(|candidate| candidate == param) {
                    if let Some(argument) = args.get(index) {
                        self.walk_paths(*argument, prefix, result);
                    }
                }
            }
            Some(TypeNode::Array(child)) => {
                self.walk_paths_applied(*child, format!("{prefix}[*]"), params, args, result);
            }
            Some(TypeNode::Map(child)) => {
                self.walk_paths_applied(*child, format!("{prefix}{{key}}"), params, args, result);
            }
            Some(TypeNode::Optional(child)) => {
                self.walk_paths_applied(*child, prefix, params, args, result);
            }
            _ => self.walk_paths(ty, prefix, result),
        }
    }

    fn graph_json(&self) -> Value {
        let declarations = self
            .decls
            .iter()
            .map(|(id, decl)| {
                let raw_id = id.into_raw().into_u32();
                json!({
                    "id": raw_id,
                    "name": self.names.resolve(&decl.name),
                    "params": decl.params.iter().map(|p| self.names.resolve(p)).collect::<Vec<_>>(),
                    "fields": decl.fields.iter().map(|field| json!({
                        "name": self.names.resolve(&field.name),
                        "type": field.ty.0,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>();
        let nodes = self
            .nodes
            .iter()
            .enumerate()
            .map(|(id, node)| json!({ "id": id, "node": format!("{node:?}") }))
            .collect::<Vec<_>>();
        json!({ "nodes": nodes, "declarations": declarations })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Var(u32);

impl UnifyKey for Var {
    type Value = Option<TypeId>;

    fn index(&self) -> u32 {
        self.0
    }
    fn from_index(index: u32) -> Self {
        Self(index)
    }
    fn tag() -> &'static str {
        "type_var"
    }
}

impl EqUnifyValue for TypeId {}

fn build_store() -> Result<TypeStore> {
    let source = json!({
        "User": {
            "fields": {
                "id": "string",
                "profile": "Profile",
                "orders": {"type": "array", "items": "Order"},
                "metadata": {"type": "map", "values": "string"}
            }
        },
        "Profile": {
            "fields": {
                "avatar": {"type": "optional", "inner": "string"},
                "tags": {"type": "array", "items": "string"}
            }
        },
        "Order": {
            "fields": {
                "id": "string",
                "total": "int",
                "states": {"type": "array", "items": {
                    "type": "union",
                    "members": ["string", "int"]
                }}
            }
        },
        "Page": {
            "params": ["T"],
            "fields": {
                "items": {"type": "array", "items": "T"},
                "next": {"type": "optional", "inner": "string"}
            }
        }
    });
    let mut store = TypeStore::default();
    let definitions = source.as_object().unwrap();
    for (name, definition) in definitions {
        let params = definition["params"]
            .as_array()
            .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>())
            .unwrap_or_default();
        store.decl(name, &params);
    }
    for (name, definition) in definitions {
        let decl = store.declarations[name];
        let params = store.decls[Idx::from_raw(decl.0.into())].params.clone();
        let fields = definition["fields"].as_object().unwrap();
        let fields = fields
            .iter()
            .map(|(field, ty)| {
                Ok(Field {
                    name: store.name(field),
                    ty: store.parse_expr(ty, &params)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        store.decls[Idx::from_raw(decl.0.into())].fields = fields;
    }
    Ok(store)
}

fn stress(count: usize) -> Result<()> {
    let mut store = TypeStore::default();
    let string = store.node(TypeNode::String);
    let mut last = string;
    for index in 0..count {
        let name = format!("Record{index}");
        let decl = store.decl(&name, &[]);
        store.decls[Idx::from_raw(decl.0.into())].fields = vec![
            Field {
                name: store.name("value"),
                ty: last,
            },
            Field {
                name: store.name("items"),
                ty: store.node(TypeNode::Array(last)),
            },
            Field {
                name: store.name("metadata"),
                ty: store.node(TypeNode::Map(string)),
            },
        ];
        last = store.node(TypeNode::Record(decl));
    }
    println!("stress.count={count}");
    println!("stress.declarations={}", store.decls.len());
    println!("stress.nodes={}", store.nodes.len());
    println!("stress.interned_names={}", store.names.len());
    println!("stress.deepest_type_id={}", last.0);
    println!("stress.allocations={}", ALLOCATIONS.load(Ordering::Relaxed));
    println!(
        "stress.peak_allocated_bytes={}",
        PEAK_BYTES.load(Ordering::Relaxed)
    );
    Ok(())
}

fn demo() -> Result<()> {
    let mut store = build_store()?;
    let user = store.primitive("User", &[])?;
    let page = store.declarations["Page"];
    let page_of_user = TypeStore {
        nodes: store.nodes.clone(),
        interned: store.interned.clone(),
        ..store
    };
    let mut page_store = page_of_user;
    let page_of_user = page_store.node(TypeNode::Apply {
        constructor: page,
        args: vec![user],
    });
    let mut variables: InPlaceUnificationTable<Var> = InPlaceUnificationTable::new();
    let inferred = variables.new_key(None);
    variables
        .unify_var_value(inferred, Some(page_store.primitive("string", &[])?))
        .map_err(|error| miette!("unification failed: {error:?}"))?;

    println!("TYPE GRAPH");
    println!(
        "{}",
        serde_json::to_string_pretty(&page_store.graph_json()).unwrap()
    );
    println!("USER PATHS");
    for (path, ty) in page_store.paths(user) {
        println!("{path} => TypeId({})", ty.0);
    }
    println!("PAGE<USER> PATHS");
    for (path, ty) in page_store.paths(page_of_user) {
        println!("{path} => TypeId({})", ty.0);
    }
    println!("UNIFICATION");
    println!(
        "T unified with string => {:?}",
        variables.probe_value(inferred)
    );
    println!("DIAGNOSTIC");
    println!("{:?}", miette!("field missing does not exist on User"));
    Ok(())
}

fn main() -> Result<()> {
    let args = env::args().collect::<Vec<_>>();
    if args.get(1).map(String::as_str) == Some("--stress") {
        let count = args
            .get(2)
            .and_then(|value| value.parse().ok())
            .unwrap_or(100_000);
        stress(count)
    } else {
        demo()
    }
}
