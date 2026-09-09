//! Pure RAM evaluator for the relational portion of an emitted DD plan.
//!
//! This module deliberately has no SQLite imports. `Operator` is the JSON twin
//! contract: bindings, predicates, projection, and aggregate metadata are the
//! only rule semantics consulted here.

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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

/// serde_json's own Hash is the one that agrees with its Eq: `Value` compares
/// f64 with `==`, and hashing 0.0 and -0.0 alike is what keeps them one key.
#[derive(Clone, Eq, PartialEq, Hash)]
struct RowKey(Rc<Tuple>);

#[derive(Clone, Deserialize)]
pub struct Operator {
    pub id: String,
    pub kind: String,
    pub head: String,
    #[serde(default)]
    pub refs: Vec<String>,
    #[serde(default)]
    pub bindings: BTreeMap<String, String>,
    #[serde(default)]
    pub predicates: Vec<Predicate>,
    #[serde(default)]
    pub projection: Vec<Projection>,
    #[serde(default)]
    pub aggregate: Option<Aggregate>,
}

#[derive(Clone, Deserialize)]
pub struct Predicate {
    #[serde(default)]
    pub column_equals: Option<[String; 2]>,
    #[serde(default)]
    pub literal_equals: Option<LiteralEquals>,
}

#[derive(Clone, Deserialize)]
pub struct LiteralEquals {
    pub column: String,
    pub value: Value,
}

#[derive(Clone, Deserialize)]
pub struct Projection {
    #[allow(dead_code)]
    pub head: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub value: Option<Value>,
}

#[derive(Clone, Deserialize)]
pub struct Aggregate {
    pub kind: Vec<String>,
    pub group: Vec<String>,
    pub value: Vec<String>,
}

pub fn run(
    rels: &[Rel],
    initial: &[Row],
    schedule: &[Vec<SignedRow>],
    operators: &[Operator],
) -> Result<()> {
    let mut runtime = Runtime::open(rels, initial, operators.to_vec())?;
    for (index, arrivals) in schedule.iter().enumerate() {
        println!("{}", runtime.tick(index + 1, arrivals)?);
    }
    Ok(())
}

pub struct Runtime {
    relation_names: Vec<String>,
    columns: BTreeMap<String, Vec<String>>,
    state: Relations,
    before: Relations,
    operators: Vec<Operator>,
}

impl Runtime {
    pub fn open(rels: &[Rel], initial: &[Row], operators: Vec<Operator>) -> Result<Self> {
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
        settle(&mut state, &columns, &operators)?;
        let before = state.clone();
        Ok(Self {
            relation_names: rels.iter().map(|relation| relation.name.clone()).collect(),
            columns,
            state,
            before,
            operators,
        })
    }

    pub fn reset(&mut self) -> Result<()> {
        self.state = self
            .relation_names
            .iter()
            .map(|name| (name.clone(), Relation::default()))
            .collect();
        settle(&mut self.state, &self.columns, &self.operators)?;
        self.before = self.state.clone();
        Ok(())
    }

    pub fn accepts(&self, relation: &str) -> bool {
        self.columns.contains_key(relation)
    }

    pub fn row_count(&self, relation: &str) -> usize {
        self.state
            .get(relation)
            .map(|rows| rows.rows.len())
            .unwrap_or(0)
    }

    pub fn tick(&mut self, generation: usize, arrivals: &[SignedRow]) -> Result<String> {
        for arrival in arrivals {
            change(&mut self.state, &arrival.row, arrival.sign)?;
        }
        settle(&mut self.state, &self.columns, &self.operators)?;
        let output = tick_json(generation, &self.before, &self.state);
        self.before = self.state.clone();
        Ok(output)
    }
}

