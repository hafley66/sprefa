//! JSON transport for programs and closures. Terms: int as number, atom as
//! `{"a": name}`, string as `{"s": text}`, proper list as array, compound as
//! `{"f": name, "args": [...]}`. Rule arguments add `{"v": identity}` and an
//! aggregate key, one of `{"count": arg}`, `{"sum": arg}`, `{"min": arg}`,
//! `{"max": arg}`, or `{"f":"fold","args":[step,seed,arg]}`. A program
//! carries an optional `names` table beside its rules and seeds, which
//! `--serve` resolves against. The oracle dump
//! `v8/oracle/eval/dump_eval.pl` writes the same shape from v7.

use super::program::{
    AggregateKind, Arg, Diagnostic, Fold, Goal, Order, Polarity, Program, Row, Rule, Seed, VarId,
};
use super::term::{Sym, Term, TermId, Universe};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};

/// SWI's JSON writer turns the atoms `null`, `true` and `false` into JSON
/// literals; read them back as atoms.
fn atom_name(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => Some("null".into()),
        _ => None,
    }
}

/// Post-order with an explicit stack: lists and compounds intern after their
/// items, left to right; the first bad item reports.
pub fn term_from_json(u: &mut Universe, v: &Value) -> Result<TermId, String> {
    enum Step<'v> {
        Enter(&'v Value),
        List(usize),
        Compound(&'v str, usize),
    }
    let mut steps = vec![Step::Enter(v)];
    let mut done: Vec<TermId> = Vec::new();
    while let Some(step) = steps.pop() {
        let v = match step {
            Step::Enter(v) => v,
            Step::List(len) => {
                let ids = done.split_off(done.len() - len);
                done.push(u.list(&ids));
                continue;
            }
            Step::Compound(f, len) => {
                let ids = done.split_off(done.len() - len);
                done.push(u.compound(f, ids));
                continue;
            }
        };
        let id = match v {
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    u.int(i)
                } else if let Some(f) = n.as_f64() {
                    u.float(f)
                } else {
                    return Err(format!("non-numeric number {n}"));
                }
            }
            Value::Bool(b) => u.boolean(*b),
            Value::Array(items) => {
                steps.push(Step::List(items.len()));
                steps.extend(items.iter().rev().map(Step::Enter));
                continue;
            }
            Value::Object(m) => {
                if let Some(a) = m.get("a").and_then(atom_name) {
                    u.atom(&a)
                } else if let Some(Value::String(s)) = m.get("s") {
                    u.string(s)
                } else if let (Some(Value::String(f)), Some(Value::Array(args))) =
                    (m.get("f"), m.get("args"))
                {
                    steps.push(Step::Compound(f, args.len()));
                    steps.extend(args.iter().rev().map(Step::Enter));
                    continue;
                } else {
                    return Err(format!("unknown term object {v}"));
                }
            }
            _ => return Err(format!("unknown term {v}")),
        };
        done.push(id);
    }
    Ok(done.pop().expect("one term"))
}

/// An arbitrary document, not the transport encoding above. `serde_json::Value`
/// stores an object in a `BTreeMap` and cannot carry the order a row index is.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

struct JsonVisitor;

