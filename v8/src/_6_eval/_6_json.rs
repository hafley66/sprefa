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
use std::collections::HashMap;

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
