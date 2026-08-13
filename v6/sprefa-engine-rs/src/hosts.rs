// Live host execution for the batch runtime: the Rust port of the tsv2
// HostRunner contract (serve/1_hosts.ts), sharing its emitted HostPlanData.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::types::{Arrival, ArrivalSign, HostColumnPlan, HostPlanData, TickDeltas, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError {
    pub host: String,
    pub message: String,
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sh host '{}': {}", self.host, self.message)
    }
}

pub trait IHostExecutor: Sync {
    fn run(
        &self,
        host: &str,
        command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<String, HostError>;
}

pub fn executor_for(execution: &str) -> Option<&'static dyn IHostExecutor> {
    match execution {
        "shell" => Some(&ShellExecutor),
        // The linked twin: sprefa-extract runs in this process, no child spawn.
        "sprefa_extract" | "sprefa_extract_repo" => Some(&SprefaExtractExecutor),
        _ => None,
    }
}

// Executors whose invocations fold across identical (execution, template,
// inputs), the ApplicativeExecutors set in serve/1_hosts.ts.
fn is_applicative(execution: &str) -> bool {
    matches!(execution, "sprefa_extract" | "sprefa_extract_repo")
}

pub struct ShellExecutor;

impl IHostExecutor for ShellExecutor {
    fn run(
        &self,
        host: &str,
        command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<String, HostError> {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(command_line)
            .envs(env)
            .output()
            .map_err(|failure| HostError {
                host: host.to_string(),
                message: format!("spawn failed for `{command_line}`: {failure}"),
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(HostError {
                host: host.to_string(),
                message: format!("exited {}: {}", output.status, stderr.trim()),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

pub struct SprefaExtractExecutor;

impl IHostExecutor for SprefaExtractExecutor {
    fn run(
        &self,
        host: &str,
        command_line: &str,
        _env: &BTreeMap<String, String>,
    ) -> Result<String, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let tokens = shell_tokens(command_line)
            .map_err(|message| named(format!("unparseable command `{command_line}`: {message}")))?;
        let mut rest = tokens.iter();
        match rest.next() {
            Some(first) if first == "$DL_EXTRACT_BIN" || first.ends_with("extract") => {}
            other => {
                return Err(named(format!(
                    "expected the extract binary first in `{command_line}`, got {other:?}"
                )))
            }
        }
        let mut family: Option<String> = None;
        let mut want_file_fact = false;
        let mut path: Option<String> = None;
        while let Some(token) = rest.next() {
            if token == "--file-fact" {
                want_file_fact = true;
            } else if token == "--family" {
                family = Some(
                    rest.next()
                        .ok_or_else(|| named("--family needs a value".to_string()))?
                        .clone(),
                );
            } else if let Some(value) = token.strip_prefix("--family=") {
                family = Some(value.to_string());
            } else if token.starts_with("--") {
                // The in-process twin covers the per-file surface; an
                // unrecognized flag is a named stop, never a silent shell fall-back.
                return Err(named(format!("flag `{token}` is not linked in-process")));
            } else {
                path = Some(token.clone());
            }
        }
        let path = path.ok_or_else(|| named(format!("no path in `{command_line}`")))?;
        let content = std::fs::read(&path)
            .map_err(|failure| named(format!("read {path} failed: {failure}")))?;
        let mask = match &family {
            None => sprefa_extract::FamilyMask::ALL,
            Some(spec) => {
                let mut mask = sprefa_extract::FamilyMask::NONE;
                for name in spec.split(',') {
                    match name.trim() {
                        "cst" => mask.cst = true,
                        "type" | "types" => mask.types = true,
                        "call" => mask.call = true,
                        "df" => mask.df = true,
                        _ => {}
                    }
                }
                mask
            }
        };
        let mut lines: Vec<String> = Vec::new();
        if want_file_fact {
            let fact = sprefa_extract::file_fact(&path, &content);
            lines.push(serde_json::to_string(&fact).expect("file fact serializes"));
        }
        if let Some(out) = sprefa_extract::dispatch(&path, &content, mask) {
            for fact in sprefa_extract::flatten(&out) {
                lines.push(serde_json::to_string(&fact).expect("flat fact serializes"));
            }
        }
        Ok(lines.join("\n"))
    }
}

// Minimal POSIX-style word split for a filled template: single quotes are
// literal, double quotes keep `\` escapes, bare backslash escapes one char.
fn shell_tokens(command_line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = command_line.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => current.push(inner),
                        None => return Err("unterminated single quote".to_string()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped) => current.push(escaped),
                            None => return Err("dangling escape".to_string()),
                        },
                        Some(inner) => current.push(inner),
                        None => return Err("unterminated double quote".to_string()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                match chars.next() {
                    Some(escaped) => current.push(escaped),
                    None => return Err("dangling escape".to_string()),
                }
            }
            character if character.is_whitespace() => {
                if in_word {
                    tokens.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            character => {
                in_word = true;
                current.push(character);
            }
        }
    }
    if in_word {
        tokens.push(current);
    }
    Ok(tokens)
}

// ═══ template fill (the quote-context walk from serve/1_hosts.ts) ═══════════

#[derive(Clone, Copy, PartialEq, Eq)]
enum QuoteContext {
    Bare,
    Single,
    Double,
}

fn escape_for_shell(value: &str, context: QuoteContext) -> String {
    match context {
        QuoteContext::Single => value.replace('\'', "'\\''"),
        QuoteContext::Double => {
            let mut escaped = String::with_capacity(value.len());
            for character in value.chars() {
                if matches!(character, '\\' | '$' | '`' | '"') {
                    escaped.push('\\');
                }
                escaped.push(character);
            }
            escaped
        }
        QuoteContext::Bare => format!("'{}'", value.replace('\'', "'\\''")),
    }
}

pub fn shell_text(value: &Value) -> String {
    match value {
        Value::Text(text) => text.clone(),
        Value::Integer(number) => number.to_string(),
        Value::Real(number) => crate::ticklog::js_float_text(*number),
        Value::Bool(flag) => if *flag { "true" } else { "false" }.to_string(),
    }
}

pub fn fill_template(template: &str, inputs: &BTreeMap<String, Value>) -> String {
    let mut context = QuoteContext::Bare;
    let mut filled = String::new();
    let characters: Vec<char> = template.chars().collect();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '\\' && context != QuoteContext::Single {
            filled.push(character);
            if index + 1 < characters.len() {
                filled.push(characters[index + 1]);
            }
            index += 2;
            continue;
        }
        if character == '\'' && context != QuoteContext::Double {
            context = if context == QuoteContext::Single {
                QuoteContext::Bare
            } else {
                QuoteContext::Single
            };
            filled.push(character);
            index += 1;
            continue;
        }
        if character == '"' && context != QuoteContext::Single {
            context = if context == QuoteContext::Double {
                QuoteContext::Bare
            } else {
                QuoteContext::Double
            };
            filled.push(character);
            index += 1;
            continue;
        }
        if character == '{' {
            if let Some(close) = characters[index..].iter().position(|&c| c == '}') {
                let name: String = characters[index + 1..index + close].iter().collect();
                if let Some(value) = inputs.get(&name) {
                    filled.push_str(&escape_for_shell(&shell_text(value), context));
                    index += close + 1;
                    continue;
                }
            }
        }
        filled.push(character);
        index += 1;
    }
    filled
}

fn env_for_inputs(inputs: &BTreeMap<String, Value>) -> BTreeMap<String, String> {
    inputs
        .iter()
        .map(|(name, value)| (name.clone(), shell_text(value)))
        .collect()
}

// ═══ output decode (three shapes, same precedence as serve/1_hosts.ts) ══════

fn coerce(host: &str, column: &HostColumnPlan, raw: &serde_json::Value) -> Result<Value, HostError> {
    let named = |message: String| HostError {
        host: host.to_string(),
        message,
    };
    match column.column_type.as_str() {
        "bool" => match raw {
            serde_json::Value::Bool(flag) => Ok(Value::Bool(*flag)),
            serde_json::Value::String(text) if text == "true" => Ok(Value::Bool(true)),
            serde_json::Value::String(text) if text == "false" => Ok(Value::Bool(false)),
            other => Err(named(format!(
                "produced a non-boolean value for bool column '{}': {other}",
                column.name
            ))),
        },
        "float" => {
            let value = match raw {
                serde_json::Value::Number(number) => number.as_f64(),
                other => text_of(other).trim().parse::<f64>().ok(),
            };
            match value {
                Some(number) if number.is_finite() => {
                    Ok(Value::Real(if number == 0.0 { 0.0 } else { number }))
                }
                _ => Err(named(format!(
                    "produced a non-finite value for float column '{}': {raw}",
                    column.name
                ))),
            }
        }
        "int" => {
            let value = match raw {
                serde_json::Value::Number(number) => number.as_i64(),
                other => text_of(other).trim().parse::<i64>().ok(),
            };
            match value {
                Some(number) => Ok(Value::Integer(number)),
                None => Err(named(format!(
                    "produced a non-integer value for int column '{}': {raw}",
                    column.name
                ))),
            }
        }
        // Every other declared type crosses the arrival seam as text; object
        // stdout becomes JSON text for the struct/json planes to intern.
        _ => Ok(Value::Text(text_of(raw))),
    }
}

fn text_of(raw: &serde_json::Value) -> String {
    match raw {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        other => other.to_string(),
    }
}

fn parse_json_items(text: &str) -> Option<Vec<serde_json::Value>> {
    if let Ok(serde_json::Value::Array(items)) = serde_json::from_str(text) {
        return Some(items);
    }
    let lines: Vec<&str> = text
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }
    let mut items = Vec::with_capacity(lines.len());
    for line in lines {
        match serde_json::from_str(line) {
            Ok(item) => items.push(item),
            Err(_) => return None,
        }
    }
    Some(items)
}

fn carries_every_column(item: &serde_json::Map<String, serde_json::Value>, outputs: &[HostColumnPlan]) -> bool {
    outputs
        .iter()
        .all(|column| matches!(item.get(&column.name), Some(value) if !value.is_null()))
}

fn decode_object_items(
    host: &str,
    items: &[serde_json::Value],
    outputs: &[HostColumnPlan],
) -> Result<Vec<Vec<Value>>, HostError> {
    let objects: Vec<&serde_json::Map<String, serde_json::Value>> =
        items.iter().filter_map(|item| item.as_object()).collect();
    if objects.len() != items.len() {
        // Mixed or non-object JSON keeps the positional reading.
        return items
            .iter()
            .map(|item| {
                outputs
                    .iter()
                    .enumerate()
                    .map(|(index, column)| {
                        let raw = match item {
                            serde_json::Value::Array(fields) => {
                                fields.get(index).cloned().unwrap_or(serde_json::Value::Null)
                            }
                            other => other.clone(),
                        };
                        coerce(host, column, &raw)
                    })
                    .collect()
            })
            .collect();
    }
    objects
        .iter()
        .filter(|item| carries_every_column(item, outputs))
        .map(|item| {
            outputs
                .iter()
                .map(|column| {
                    coerce(
                        host,
                        column,
                        item.get(&column.name).unwrap_or(&serde_json::Value::Null),
                    )
                })
                .collect()
        })
        .collect()
}

// GRID before PER-COLUMN, and grid only when every line splits into exactly
// the declared column count (bug host_grid_answer_folded).
fn parse_whitespace(
    host: &str,
    text: &str,
    outputs: &[HostColumnPlan],
) -> Result<Vec<Vec<Value>>, HostError> {
    let lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    let fields_per_line: Vec<Vec<&str>> = lines
        .iter()
        .map(|line| line.split_whitespace().collect())
        .collect();
    let is_grid = fields_per_line
        .iter()
        .all(|fields| fields.len() == outputs.len());
    let text_value =
        |field: Option<&str>| serde_json::Value::String(field.unwrap_or("").to_string());
    if !is_grid && outputs.len() > 1 && lines.len() == outputs.len() {
        let row: Result<Vec<Value>, HostError> = outputs
            .iter()
            .enumerate()
            .map(|(index, column)| coerce(host, column, &text_value(lines.get(index).copied())))
            .collect();
        return Ok(vec![row?]);
    }
    fields_per_line
        .iter()
        .map(|fields| {
            outputs
                .iter()
                .enumerate()
                .map(|(index, column)| coerce(host, column, &text_value(fields.get(index).copied())))
                .collect()
        })
        .collect()
}

pub fn decode_output(
    host: &str,
    stdout: &str,
    outputs: &[HostColumnPlan],
) -> Result<Vec<Vec<Value>>, HostError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() || outputs.is_empty() {
        return Ok(Vec::new());
    }
    match parse_json_items(trimmed) {
        Some(items) => decode_object_items(host, &items, outputs),
        None => parse_whitespace(host, trimmed, outputs),
    }
}

// ═══ the live runner ═════════════════════════════════════════════════════════

struct HostDemand<'p> {
    plan: &'p HostPlanData,
    witness_digest: String,
    inputs: BTreeMap<String, Value>,
}

pub struct HostLiveRunner<'p> {
    plans: Vec<&'p HostPlanData>,
    rel_columns: &'p HashMap<String, Vec<String>>,
    claimed: HashSet<String>,
}