impl<'de> serde::de::Visitor<'de> for JsonVisitor {
    type Value = Json;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a json value")
    }

    fn visit_unit<E>(self) -> Result<Json, E> {
        Ok(Json::Null)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Json, E> {
        Ok(Json::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Json, E> {
        Ok(Json::Int(value))
    }

    /// A magnitude past `i64` keeps its value as a float, the split
    /// `term_from_json` makes.
    fn visit_u64<E>(self, value: u64) -> Result<Json, E> {
        Ok(match i64::try_from(value) {
            Ok(value) => Json::Int(value),
            Err(_) => Json::Float(value as f64),
        })
    }

    fn visit_f64<E>(self, value: f64) -> Result<Json, E> {
        Ok(Json::Float(value))
    }

    fn visit_str<E>(self, value: &str) -> Result<Json, E> {
        Ok(Json::Text(value.to_string()))
    }

    fn visit_string<E>(self, value: String) -> Result<Json, E> {
        Ok(Json::Text(value))
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut access: A) -> Result<Json, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = access.next_element()? {
            items.push(item);
        }
        Ok(Json::Array(items))
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut access: A) -> Result<Json, A::Error> {
        let mut members = Vec::new();
        while let Some((name, value)) = access.next_entry()? {
            members.push((name, value));
        }
        Ok(Json::Object(members))
    }
}

impl<'de> serde::Deserialize<'de> for Json {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Json, D::Error> {
        d.deserialize_any(JsonVisitor)
    }
}

/// `ref(application(primitive(Name), [Value]))`: the `intern` of one primitive,
/// the node `(str "x")` names, `kernel::intern_row` incarnate.
fn value_node(u: &mut Universe, primitive: &str, value: TermId) -> TermId {
    let name = u.atom(primitive);
    let constructor = u.compound("primitive", vec![name]);
    let arguments = u.list(&[value]);
    let application = u.compound("application", vec![constructor, arguments]);
    u.compound("ref", vec![application])
}

/// `identity` is `edge(Owner, Label)`, `kernel::edge_ref_row`'s own shape, so a
/// member is reachable by `edge_ref`. serde_json's 128-level cap bounds this.
fn walk(
    u: &mut Universe,
    colon: TermId,
    none: TermId,
    identity: TermId,
    json: &Json,
    rows: &mut Vec<Row>,
) -> TermId {
    /// An open container; `returned` carries the value of the child it entered last.
    enum Frame<'j> {
        Enter(TermId, &'j Json),
        Array {
            identity: TermId,
            items: &'j [Json],
            cells: Vec<TermId>,
        },
        Object {
            identity: TermId,
            owner: TermId,
            members: &'j [(String, Json)],
            next: usize,
            label: TermId,
        },
    }
    let mut stack = vec![Frame::Enter(identity, json)];
    let mut returned: Option<TermId> = None;
    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Enter(identity, json) => {
                returned = Some(match json {
                    Json::Null => none,
                    Json::Bool(value) => {
                        let value = u.boolean(*value);
                        value_node(u, "bool", value)
                    }
                    Json::Int(value) => {
                        let value = u.int(*value);
                        value_node(u, "int", value)
                    }
                    Json::Float(value) => {
                        let value = u.float(*value);
                        value_node(u, "float", value)
                    }
                    Json::Text(value) => {
                        let value = u.string(value);
                        value_node(u, "str", value)
                    }
                    Json::Array(items) => {
                        stack.push(Frame::Array {
                            identity,
                            items,
                            cells: Vec::with_capacity(items.len()),
                        });
                        continue;
                    }
                    Json::Object(members) => {
                        let owner = u.compound("ref", vec![identity]);
                        stack.push(Frame::Object {
                            identity,
                            owner,
                            members,
                            next: 0,
                            label: none,
                        });
                        continue;
                    }
                });
            }
            Frame::Array {
                identity,
                items,
                mut cells,
            } => {
                cells.extend(returned.take());
                let Some(item) = items.get(cells.len()) else {
                    let list = u.list(&cells);
                    returned = Some(u.compound("const", vec![list]));
                    continue;
                };
                let label = u.int(cells.len() as i64);
                let child = u.compound("edge", vec![identity, label]);
                stack.push(Frame::Array {
                    identity,
                    items,
                    cells,
                });
                stack.push(Frame::Enter(child, item));
            }
            Frame::Object {
                identity,
                owner,
                members,
                mut next,
                label,
            } => {
                if let Some(target) = returned.take() {
                    let name = u.compound("const", vec![label]);
                    let index = u.int(next as i64);
                    let index = u.compound("const", vec![index]);
                    rows.push(Row {
                        rel: colon,
                        args: vec![owner, name, target, index],
                    });
                    next += 1;
                }
                let Some((name, value)) = members.get(next) else {
                    returned = Some(owner);
                    continue;
                };
                let label = u.atom(name);
                let child = u.compound("edge", vec![identity, label]);
                stack.push(Frame::Object {
                    identity,
                    owner,
                    members,
                    next,
                    label,
                });
                stack.push(Frame::Enter(child, value));
            }
        }
    }
    returned.expect("the root value")
}

