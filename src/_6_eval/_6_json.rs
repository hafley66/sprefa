//! JSON transport for programs and closures. Terms: int as number, atom as
//! `{"a": name}`, string as `{"s": text}`, proper list as array, compound as
//! `{"f": name, "args": [...]}`. Rule arguments add `{"v": identity}` and an
//! aggregate key, one of `{"count": arg}`, `{"sum": arg}`, `{"min": arg}`,
//! `{"max": arg}`. A program carries an optional `names` table beside its
//! rules and seeds, which `--serve` resolves against. The oracle dump
//! `v8/oracle/eval/dump_eval.pl` writes the same shape from v7.

use super::program::{AggregateKind, Arg, Diagnostic, Goal, Polarity, Program, Row, Rule, VarId};
use super::term::{Term, TermId, Universe};
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

pub fn term_from_json(u: &mut Universe, v: &Value) -> Result<TermId, String> {
    match v {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(u.int(i))
            } else if let Some(f) = n.as_f64() {
                Ok(u.float(f))
            } else {
                Err(format!("non-numeric number {n}"))
            }
        }
        Value::Bool(b) => Ok(u.boolean(*b)),
        Value::Array(items) => {
            let mut ids = Vec::with_capacity(items.len());
            for item in items {
                ids.push(term_from_json(u, item)?);
            }
            Ok(u.list(&ids))
        }
        Value::Object(m) => {
            if let Some(a) = m.get("a").and_then(atom_name) {
                return Ok(u.atom(&a));
            }
            if let Some(Value::String(s)) = m.get("s") {
                return Ok(u.string(s));
            }
            if let (Some(Value::String(f)), Some(Value::Array(args))) = (m.get("f"), m.get("args"))
            {
                let mut ids = Vec::with_capacity(args.len());
                for a in args {
                    ids.push(term_from_json(u, a)?);
                }
                return Ok(u.compound(f, ids));
            }
            Err(format!("unknown term object {v}"))
        }
        _ => Err(format!("unknown term {v}")),
    }
}

pub fn term_to_json(u: &Universe, id: TermId) -> Value {
    if let Some(items) = u.as_list(id) {
        return Value::Array(items.iter().map(|i| term_to_json(u, *i)).collect());
    }
    match u.get(id) {
        Term::Int(n) => json!(n),
        Term::Float(x) => json!(x.0),
        Term::Bool(b) => json!(b),
        Term::Atom(s) => json!({ "a": u.sym_str(*s) }),
        Term::Str(s) => json!({ "s": u.sym_str(*s) }),
        Term::Compound(s, args) => json!({
            "f": u.sym_str(*s),
            "args": args.iter().map(|a| term_to_json(u, *a)).collect::<Vec<_>>()
        }),
    }
}

fn arg_from_json(u: &mut Universe, v: &Value, vars: &mut Vec<TermId>) -> Result<Arg, String> {
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
            return Ok(Arg::Var(VarId(pos as u32)));
        }
        if let Some((aggregation, subject)) = m
            .iter()
            .find_map(|(name, subject)| Some((AggregateKind::of(name)?, subject)))
        {
            return Ok(Arg::Aggregate(
                aggregation,
                Box::new(arg_from_json(u, subject, vars)?),
            ));
        }
    }
    Ok(Arg::Ground(term_from_json(u, v)?))
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

fn arg_to_json(u: &Universe, id: TermId) -> Value {
    if let Some(identity) = u.unary(id, "var") {
        return json!({ "v": term_to_json(u, identity) });
    }
    if let Some([kind, subject]) = u.args::<2>(id, "aggregate") {
        if let Term::Atom(s) = u.get(kind) {
            let name = u.sym_str(*s);
            if AggregateKind::of(name).is_some() {
                return json!({ name: arg_to_json(u, subject) });
            }
        }
    }
    term_to_json(u, id)
}

fn call_to_json(u: &Universe, id: TermId) -> Result<Value, Transport> {
    let [rel, args] = u.args::<2>(id, "call").ok_or(Transport::Shape("bad_call"))?;
    let args = u.as_list(args).ok_or(Transport::Shape("bad_call"))?;
    Ok(json!({
        "rel": term_to_json(u, rel),
        "args": args.iter().map(|a| arg_to_json(u, *a)).collect::<Vec<_>>(),
    }))
}

fn goal_to_json(u: &Universe, id: TermId) -> Result<Value, Transport> {
    let [polarity, call] = u
        .args::<2>(id, "checked_goal")
        .ok_or(Transport::Shape("bad_goal"))?;
    let Term::Atom(s) = u.get(polarity) else {
        return Err(Transport::Shape("bad_goal"));
    };
    let mut out = call_to_json(u, call)?;
    out["polarity"] = json!(u.sym_str(*s));
    Ok(out)
}

fn rule_to_json(u: &Universe, id: TermId) -> Result<Value, Transport> {
    let [head, body] = u.args::<2>(id, "rule").ok_or(Transport::Shape("bad_rule"))?;
    let body = u.as_list(body).ok_or(Transport::Shape("bad_rule"))?;
    Ok(json!({
        "head": call_to_json(u, head)?,
        "body": body
            .iter()
            .map(|g| goal_to_json(u, *g))
            .collect::<Result<Vec<_>, _>>()?,
    }))
}

/// Every `:(module(M), Name, ref(Relation), _)` bind in the root graph. A file
/// module shadows the prelude's bind of one name; two files disagreeing do not.
/// A `@std/<space>` member carries its namespace, so `--serve git.refs` names it.
fn names_to_json(u: &Universe, graph: TermId) -> Result<Value, Transport> {
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
        // An import binds a module node, never a relation `--serve` can settle.
        match u.unary(relation, "ref") {
            None => continue,
            Some(target) if u.unary(target, "module").is_some() => continue,
            Some(_) => {}
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
        match out.get(&name) {
            Some((_, first)) if *first == relation => {}
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
    Ok(Value::Object(
        out.into_iter()
            .map(|(name, (_, relation))| (name, term_to_json(u, relation)))
            .collect(),
    ))
}

fn transport(u: &Universe, checked_datalog: TermId) -> Result<Value, Transport> {
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
    Ok(json!({
        "rules": rules
            .iter()
            .map(|r| rule_to_json(u, *r))
            .collect::<Result<Vec<_>, _>>()?,
        "seeds": seeds
            .iter()
            .map(|s| call_to_json(u, *s))
            .collect::<Result<Vec<_>, _>>()?,
        "names": names_to_json(u, graph)?,
    }))
}

/// The runtime program as `dl8 eval` reads it: `{"rules", "seeds", "names"}`.
/// `Err` is a diagnostic payload term.
pub fn program_to_json(u: &mut Universe, checked_datalog: TermId) -> Result<Value, TermId> {
    match transport(u, checked_datalog) {
        Ok(value) => Ok(value),
        Err(Transport::Shape(reason)) => {
            let reason = u.atom(reason);
            Err(u.compound("program_transport", vec![reason]))
        }
        Err(Transport::DuplicateName {
            name,
            first,
            second,
        }) => {
            let name = u.atom(&name);
            Err(u.compound("duplicate_relation_name", vec![name, first, second]))
        }
    }
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