pub fn shootout(graph_case: &str, n: usize) -> Result<Value> {
    if n == 0 {
        return Err("shootout N must be greater than zero".into());
    }
    let rels = vec![
        Rel {
            name: "edge".into(),
            columns: vec!["from".into(), "to".into()],
            select_all: String::new(),
        },
        Rel {
            name: "path".into(),
            columns: vec!["from".into(), "to".into()],
            select_all: String::new(),
        },
    ];
    let copy = Operator {
        id: "map_edge".into(),
        kind: "map".into(),
        head: "path".into(),
        refs: vec!["edge".into()],
        bindings: BTreeMap::from([("b0".into(), "edge".into())]),
        predicates: Vec::new(),
        projection: vec![
            Projection {
                head: "from".into(),
                source: Some("b0.from".into()),
                value: None,
            },
            Projection {
                head: "to".into(),
                source: Some("b0.to".into()),
                value: None,
            },
        ],
        aggregate: None,
    };
    let extend = Operator {
        id: "map_extend".into(),
        kind: "map".into(),
        head: "path".into(),
        refs: vec!["path".into(), "edge".into()],
        bindings: BTreeMap::from([("b0".into(), "path".into()), ("b1".into(), "edge".into())]),
        predicates: vec![Predicate {
            column_equals: Some(["b0.to".into(), "b1.from".into()]),
            literal_equals: None,
        }],
        projection: vec![
            Projection {
                head: "from".into(),
                source: Some("b0.from".into()),
                value: None,
            },
            Projection {
                head: "to".into(),
                source: Some("b1.to".into()),
                value: None,
            },
        ],
        aggregate: None,
    };
    let setup_started = std::time::Instant::now();
    let mut runtime = Runtime::open(&rels, &[], vec![copy, extend])?;
    let setup_ms = setup_started.elapsed().as_secs_f64() * 1000.0;
    let edge_count = match graph_case {
        "chain" => n - 1,
        "ring" => n,
        other => return Err(format!("unknown shootout case {other}")),
    };
    let mut arrivals = (0..n.saturating_sub(1))
        .map(|from| SignedRow {
            sign: 1,
            row: Row {
                rel: "edge".into(),
                values: vec![json!(from), json!(from + 1)],
            },
        })
        .collect::<Vec<_>>();
    if graph_case == "ring" {
        arrivals.push(SignedRow {
            sign: 1,
            row: Row {
                rel: "edge".into(),
                values: vec![json!(n - 1), json!(0)],
            },
        });
    }
    let closure_started = std::time::Instant::now();
    let _ = runtime.tick(1, &arrivals)?;
    let closure_ms = closure_started.elapsed().as_secs_f64() * 1000.0;
    Ok(json!({
        "runtime":"dbsp-kernel",
        "version":env!("CARGO_PKG_VERSION"),
        "case":graph_case,
        "n":n,
        "edge_count":edge_count,
        "closure_count":runtime.row_count("path"),
        "setup_ms":setup_ms,
        "closure_ms":closure_ms,
    }))
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
    // A bounded monotone fixed point covers recursive positive rules;
    // retractions begin each tick from the supplied base state.
    let heads = operators
        .iter()
        .map(|op| op.head.clone())
        .collect::<BTreeSet<_>>();
    let base = std::mem::take(state);
    let mut queries: Vec<Option<Query>> = operators.iter().map(|_| None).collect();
    let mut rows: Vec<Vec<Rc<Tuple>>> = operators.iter().map(|_| Vec::new()).collect();
    let mut change: BTreeMap<String, Change> = BTreeMap::new();
    let mut old = cleared(&base, &heads);
    for round in 0..10_000 {
        let mut next = cleared(&base, &heads);
        for (index, op) in operators.iter().enumerate() {
            if !evaluated(op, operators) {
                continue;
            }
            if queries[index].is_none() {
                queries[index] = Some(Query::compile(op, columns)?);
            }
            let query = queries[index].as_ref().expect("compiled query");
            match work(op, query, &change, round) {
                Work::Reuse => {}
                Work::Append(prefix) => {
                    let tail = &old[&query.scans[0].relation].rows[prefix..];
                    rows[index].extend(eval(op, query, &old, Some(tail))?);
                }
                Work::Full => rows[index] = eval(op, query, &old, None)?,
            }
            insert_rows(&mut next, op, &rows[index]);
        }
        change = changes(&old, &next);
        if change.values().all(|item| matches!(item, Change::Same)) {
            *state = next;
            return Ok(());
        }
        old = next;
    }
    Err("recursive plan did not reach a fixed point in 10000 rounds".into())
}

/// Base relations do not move inside a settle, so only the heads are reset.
fn cleared(base: &Relations, heads: &BTreeSet<String>) -> Relations {
    let mut out = base.clone();
    for head in heads {
        out.insert(head.clone(), Relation::default());
    }
    out
}