/// One document as `:` rows under `identity`, with the cell its root binds to.
/// Keys keep document order as the row index; nothing is sorted.
pub fn rows_from_json(
    u: &mut Universe,
    colon: TermId,
    none: TermId,
    identity: TermId,
    document: &Json,
) -> (TermId, Vec<Row>) {
    let mut rows = Vec::new();
    let root = walk(u, colon, none, identity, document, &mut rows);
    (root, rows)
}

/// Post-order with an explicit stack: proper lists become arrays, compounds
/// `{"f", "args"}`, items left to right.
pub fn term_to_json(u: &Universe, id: TermId) -> Value {
    enum Step {
        Enter(TermId),
        Array(usize),
        Compound(Sym, usize),
    }
    let mut steps = vec![Step::Enter(id)];
    let mut done: Vec<Value> = Vec::new();
    while let Some(step) = steps.pop() {
        let id = match step {
            Step::Enter(id) => id,
            Step::Array(len) => {
                let items = done.split_off(done.len() - len);
                done.push(Value::Array(items));
                continue;
            }
            Step::Compound(s, len) => {
                let args = done.split_off(done.len() - len);
                done.push(json!({ "f": u.sym_str(s), "args": args }));
                continue;
            }
        };
        if let Some(items) = u.as_list(id) {
            steps.push(Step::Array(items.len()));
            steps.extend(items.iter().rev().map(|item| Step::Enter(*item)));
            continue;
        }
        done.push(match u.get(id) {
            Term::Int(n) => json!(n),
            Term::Float(x) => json!(x.0),
            Term::Bool(b) => json!(b),
            Term::Atom(s) => json!({ "a": u.sym_str(*s) }),
            Term::Str(s) => json!({ "s": u.sym_str(*s) }),
            Term::Compound(s, args) => {
                steps.push(Step::Compound(*s, args.len()));
                steps.extend(args.iter().rev().map(|arg| Step::Enter(*arg)));
                continue;
            }
        });
    }
    done.pop().expect("one value")
}

