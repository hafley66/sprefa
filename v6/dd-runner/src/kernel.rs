//! Pure RAM evaluator for the relational portion of an emitted DD plan.
//!
//! This module deliberately has no SQLite imports. `Operator` is the JSON twin
//! contract: bindings, predicates, projection, and aggregate metadata are the
//! only rule semantics consulted here.

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::{Rel, Row, SignedRow};

pub type Result<T> = std::result::Result<T, String>;
type Tuple = Vec<Value>;
type Relations = BTreeMap<String, Relation>;

/// Insertion-ordered set. The order is observable: `eval_reduce` folds f64 in
/// row order and f64 addition is not associative.
#[derive(Clone, Default)]
struct Relation {
    rows: Vec<Rc<Tuple>>,
    index: HashSet<RowKey>,
}

impl PartialEq for Relation {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
    }
}

impl Relation {
    fn contains(&self, row: &Rc<Tuple>) -> bool {
        self.index.contains(&RowKey(Rc::clone(row)))
    }

    fn insert(&mut self, row: Rc<Tuple>) {
        if self.index.insert(RowKey(Rc::clone(&row))) {
            self.rows.push(row);
        }
    }

    fn remove(&mut self, row: &Rc<Tuple>) {
        if self.index.remove(&RowKey(Rc::clone(row))) {
            self.rows.retain(|held| held != row);
        }
    }
}

#[derive(Clone, Eq)]
struct RowKey(Rc<Tuple>);

impl PartialEq for RowKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Hash for RowKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for value in self.0.iter() {
            hash_value(value, state);
        }
    }
}