impl<'p> HostLiveRunner<'p> {
    // An executor this runtime does not know is named at construction rather
    // than skipped in silence (the serve/1_hosts.ts refused-plan contract).
    pub fn new(
        plans: &'p [HostPlanData],
        rel_columns: &'p HashMap<String, Vec<String>>,
    ) -> Result<Self, HostError> {
        for plan in plans {
            if executor_for(&plan.execution).is_none() {
                return Err(HostError {
                    host: plan.name.clone(),
                    message: format!("unknown host executor '{}'", plan.execution),
                });
            }
        }
        Ok(HostLiveRunner {
            plans: plans.iter().collect(),
            rel_columns,
            claimed: HashSet::new(),
        })
    }

    pub fn has_plans(&self) -> bool {
        !self.plans.is_empty()
    }

    fn demand_of(&self, plan: &'p HostPlanData, row: &[Value]) -> HostDemand<'p> {
        let columns = self
            .rel_columns
            .get(&plan.demand_rel)
            .map(|columns| columns.as_slice())
            .unwrap_or(&[]);
        let mut inputs = BTreeMap::new();
        for input in &plan.inputs {
            let value = columns
                .iter()
                .position(|column| *column == input.name)
                .and_then(|index| row.get(index).cloned())
                .unwrap_or(Value::Text(String::new()));
            inputs.insert(input.name.clone(), value);
        }
        let witness_digest = columns
            .iter()
            .position(|column| column == "witness_digest")
            .and_then(|index| row.get(index).cloned())
            .map(|value| shell_text(&value))
            .unwrap_or_default();
        HostDemand {
            plan,
            witness_digest,
            inputs,
        }
    }

    fn claim_once(&mut self, plan_name: &str, witness_digest: &str) -> bool {
        self.claimed.insert(format!("{plan_name}|{witness_digest}"))
    }

    fn project(demand: &HostDemand<'p>, stdout: &str, rel_columns: &HashMap<String, Vec<String>>) -> Result<Vec<Arrival>, HostError> {
        let output_rows = decode_output(&demand.plan.name, stdout, &demand.plan.outputs)?;
        let response_columns = rel_columns
            .get(&demand.plan.response_rel)
            .map(|columns| columns.as_slice())
            .unwrap_or(&[]);
        Ok(output_rows
            .into_iter()
            .enumerate()
            .map(|(ordinal, output_row)| Arrival {
                rel: demand.plan.response_rel.clone(),
                sign: ArrivalSign::Add,
                row: response_columns
                    .iter()
                    .map(|column| {
                        if column == "witness_digest" {
                            return Value::Text(demand.witness_digest.clone());
                        }
                        if column == "ordinal" {
                            return Value::Integer(ordinal as i64);
                        }
                        if let Some(input) = demand.inputs.get(column) {
                            return input.clone();
                        }
                        demand
                            .plan
                            .outputs
                            .iter()
                            .position(|output| output.name == *column)
                            .and_then(|index| output_row.get(index).cloned())
                            .unwrap_or(Value::Text(String::new()))
                    })
                    .collect(),
            })
            .collect())
    }

    // One tick's +deltas on every demand rel, claimed, folded per the
    // applicative rule, executed, projected into response-rel arrivals.
    pub fn collect(&mut self, deltas: &TickDeltas) -> Result<Vec<Arrival>, HostError> {
        let mut demands: Vec<HostDemand<'p>> = Vec::new();
        for delta in &deltas.rels {
            let Some(plan) = self
                .plans
                .iter()
                .find(|plan| plan.demand_rel == delta.rel)
                .copied()
            else {
                continue;
            };
            for row in &delta.add {
                demands.push(self.demand_of(plan, row));
            }
        }
        let claimed: Vec<HostDemand<'p>> = demands
            .into_iter()
            .filter(|demand| {
                let name = demand.plan.name.clone();
                let witness = demand.witness_digest.clone();
                self.claim_once(&name, &witness)
            })
            .collect();
        let mut arrivals = Vec::new();
        let mut group_index: HashMap<String, usize> = HashMap::new();
        let mut groups: Vec<Vec<&HostDemand<'p>>> = Vec::new();
        for demand in &claimed {
            if !is_applicative(&demand.plan.execution) {
                groups.push(vec![demand]);
                continue;
            }
            let ordered_inputs: Vec<(String, String)> = demand
                .plan
                .inputs
                .iter()
                .map(|input| {
                    (
                        input.name.clone(),
                        demand
                            .inputs
                            .get(&input.name)
                            .map(shell_text)
                            .unwrap_or_default(),
                    )
                })
                .collect();
            let key = format!(
                "{}|{}|{:?}",
                demand.plan.execution, demand.plan.template, ordered_inputs
            );
            match group_index.get(&key) {
                Some(&index) => groups[index].push(demand),
                None => {
                    group_index.insert(key, groups.len());
                    groups.push(vec![demand]);
                }
            }
        }
        for group in groups {
            let first = group[0];
            let executor =
                executor_for(&first.plan.execution).expect("validated at construction");
            let command_line = fill_template(&first.plan.template, &first.inputs);
            let env = env_for_inputs(&first.inputs);
            let stdout = executor.run(&first.plan.name, &command_line, &env)?;
            for demand in group {
                arrivals.extend(Self::project(demand, &stdout, self.rel_columns)?);
            }
        }
        Ok(arrivals)
    }
}