/// `term_to_json`'s bytes, written as the walk goes: no `Value` tree, so a shared
/// subterm costs its text once per use and no node per use.
pub fn write_term_json(u: &Universe, id: TermId, w: &mut impl std::io::Write) -> std::io::Result<()> {
    enum Step {
        Enter(TermId),
        Text(&'static str),
        Name(Sym),
    }
    let mut steps = vec![Step::Enter(id)];
    while let Some(step) = steps.pop() {
        let id = match step {
            Step::Enter(id) => id,
            Step::Text(text) => {
                w.write_all(text.as_bytes())?;
                continue;
            }
            Step::Name(s) => {
                serde_json::to_writer(&mut *w, u.sym_str(s))?;
                w.write_all(b"}")?;
                continue;
            }
        };
        let (open, close, items, name) = match (u.as_list(id), u.get(id)) {
            (Some(items), _) => ("[", Step::Text("]"), items, None),
            (None, Term::Compound(s, args)) => ("{\"args\":[", Step::Text("],\"f\":"), args.to_vec(), Some(*s)),
            (None, Term::Int(n)) => {
                serde_json::to_writer(&mut *w, n)?;
                continue;
            }
            (None, Term::Float(x)) => {
                serde_json::to_writer(&mut *w, &x.0)?;
                continue;
            }
            (None, Term::Bool(b)) => {
                serde_json::to_writer(&mut *w, b)?;
                continue;
            }
            (None, Term::Atom(s)) => {
                w.write_all(b"{\"a\":")?;
                serde_json::to_writer(&mut *w, u.sym_str(*s))?;
                w.write_all(b"}")?;
                continue;
            }
            (None, Term::Str(s)) => {
                w.write_all(b"{\"s\":")?;
                serde_json::to_writer(&mut *w, u.sym_str(*s))?;
                w.write_all(b"}")?;
                continue;
            }
        };
        w.write_all(open.as_bytes())?;
        steps.extend(name.map(Step::Name));
        steps.push(close);
        for (index, item) in items.iter().enumerate().rev() {
            steps.push(Step::Enter(*item));
            if index > 0 {
                steps.push(Step::Text(","));
            }
        }
    }
    Ok(())
}

/// One aggregate or fold wrapper around a rule argument, outermost first.
enum ArgLayer {
    Aggregate(AggregateKind),
    Fold(Fold),
}

fn arg_from_json(u: &mut Universe, v: &Value, vars: &mut Vec<TermId>) -> Result<Arg, String> {
    let mut layers: Vec<ArgLayer> = Vec::new();
    let mut pending = vec![v];
    let mut base = None;
    while let Some(v) = pending.pop() {
        if let Value::Object(m) = v {
            if let Some(identity) = m.get("v") {
                let id = term_from_json(u, identity)?;
                let pos = match vars.iter().position(|x| *x == id) {
                    Some(p) => p,
                    None => {
                        vars.push(id);
                        vars.len() - 1
                    }
                };
                base = Some(Arg::Var(VarId(pos as u32)));
                continue;
            }
            if let Some((aggregation, subject)) = m
                .iter()
                .find_map(|(name, subject)| Some((AggregateKind::of(name)?, subject)))
            {
                layers.push(ArgLayer::Aggregate(aggregation));
                pending.push(subject);
                continue;
            }
            if m.get("f").and_then(Value::as_str) == Some("fold") {
                let args = m
                    .get("args")
                    .and_then(Value::as_array)
                    .ok_or("fold without args")?;
                let [step, seed, subject] = args.as_slice() else {
                    return Err(format!("bad fold arity {}", args.len()));
                };
                layers.push(ArgLayer::Fold(Fold {
                    step: term_from_json(u, step)?,
                    seed: Seed::Term(term_from_json(u, seed)?),
                    order: Order::TermLt,
                }));
                pending.push(subject);
                continue;
            }
        }
        base = Some(Arg::Ground(term_from_json(u, v)?));
    }
    let base = base.expect("the innermost argument");
    Ok(layers.into_iter().rev().fold(base, |inner, layer| match layer {
        ArgLayer::Aggregate(kind) => Arg::Aggregate(kind, Box::new(inner)),
        ArgLayer::Fold(fold) => Arg::Fold(fold, Box::new(inner)),
    }))
}

fn call_parts(v: &Value) -> Result<(&Value, &Vec<Value>), String> {
    let m = v.as_object().ok_or("call is not an object")?;
    let rel = m.get("rel").ok_or("call without rel")?;
    let args = m
        .get("args")
        .and_then(|a| a.as_array())
        .ok_or("call without args")?;
    Ok((rel, args))
}

/// A `checked_datalog/4` term that cannot be transported.
enum Transport {
    Shape(&'static str),
    DuplicateName {
        name: String,
        first: TermId,
        second: TermId,
    },
}

/// `term_to_json`'s text appended to an in-memory buffer.
/// A writer that keeps the first io error and drops every write after it, so the
/// program walk reports a transport error or an io error, never half of each.
struct Out<'w> {
    w: &'w mut dyn std::io::Write,
    error: Option<std::io::Error>,
}

impl Out<'_> {
    fn push(&mut self, byte: u8) {
        self.extend_from_slice(&[byte]);
    }

    fn extend_from_slice(&mut self, bytes: &[u8]) {
        if self.error.is_none() {
            self.error = self.w.write_all(bytes).err();
        }
    }
}

impl std::io::Write for Out<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn term_text(u: &Universe, id: TermId, w: &mut Out) {
    write_term_json(u, id, w).expect("Out keeps its error");
}

fn str_text(text: &str, w: &mut Out) {
    serde_json::to_writer(&mut *w, text).expect("Out keeps its error");
}