fn evaluated(op: &Operator, operators: &[Operator]) -> bool {
    match op.kind.as_str() {
        "reduce" => true,
        "map" => !operators
            .iter()
            .any(|other| other.kind == "reduce" && other.head == op.head),
        _ => false,
    }
}

enum Change {
    Same,
    Appended(usize),
    Rebuilt,
}

enum Work {
    Reuse,
    Append(usize),
    Full,
}

/// Appending is sound only for the outermost binding: the nested loop over it
/// extends by exactly the rows its tail derives, leaving earlier rows in place.
fn work(op: &Operator, query: &Query, change: &BTreeMap<String, Change>, round: usize) -> Work {
    if round == 0 {
        return Work::Full;
    }
    let mut moved = Vec::new();
    for (index, scan) in query.scans.iter().enumerate() {
        match change.get(&scan.relation) {
            Some(Change::Same) => {}
            Some(other) => moved.push((index, other)),
            None => moved.push((index, &Change::Rebuilt)),
        }
    }
    match moved.as_slice() {
        [] => Work::Reuse,
        [(0, Change::Appended(prefix))] if op.kind == "map" => Work::Append(*prefix),
        _ => Work::Full,
    }
}

fn changes(old: &Relations, next: &Relations) -> BTreeMap<String, Change> {
    let mut out = BTreeMap::new();
    for name in old.keys().chain(next.keys()).collect::<BTreeSet<_>>() {
        let change = match (old.get(name), next.get(name)) {
            (Some(before), Some(after)) => compare(before, after),
            (None, None) => Change::Same,
            _ => Change::Rebuilt,
        };
        out.insert(name.clone(), change);
    }
    out
}

fn compare(before: &Relation, after: &Relation) -> Change {
    let held = before.rows.len();
    if before.rows == after.rows {
        Change::Same
    } else if after.rows.len() > held && after.rows[..held] == before.rows[..] {
        Change::Appended(held)
    } else {
        Change::Rebuilt
    }
}

fn eval(
    op: &Operator,
    query: &Query,
    state: &Relations,
    tail: Option<&[Rc<Tuple>]>,
) -> Result<Vec<Rc<Tuple>>> {
    let rows = if op.kind == "reduce" {
        eval_reduce(op, query, state)?
    } else {
        eval_rows(op, query, state, tail)?
    };
    Ok(rows.into_iter().map(Rc::new).collect())
}

fn insert_rows(state: &mut Relations, op: &Operator, rows: &[Rc<Tuple>]) {
    let target = state.entry(op.head.clone()).or_default();
    for row in rows {
        target.insert(Rc::clone(row));
    }
}

fn eval_rows(
    op: &Operator,
    query: &Query,
    state: &Relations,
    tail: Option<&[Rc<Tuple>]>,
) -> Result<Vec<Tuple>> {
    let projection = query.projection(&op.projection);
    binding_rows(query, state, tail)?
        .into_iter()
        .filter(|row| query.holds(row))
        .map(|row| project(&row, &projection))
        .collect()
}

fn eval_reduce(op: &Operator, query: &Query, state: &Relations) -> Result<Vec<Tuple>> {
    let aggregate = op
        .aggregate
        .as_ref()
        .ok_or_else(|| format!("{} reduce missing aggregate", op.id))?;
    let projection = query.projection(&op.projection);
    let grouping = aggregate
        .group
        .iter()
        .map(|column| query.column(column))
        .collect::<Vec<_>>();
    let mut groups: BTreeMap<String, Vec<BoundRow>> = BTreeMap::new();
    for row in binding_rows(query, state, None)? {
        if query.holds(&row) {
            let key = serde_json::to_string(&project(&row, &grouping)?).unwrap();
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
            for (index, item) in op.projection.iter().enumerate() {
                if let Some(source) = &item.source {
                    let col = source.rsplit('.').next().unwrap_or(source);
                    if let Some(position) = aggregate.value.iter().position(|value| value == col) {
                        let kind = &aggregate.kind[position];
                        out.push(aggregate_value(kind, &rows, &projection[index])?);
                    } else {
                        out.push(lookup(first, &projection[index])?);
                    }
                } else {
                    out.push(item.value.clone().unwrap_or(Value::Null));
                }
            }
            Ok(out)
        })
        .collect()
}