/// Agrees with `Value`'s equality, which compares f64 with `==`: -0.0 and 0.0
/// are one key, and an integer variant never equals a float one.
fn hash_value<H: Hasher>(value: &Value, state: &mut H) {
    match value {
        Value::Null => state.write_u8(0),
        Value::Bool(flag) => {
            state.write_u8(1);
            state.write_u8(u8::from(*flag));
        }
        Value::Number(number) => match (number.as_u64(), number.as_i64()) {
            (Some(whole), _) => {
                state.write_u8(2);
                state.write_u64(whole);
            }
            (None, Some(signed)) => {
                state.write_u8(3);
                state.write_i64(signed);
            }
            _ => {
                let float = number.as_f64().unwrap_or_default();
                state.write_u8(4);
                state.write_u64(if float == 0.0 { 0 } else { float.to_bits() });
            }
        },
        Value::String(text) => {
            state.write_u8(5);
            state.write(text.as_bytes());
            state.write_u8(0xff);
        }
        Value::Array(items) => {
            state.write_u8(6);
            state.write_usize(items.len());
            for item in items {
                hash_value(item, state);
            }
        }
        Value::Object(entries) => {
            state.write_u8(7);
            state.write_usize(entries.len());
            for (key, item) in entries {
                state.write(key.as_bytes());
                state.write_u8(0xff);
                hash_value(item, state);
            }
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct Operator {
    id: String,
    kind: String,
    head: String,
    #[serde(default)]
    refs: Vec<String>,
    #[serde(default)]
    bindings: BTreeMap<String, String>,
    #[serde(default)]
    predicates: Vec<Predicate>,
    #[serde(default)]
    projection: Vec<Projection>,
    #[serde(default)]
    aggregate: Option<Aggregate>,
}

#[derive(Clone, Deserialize)]
pub struct Predicate {
    #[serde(default)]
    column_equals: Option<[String; 2]>,
    #[serde(default)]
    literal_equals: Option<LiteralEquals>,
}

#[derive(Clone, Deserialize)]
pub struct LiteralEquals {
    column: String,
    value: Value,
}

#[derive(Clone, Deserialize)]
pub struct Projection {
    #[allow(dead_code)]
    head: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    value: Option<Value>,
}

#[derive(Clone, Deserialize)]
pub struct Aggregate {
    kind: Vec<String>,
    group: Vec<String>,
    value: Vec<String>,
}

pub fn run(
    rels: &[Rel],
    initial: &[Row],
    schedule: &[Vec<SignedRow>],
    operators: &[Operator],
) -> Result<()> {
    let columns = rels
        .iter()
        .map(|rel| (rel.name.clone(), rel.columns.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut state = Relations::new();
    for rel in rels {
        state.insert(rel.name.clone(), Relation::default());
    }
    for row in initial {
        change(&mut state, row, 1)?;
    }
    settle(&mut state, &columns, operators)?;
    let mut before = state.clone();
    for (index, arrivals) in schedule.iter().enumerate() {
        for arrival in arrivals {
            change(&mut state, &arrival.row, arrival.sign)?;
        }
        settle(&mut state, &columns, operators)?;
        println!("{}", tick_json(index + 1, &before, &state));
        before = state.clone();
    }
    Ok(())
}

fn change(state: &mut Relations, row: &Row, sign: i8) -> Result<()> {
    let target = state
        .get_mut(&row.rel)
        .ok_or_else(|| format!("unknown relation {}", row.rel))?;
    let tuple = Rc::new(row.values.clone());
    if sign > 0 {
        target.insert(tuple);
    } else {
        target.remove(&tuple);
    }
    Ok(())
}

fn settle(
    state: &mut Relations,
    columns: &BTreeMap<String, Vec<String>>,
    operators: &[Operator],
) -> Result<()> {
    // Re-evaluation is from the relation descriptions each round. A bounded
    // monotone fixed point covers recursive positive rules; retractions begin
    // each tick from the supplied base state and derived heads are rebuilt.
    let heads = operators
        .iter()
        .map(|op| op.head.clone())
        .collect::<BTreeSet<_>>();
    let base = state.clone();
    for head in &heads {
        state.insert(head.clone(), Relation::default());
    }
    for _ in 0..10_000 {
        let old = state.clone();
        let mut next = base.clone();
        for head in &heads {
            next.insert(head.clone(), Relation::default());
        }
        for op in operators {
            if op.kind == "map"
                && !operators
                    .iter()
                    .any(|other| other.kind == "reduce" && other.head == op.head)
            {
                insert_rows(&mut next, op, eval_rows(op, &old, columns)?);
            }
            if op.kind == "reduce" {
                insert_rows(&mut next, op, eval_reduce(op, &old, columns)?);
            }
        }
        if next == old {
            return Ok(());
        }
        *state = next;
    }
    Err("recursive plan did not reach a fixed point in 10000 rounds".into())
}

fn insert_rows(state: &mut Relations, op: &Operator, rows: Vec<Tuple>) {
    let target = state.entry(op.head.clone()).or_default();
    for row in rows {
        target.insert(Rc::new(row));
    }
}

fn eval_rows(
    op: &Operator,
    state: &Relations,
    columns: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<Tuple>> {
    let bindings = binding_rows(op, state, columns)?;
    bindings
        .into_iter()
        .filter(|row| matches_predicates(row, &op.predicates))
        .map(|row| project(&row, &op.projection))
        .collect()
}

fn eval_reduce(
    op: &Operator,
    state: &Relations,
    columns: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<Tuple>> {
    let aggregate = op
        .aggregate
        .as_ref()
        .ok_or_else(|| format!("{} reduce missing aggregate", op.id))?;
    let mut groups: BTreeMap<String, Vec<BTreeMap<String, Value>>> = BTreeMap::new();
    for row in binding_rows(op, state, columns)? {
        if matches_predicates(&row, &op.predicates) {
            let key = serde_json::to_string(
                &aggregate
                    .group
                    .iter()
                    .map(|column| lookup(&row, column))
                    .collect::<Result<Vec<_>>>()?,
            )
            .unwrap();
            groups.entry(key).or_default().push(row);
        }
    }
    groups
        .into_values()
        .map(|rows| {
            let first = rows
                .first()
                .ok_or_else(|| "empty aggregate group".to_owned())?;
            let mut out = Vec::new();
            for projection in &op.projection {
                if let Some(source) = &projection.source {
                    let col = source.rsplit('.').next().unwrap_or(source);
                    if let Some(position) = aggregate.value.iter().position(|value| value == col) {
                        out.push(aggregate_value(&aggregate.kind[position], &rows, source)?);
                    } else {
                        out.push(lookup(first, source)?);
                    }
                } else {
                    out.push(projection.value.clone().unwrap_or(Value::Null));
                }
            }
            Ok(out)
        })
        .collect()
}

fn aggregate_value(kind: &str, rows: &[BTreeMap<String, Value>], source: &str) -> Result<Value> {
    let values = rows
        .iter()
        .map(|row| lookup(row, source))
        .collect::<Result<Vec<_>>>()?;
    match kind {
        "count" => Ok(json!(values.len() as i64)),
        "sum" | "avg" => {
            let sum = values
                .iter()
                .map(number)
                .collect::<Result<Vec<_>>>()?
                .iter()
                .sum::<f64>();
            if kind == "avg" {
                Ok(json!(sum / values.len() as f64))
            } else {
                Ok(json!(sum))
            }
        }
        "min" => Ok(values
            .into_iter()
            .min_by_key(|value| serde_json::to_string(value).unwrap())
            .unwrap()),
        "max" => Ok(values
            .into_iter()
            .max_by_key(|value| serde_json::to_string(value).unwrap())
            .unwrap()),
        other => Err(format!("unsupported aggregate {other}")),
    }
}

fn number(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .ok_or_else(|| format!("aggregate numeric value required: {value}"))
}

fn binding_rows(
    op: &Operator,
    state: &Relations,
    columns: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<BTreeMap<String, Value>>> {
    let mut out = vec![BTreeMap::new()];
    let bindings = if op.bindings.is_empty() {
        op.refs
            .iter()
            .enumerate()
            .map(|(i, relation)| (format!("b{i}"), relation.clone()))
            .collect()
    } else {
        op.bindings.clone()
    };
    for (alias, relation) in bindings {
        let fields = columns
            .get(&relation)
            .ok_or_else(|| format!("{} references unknown relation {relation}", op.id))?;
        let source = state
            .get(&relation)
            .ok_or_else(|| format!("state has no relation {relation}"))?;
        let mut expanded = Vec::new();
        for prior in &out {
            for tuple in &source.rows {
                let mut item = prior.clone();
                for (column, value) in fields.iter().zip(tuple.iter()) {
                    item.insert(format!("{alias}.{column}"), value.clone());
                }
                expanded.push(item);
            }
        }
        out = expanded;
    }
    Ok(out)
}

fn matches_predicates(row: &BTreeMap<String, Value>, predicates: &[Predicate]) -> bool {
    predicates.iter().all(|predicate| {
        predicate
            .column_equals
            .as_ref()
            .map(|[left, right]| row.get(left) == row.get(right))
            .unwrap_or(true)
            && predicate
                .literal_equals
                .as_ref()
                .map(|literal| row.get(&literal.column) == Some(&literal.value))
                .unwrap_or(true)
    })
}

fn project(row: &BTreeMap<String, Value>, projection: &[Projection]) -> Result<Tuple> {
    projection
        .iter()
        .map(|item| match &item.source {
            Some(source) => lookup(row, source),
            None => Ok(item.value.clone().unwrap_or(Value::Null)),
        })
        .collect()
}

fn lookup(row: &BTreeMap<String, Value>, source: &str) -> Result<Value> {
    row.get(source)
        .cloned()
        .ok_or_else(|| format!("projection source missing: {source}"))
}

fn tick_json(tick: usize, before: &Relations, after: &Relations) -> String {
    let empty = Relation::default();
    let mut deltas = Vec::new();
    for name in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
        let old = before.get(name).unwrap_or(&empty);
        let new = after.get(name).unwrap_or(&empty);
        let add = sorted(
            new.rows
                .iter()
                .filter(|row| !old.contains(row))
                .map(|row| &**row)
                .collect(),
        );
        let del = sorted(
            old.rows
                .iter()
                .filter(|row| !new.contains(row))
                .map(|row| &**row)
                .collect(),
        );
        if !add.is_empty() || !del.is_empty() {
            let relation = serde_json::to_string(name.split('/').next().unwrap()).unwrap();
            deltas.push(format!(
                "{relation}:{{\"add\":{},\"del\":{}}}",
                json!(add),
                json!(del)
            ));
        }
    }
    format!("{{\"tick\":{tick},\"deltas\":{{{}}}}}", deltas.join(","))
}

fn sorted(mut rows: Vec<&Tuple>) -> Vec<&Tuple> {
    rows.sort_by_key(|row| serde_json::to_string(row).unwrap());
    rows
}