/// Keys are written in sorted order, the order `serde_json::Map` prints.
fn arg_text(u: &Universe, id: TermId, w: &mut Out) {
    /// One wrapper around the argument inside it, outermost first.
    enum Layer<'u> {
        Aggregate(&'u str),
        Fold(TermId, TermId),
    }
    let mut layers: Vec<Layer> = Vec::new();
    let mut pending = vec![id];
    let mut base = None;
    while let Some(id) = pending.pop() {
        if let Some(identity) = u.unary(id, "var") {
            base = Some((true, identity));
            continue;
        }
        if let Some([kind, subject]) = u.args::<2>(id, "aggregate") {
            if let Term::Atom(s) = u.get(kind) {
                let name = u.sym_str(*s);
                if AggregateKind::of(name).is_some() {
                    layers.push(Layer::Aggregate(name));
                    pending.push(subject);
                    continue;
                }
            }
        }
        if let Some([step, seed, subject]) = u.args::<3>(id, "fold") {
            layers.push(Layer::Fold(step, seed));
            pending.push(subject);
            continue;
        }
        base = Some((false, id));
    }
    for layer in &layers {
        match layer {
            Layer::Aggregate(name) => {
                w.push(b'{');
                str_text(name, w);
                w.push(b':');
            }
            Layer::Fold(step, seed) => {
                w.extend_from_slice(b"{\"args\":[");
                term_text(u, *step, w);
                w.push(b',');
                term_text(u, *seed, w);
                w.push(b',');
            }
        }
    }
    match base.expect("the innermost argument") {
        (true, identity) => {
            w.extend_from_slice(b"{\"v\":");
            term_text(u, identity, w);
            w.push(b'}');
        }
        (false, id) => term_text(u, id, w),
    }
    for layer in layers.iter().rev() {
        w.extend_from_slice(match layer {
            Layer::Aggregate(_) => b"}".as_slice(),
            Layer::Fold(..) => b"],\"f\":\"fold\"}".as_slice(),
        });
    }
}

fn call_text(u: &Universe, id: TermId, polarity: Option<&str>, w: &mut Out) -> Result<(), Transport> {
    let [rel, args] = u
        .args::<2>(id, "call")
        .ok_or(Transport::Shape("bad_call"))?;
    let args = u.as_list(args).ok_or(Transport::Shape("bad_call"))?;
    w.extend_from_slice(b"{\"args\":[");
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            w.push(b',');
        }
        arg_text(u, *arg, w);
    }
    w.push(b']');
    if let Some(polarity) = polarity {
        w.extend_from_slice(b",\"polarity\":");
        str_text(polarity, w);
    }
    w.extend_from_slice(b",\"rel\":");
    term_text(u, rel, w);
    w.push(b'}');
    Ok(())
}

fn goal_text(u: &Universe, id: TermId, w: &mut Out) -> Result<(), Transport> {
    let [polarity, call] = u
        .args::<2>(id, "checked_goal")
        .ok_or(Transport::Shape("bad_goal"))?;
    let Term::Atom(s) = u.get(polarity) else {
        return Err(Transport::Shape("bad_goal"));
    };
    call_text(u, call, Some(u.sym_str(*s)), w)
}

fn rule_text(u: &Universe, id: TermId, w: &mut Out) -> Result<(), Transport> {
    let [head, body] = u
        .args::<2>(id, "rule")
        .ok_or(Transport::Shape("bad_rule"))?;
    let body = u.as_list(body).ok_or(Transport::Shape("bad_rule"))?;
    w.extend_from_slice(b"{\"body\":[");
    for (index, goal) in body.iter().enumerate() {
        if index > 0 {
            w.push(b',');
        }
        goal_text(u, *goal, w)?;
    }
    w.extend_from_slice(b"],\"head\":");
    call_text(u, head, None, w)?;
    w.push(b'}');
    Ok(())
}