fn aggregate_value(kind: &str, rows: &[BoundRow], column: &Column) -> Result<Value> {
    let values = rows
        .iter()
        .map(|row| lookup(row, column))
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

/// A bound row is a slot vector, not a name map. `None` is "this binding never
/// carried the column", which is what an absent map key used to mean.
type BoundRow = Vec<Option<Value>>;

/// One binding, with the equality predicates that reach it resolved into an
/// equijoin: `probe` reads prior slots, `key` reads this relation's tuple.
struct Scan {
    relation: String,
    width: usize,
    probe: Vec<usize>,
    key: Vec<usize>,
    filters: Vec<(usize, Value)>,
}

enum Test {
    Columns(Option<usize>, Option<usize>),
    Literal(Option<usize>, Value),
}

enum Column<'a> {
    Slot(usize, &'a str),
    Absent(&'a str),
    Constant(Option<&'a Value>),
}

#[derive(Eq, PartialEq, Hash)]
struct JoinKey(Vec<Option<Value>>);

struct Query {
    scans: Vec<Scan>,
    slots: BTreeMap<String, usize>,
    tests: Vec<Test>,
}

impl Query {
    fn compile(op: &Operator, columns: &BTreeMap<String, Vec<String>>) -> Result<Query> {
        let bindings = if op.bindings.is_empty() {
            op.refs
                .iter()
                .enumerate()
                .map(|(index, relation)| (format!("b{index}"), relation.clone()))
                .collect::<BTreeMap<_, _>>()
        } else {
            op.bindings.clone()
        };
        let mut query = Query {
            scans: Vec::new(),
            slots: BTreeMap::new(),
            tests: Vec::new(),
        };
        let mut width = 0;
        for (alias, relation) in &bindings {
            let fields = columns
                .get(relation)
                .ok_or_else(|| format!("{} references unknown relation {relation}", op.id))?;
            let prefix = format!("{alias}.");
            let position = |name: &String| {
                name.strip_prefix(&prefix)
                    .and_then(|column| fields.iter().rposition(|field| field == column))
            };
            let mut scan = Scan {
                relation: relation.clone(),
                width: fields.len(),
                probe: Vec::new(),
                key: Vec::new(),
                filters: Vec::new(),
            };
            for predicate in &op.predicates {
                if let Some([left, right]) = &predicate.column_equals {
                    for (bound, arriving) in [(left, right), (right, left)] {
                        if let (Some(slot), Some(at)) = (query.slots.get(bound), position(arriving))
                        {
                            scan.probe.push(*slot);
                            scan.key.push(at);
                        }
                    }
                }
                if let Some(literal) = &predicate.literal_equals {
                    if let Some(at) = position(&literal.column) {
                        scan.filters.push((at, literal.value.clone()));
                    }
                }
            }
            for (at, column) in fields.iter().enumerate() {
                query.slots.insert(format!("{alias}.{column}"), width + at);
            }
            width += fields.len();
            query.scans.push(scan);
        }
        for predicate in &op.predicates {
            if let Some([left, right]) = &predicate.column_equals {
                let sides = (
                    query.slots.get(left).copied(),
                    query.slots.get(right).copied(),
                );
                query.tests.push(Test::Columns(sides.0, sides.1));
            }
            if let Some(literal) = &predicate.literal_equals {
                let slot = query.slots.get(&literal.column).copied();
                query.tests.push(Test::Literal(slot, literal.value.clone()));
            }
        }
        Ok(query)
    }

    fn column<'a>(&self, source: &'a String) -> Column<'a> {
        match self.slots.get(source) {
            Some(slot) => Column::Slot(*slot, source),
            None => Column::Absent(source),
        }
    }

    fn projection<'a>(&self, projection: &'a [Projection]) -> Vec<Column<'a>> {
        projection
            .iter()
            .map(|item| match &item.source {
                Some(source) => self.column(source),
                None => Column::Constant(item.value.as_ref()),
            })
            .collect()
    }

    fn holds(&self, row: &BoundRow) -> bool {
        self.tests.iter().all(|test| match test {
            Test::Columns(left, right) => at(row, *left) == at(row, *right),
            Test::Literal(slot, value) => at(row, *slot) == Some(value),
        })
    }
}