/// Every `:(module(M), Name, ref(Relation), _)` bind in the root graph. A file
/// module shadows the prelude's bind of one name; two files disagreeing do not.
/// A `@std/<space>` member carries its namespace, so `--serve git.refs` names it.
fn names_text(u: &Universe, graph: TermId, w: &mut Out) -> Result<(), Transport> {
    let [_, edges] = u
        .args::<2>(graph, "root_graph")
        .ok_or(Transport::Shape("no_root_graph"))?;
    let edges = u.as_list(edges).ok_or(Transport::Shape("no_root_graph"))?;
    let mut out: BTreeMap<String, (bool, TermId)> = BTreeMap::new();
    for edge in edges {
        let Some([owner, name, relation, _]) = u.args::<4>(edge, ":") else {
            continue;
        };
        let Some(module) = u.unary(owner, "module") else {
            continue;
        };
        if u.unary(relation, "ref").is_none() {
            continue;
        }
        let Term::Atom(s) = u.get(name) else {
            continue;
        };
        let name = u.sym_str(*s).to_string();
        let name = match u.unary(module, "std").map(|space| u.get(space)) {
            Some(Term::Atom(space)) => format!("{}.{name}", u.sym_str(*space)),
            _ => name,
        };
        let prelude = matches!(u.get(module), Term::Atom(m) if u.sym_str(*m) == "prelude");
        let module_target = u
            .unary(relation, "ref")
            .and_then(|target| u.unary(target, "module"))
            .is_some();
        match out.get(&name) {
            Some((_, first)) if *first == relation => {}
            Some((_, first))
                if module_target
                    && u.unary(*first, "ref")
                        .and_then(|target| u.unary(target, "module"))
                        .is_some() => {}
            Some((true, _)) if !prelude => {
                out.insert(name, (prelude, relation));
            }
            Some((false, _)) if prelude => {}
            Some((_, first)) => {
                return Err(Transport::DuplicateName {
                    name,
                    first: *first,
                    second: relation,
                })
            }
            None => {
                out.insert(name, (prelude, relation));
            }
        }
    }
    w.push(b'{');
    for (index, (name, (_, relation))) in out.iter().enumerate() {
        if index > 0 {
            w.push(b',');
        }
        str_text(name, w);
        w.push(b':');
        term_text(u, *relation, w);
    }
    w.push(b'}');
    Ok(())
}

fn transport(u: &Universe, checked_datalog: TermId, w: &mut Out) -> Result<(), Transport> {
    let [graph, datalog, _, _] = u
        .args::<4>(checked_datalog, "checked_datalog")
        .ok_or(Transport::Shape("not_checked_datalog"))?;
    let [_, seeds, rules] = u
        .args::<3>(datalog, "datalog_program")
        .ok_or(Transport::Shape("no_datalog_program"))?;
    let seeds = u
        .as_list(seeds)
        .ok_or(Transport::Shape("no_datalog_program"))?;
    let rules = u
        .as_list(rules)
        .ok_or(Transport::Shape("no_datalog_program"))?;
    w.extend_from_slice(b"{\"names\":");
    names_text(u, graph, w)?;
    w.extend_from_slice(b",\"rules\":[");
    for (index, rule) in rules.iter().enumerate() {
        if index > 0 {
            w.push(b',');
        }
        rule_text(u, *rule, w)?;
    }
    w.extend_from_slice(b"],\"seeds\":[");
    for (index, seed) in seeds.iter().enumerate() {
        if index > 0 {
            w.push(b',');
        }
        call_text(u, *seed, None, w)?;
    }
    w.extend_from_slice(b"]}");
    Ok(())
}

fn transport_term(u: &mut Universe, error: Transport) -> TermId {
    match error {
        Transport::Shape(reason) => {
            let reason = u.atom(reason);
            u.compound("program_transport", vec![reason])
        }
        Transport::DuplicateName {
            name,
            first,
            second,
        } => {
            let name = u.atom(&name);
            u.compound("duplicate_relation_name", vec![name, first, second])
        }
    }
}

/// The runtime program as `dl8 eval` reads it, `{"names", "rules", "seeds"}`, written
/// to `w`. `None` is a transport error, after `w` took a prefix; callers that write
/// to a stream run `program_transport_error` first.
pub fn write_program_json(
    u: &Universe,
    checked_datalog: TermId,
    w: &mut dyn std::io::Write,
) -> Option<std::io::Result<()>> {
    let mut out = Out { w, error: None };
    transport(u, checked_datalog, &mut out).ok()?;
    Some(out.error.map_or(Ok(()), Err))
}

/// The diagnostic payload term `write_program_json` would stop on, from a walk into
/// `io::sink()`.
pub fn program_transport_error(u: &mut Universe, checked_datalog: TermId) -> Option<TermId> {
    let mut sink = std::io::sink();
    let error = transport(u, checked_datalog, &mut Out { w: &mut sink, error: None }).err()?;
    Some(transport_term(u, error))
}

/// `write_program_json` into memory, parsed. `Err` is a diagnostic payload term.
pub fn program_to_json(u: &mut Universe, checked_datalog: TermId) -> Result<Value, TermId> {
    let mut text = Vec::new();
    match write_program_json(u, checked_datalog, &mut text) {
        Some(written) => written.expect("a Vec takes every write"),
        None => return Err(program_transport_error(u, checked_datalog).expect("the walk stopped")),
    }
    Ok(serde_json::from_slice(&text).expect("write_program_json writes JSON"))
}

pub fn program_from_json(u: &mut Universe, v: &Value) -> Result<Program, String> {
    let m = v.as_object().ok_or("program is not an object")?;
    let mut program = Program::default();
    for r in m
        .get("rules")
        .and_then(|r| r.as_array())
        .ok_or("program without rules")?
    {
        let rm = r.as_object().ok_or("rule is not an object")?;
        let (rel, head_args) = call_parts(rm.get("head").ok_or("rule without head")?)?;
        let rel = term_from_json(u, rel)?;
        let mut vars = Vec::new();
        let mut head = Vec::new();
        for a in head_args {
            head.push(arg_from_json(u, a, &mut vars)?);
        }
        let mut body = Vec::new();
        for g in rm
            .get("body")
            .and_then(|b| b.as_array())
            .ok_or("rule without body")?
        {
            let gm = g.as_object().ok_or("goal is not an object")?;
            let polarity = match gm.get("polarity").and_then(|p| p.as_str()) {
                Some("positive") => Polarity::Positive,
                Some("negative") => Polarity::Negative,
                other => return Err(format!("bad polarity {other:?}")),
            };
            let (grel, gargs) = call_parts(g)?;
            let grel = term_from_json(u, grel)?;
            let mut args = Vec::new();
            for a in gargs {
                args.push(arg_from_json(u, a, &mut vars)?);
            }
            body.push(Goal {
                polarity,
                rel: grel,
                args,
            });
        }
        program.rules.push(Rule {
            rel,
            head,
            body,
            vars,
        });
    }
    for s in m
        .get("seeds")
        .and_then(|s| s.as_array())
        .ok_or("program without seeds")?
    {
        let (rel, args) = call_parts(s)?;
        let rel = term_from_json(u, rel)?;
        let mut ids = Vec::new();
        for a in args {
            ids.push(term_from_json(u, a)?);
        }
        program.seeds.push(Row { rel, args: ids });
    }
    Ok(program)
}

/// `{"names": {"<declared name>": <relation ref>}}`; absent means serve nothing.
pub fn program_names(u: &mut Universe, v: &Value) -> Result<HashMap<String, TermId>, String> {
    let Some(entries) = v.get("names").and_then(|n| n.as_object()) else {
        return Ok(HashMap::new());
    };
    let mut out = HashMap::with_capacity(entries.len());
    for (name, rel) in entries {
        out.insert(name.clone(), term_from_json(u, rel)?);
    }
    Ok(out)
}

/// Fill `Program.served` from the names the driver was asked to serve.
pub fn serve_relations(
    u: &mut Universe,
    program: &mut Program,
    names: &HashMap<String, TermId>,
    requested: &[String],
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for name in requested {
        match names.get(name) {
            Some(rel) => {
                program.served.insert(*rel);
            }
            None => {
                let atom = u.atom(name);
                let payload = u.compound("served_relation_unknown", vec![atom]);
                diagnostics.push(Diagnostic {
                    phase: "eval",
                    payload,
                });
            }
        }
    }
    diagnostics
}

pub fn row_to_json(u: &Universe, row: &Row) -> Value {
    json!({
        "rel": term_to_json(u, row.rel),
        "args": row.args.iter().map(|a| term_to_json(u, *a)).collect::<Vec<_>>()
    })
}

pub fn diagnostic_to_json(u: &Universe, d: &Diagnostic) -> Value {
    json!({ "phase": d.phase, "payload": term_to_json(u, d.payload) })
}

pub fn closure_to_json(u: &Universe, rows: &[Row], diagnostics: &[Diagnostic]) -> Value {
    let mut m = Map::new();
    m.insert(
        "closure".into(),
        Value::Array(rows.iter().map(|r| row_to_json(u, r)).collect()),
    );
    m.insert(
        "diagnostics".into(),
        Value::Array(
            diagnostics
                .iter()
                .map(|d| diagnostic_to_json(u, d))
                .collect(),
        ),
    );
    Value::Object(m)
}