impl Scan {
    fn passes(&self, tuple: &Tuple) -> bool {
        self.filters
            .iter()
            .all(|(at, value)| tuple.get(*at) == Some(value))
    }

    fn extend(&self, prior: &BoundRow, tuple: &Tuple) -> BoundRow {
        let mut row = prior.clone();
        row.extend((0..self.width).map(|at| tuple.get(at).cloned()));
        row
    }
}

fn at(row: &BoundRow, slot: Option<usize>) -> Option<&Value> {
    slot.and_then(|index| row[index].as_ref())
}

fn binding_rows(
    query: &Query,
    state: &Relations,
    tail: Option<&[Rc<Tuple>]>,
) -> Result<Vec<BoundRow>> {
    let mut out = vec![Vec::new()];
    for (index, scan) in query.scans.iter().enumerate() {
        let source = state
            .get(&scan.relation)
            .ok_or_else(|| format!("state has no relation {}", scan.relation))?;
        let rows = match tail {
            Some(rows) if index == 0 => rows,
            _ => &source.rows,
        };
        let mut expanded = Vec::new();
        if scan.probe.is_empty() {
            for prior in &out {
                for tuple in rows {
                    if scan.passes(tuple) {
                        expanded.push(scan.extend(prior, tuple));
                    }
                }
            }
        } else {
            let mut index: HashMap<JoinKey, Vec<usize>> = HashMap::new();
            for (position, tuple) in source.rows.iter().enumerate() {
                if scan.passes(tuple) {
                    let key = scan.key.iter().map(|at| tuple.get(*at).cloned()).collect();
                    index.entry(JoinKey(key)).or_default().push(position);
                }
            }
            for prior in &out {
                let probe = scan.probe.iter().map(|slot| prior[*slot].clone());
                if let Some(positions) = index.get(&JoinKey(probe.collect())) {
                    for position in positions {
                        expanded.push(scan.extend(prior, &source.rows[*position]));
                    }
                }
            }
        }
        out = expanded;
    }
    Ok(out)
}

fn project(row: &BoundRow, projection: &[Column]) -> Result<Tuple> {
    projection
        .iter()
        .map(|column| lookup(row, column))
        .collect()
}

fn lookup(row: &BoundRow, column: &Column) -> Result<Value> {
    match column {
        Column::Slot(slot, source) => row[*slot]
            .clone()
            .ok_or_else(|| format!("projection source missing: {source}")),
        Column::Absent(source) => Err(format!("projection source missing: {source}")),
        Column::Constant(value) => Ok(value.cloned().unwrap_or(Value::Null)),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resident_runtime_reports_derived_add_and_retract() {
        let rels = vec![
            Rel {
                name: "input".into(),
                columns: vec!["value".into()],
                select_all: String::new(),
            },
            Rel {
                name: "output".into(),
                columns: vec!["value".into()],
                select_all: String::new(),
            },
        ];
        let operator = Operator {
            id: "map_0".into(),
            kind: "map".into(),
            head: "output".into(),
            refs: vec!["input".into()],
            bindings: BTreeMap::from([("b0".into(), "input".into())]),
            predicates: Vec::new(),
            projection: vec![Projection {
                head: "value".into(),
                source: Some("b0.value".into()),
                value: None,
            }],
            aggregate: None,
        };
        let row = SignedRow {
            sign: 1,
            row: Row {
                rel: "input".into(),
                values: vec![json!("alpha")],
            },
        };
        let mut runtime = Runtime::open(&rels, &[], vec![operator]).unwrap();

        assert_eq!(
            runtime.tick(4, std::slice::from_ref(&row)).unwrap(),
            r#"{"tick":4,"deltas":{"input":{"add":[["alpha"]],"del":[]},"output":{"add":[["alpha"]],"del":[]}}}"#
        );

        let removed = SignedRow { sign: -1, ..row };
        assert_eq!(
            runtime.tick(5, &[removed]).unwrap(),
            r#"{"tick":5,"deltas":{"input":{"add":[],"del":[["alpha"]]},"output":{"add":[],"del":[["alpha"]]}}}"#
        );
    }
}
