use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use effect_runtime::v2::{ByteRange, Diag, Severity};

use super::ast::{OpCall, PipeAst, SlotText};
use super::lower::default_registry;
use super::lower::op_def::{default_plain_dsl_parse, InterpKind, InterpMode};
use super::parse::host_parse;

#[derive(Clone, Debug)]
pub struct RustDaemonSpec {
    pub sprf_path: PathBuf,
    pub root: PathBuf,
    pub bind: String,
    pub fact_db: Option<PathBuf>,
    pub queue_db: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct RustDaemonArtifact {
    pub dir: PathBuf,
    pub manifest_path: PathBuf,
    pub main_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum RustEmitError {
    #[error("io: {0}")]
    Io(String),
    #[error("diagnostics: {0:?}")]
    Diagnostics(Vec<Diag>),
    #[error("build failed: {0}")]
    Build(String),
}

impl From<std::io::Error> for RustEmitError {
    fn from(value: std::io::Error) -> Self {
        RustEmitError::Io(value.to_string())
    }
}

#[derive(Clone, Debug)]
enum SlotValue {
    Atom(String),
    TermRead(String),
    TermBind(String),
    Unsupported,
}

#[derive(Clone, Debug)]
enum DirectStep {
    Unsupported {
        name: String,
    },
    StrConst {
        literal: Arc<str>,
    },
    TermRead {
        name: String,
    },
    TermBind {
        name: String,
    },
    RuleDecl {
        name: String,
        cols: Vec<String>,
    },
    RuleWrite {
        table: String,
        assignments: Vec<DirectAssign>,
    },
    RuleQuery {
        table: String,
        sql: String,
    },
    Split {
        sep: Arc<str>,
        into: Option<String>,
    },
    Fs,
    Glob {
        regex: String,
    },
    Read,
    Re {
        pattern: Arc<str>,
        captures: Vec<String>,
    },
    Json {
        body: Arc<str>,
    },
}

#[derive(Clone, Debug)]
enum DirectAssignValue {
    Term(String),
    Value,
    Literal(String),
}

#[derive(Clone, Debug)]
struct DirectAssign {
    col: String,
    value: DirectAssignValue,
}

#[derive(Clone, Debug)]
struct DirectPipe {
    span: ByteRange,
    steps: Vec<DirectStep>,
}

#[derive(Default)]
struct DirectProgramCtx {
    tables: BTreeMap<String, Vec<String>>,
    unsupported: BTreeSet<String>,
}

pub fn emit_rust_daemon(spec: &RustDaemonSpec) -> Result<RustDaemonArtifact, RustEmitError> {
    let src = std::fs::read_to_string(&spec.sprf_path)?;
    let rendered = emit_rust_daemon_source(&src, spec)?;
    let key = artifact_key(&src, spec);
    let dir = PathBuf::from("target").join("sprefa-dyn").join(key);
    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir)?;
    let manifest_path = dir.join("Cargo.toml");
    let main_path = src_dir.join("main.rs");
    std::fs::write(&manifest_path, rendered_cargo_toml())?;
    std::fs::write(&main_path, rendered)?;
    Ok(RustDaemonArtifact {
        dir,
        manifest_path,
        main_path,
    })
}

pub fn compile_rust_daemon(artifact: &RustDaemonArtifact) -> Result<PathBuf, RustEmitError> {
    let out = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(&artifact.manifest_path)
        .output()?;
    if !out.status.success() {
        return Err(RustEmitError::Build(format!(
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(artifact.dir.join("target/release/sprefa-script-daemon"))
}

pub fn emit_rust_daemon_source(src: &str, spec: &RustDaemonSpec) -> Result<String, RustEmitError> {
    let (program, parse_diags) = host_parse(src);
    if parse_diags.iter().any(|d| d.severity == Severity::Error) {
        return Err(RustEmitError::Diagnostics(parse_diags));
    }
    let reg = default_registry();
    let graph_diags = super::binding_graph::analyze_program(&program, &reg);
    if graph_diags.iter().any(|d| d.severity == Severity::Error) {
        return Err(RustEmitError::Diagnostics(graph_diags));
    }

    let mut ctx = DirectProgramCtx::default();
    let direct: Vec<DirectPipe> = program
        .iter()
        .map(|pipe| direct_pipe(pipe, &mut ctx))
        .collect();
    Ok(render_source(spec, &direct, &ctx))
}

fn direct_pipe(pipe: &PipeAst, ctx: &mut DirectProgramCtx) -> DirectPipe {
    let steps = pipe
        .steps
        .iter()
        .enumerate()
        .map(|(idx, op)| direct_step(op, idx, ctx))
        .collect();
    DirectPipe {
        span: pipe.span,
        steps,
    }
}

fn direct_step(op: &OpCall, chain_pos: usize, ctx: &mut DirectProgramCtx) -> DirectStep {
    if op.flow.is_some() || op.block.is_some() || op.apply || op.force {
        return unsupported(op, ctx);
    }
    let name = op.name.as_ref();
    if op.flow.is_none()
        && op.args.is_empty()
        && op.dsl.is_none()
        && op.block.is_none()
        && is_caps_ident(name)
    {
        return if op.predicate {
            DirectStep::TermBind {
                name: name.to_string(),
            }
        } else {
            DirectStep::TermRead {
                name: name.to_string(),
            }
        };
    }
    match name {
        "str" => {
            let Some(dsl) = &op.dsl else {
                return unsupported(op, ctx);
            };
            let interps = default_plain_dsl_parse(&dsl.raw);
            if interps.is_empty() {
                DirectStep::StrConst {
                    literal: dsl.raw.clone(),
                }
            } else {
                unsupported(op, ctx)
            }
        }
        "term" => match atom_arg(&op.args, 0) {
            Some(name) => DirectStep::TermRead { name },
            None => unsupported(op, ctx),
        },
        "term_bind" => match atom_arg(&op.args, 0) {
            Some(name) => DirectStep::TermBind { name },
            None => unsupported(op, ctx),
        },
        "rule" if chain_pos == 0 && !op.predicate => {
            let Some(table) = atom_arg(&op.args, 0) else {
                return unsupported(op, ctx);
            };
            let cols: Vec<String> = op.args[1..]
                .iter()
                .filter_map(slot_value)
                .filter_map(|v| match v {
                    SlotValue::Atom(s) | SlotValue::TermRead(s) | SlotValue::TermBind(s) => Some(s),
                    SlotValue::Unsupported => None,
                })
                .collect();
            ctx.tables.insert(table.clone(), cols.clone());
            DirectStep::RuleDecl { name: table, cols }
        }
        "split" => {
            let Some(dsl) = &op.dsl else {
                return unsupported(op, ctx);
            };
            let into = match op.args.len() {
                0 => None,
                1 => match slot_value(&op.args[0]) {
                    Some(SlotValue::Atom(s) | SlotValue::TermRead(s) | SlotValue::TermBind(s)) => {
                        Some(s)
                    }
                    _ => return unsupported(op, ctx),
                },
                _ => return unsupported(op, ctx),
            };
            DirectStep::Split {
                sep: dsl.raw.clone(),
                into,
            }
        }
        "fs" if op.args.is_empty() && op.dsl.is_none() => DirectStep::Fs,
        "glob" => {
            let Some(dsl) = &op.dsl else {
                return unsupported(op, ctx);
            };
            let interps = default_plain_dsl_parse(&dsl.raw);
            match glob_body_to_regex(dsl.raw.as_ref(), &interps) {
                Ok(regex) => DirectStep::Glob { regex },
                Err(_) => unsupported(op, ctx),
            }
        }
        "read" if op.args.is_empty() && op.dsl.is_none() => DirectStep::Read,
        "re" => {
            let Some(dsl) = &op.dsl else {
                return unsupported(op, ctx);
            };
            let captures = scan_re_named_groups(dsl.raw.as_ref());
            DirectStep::Re {
                pattern: dsl.raw.clone(),
                captures,
            }
        }
        "json" => {
            let Some(dsl) = &op.dsl else {
                return unsupported(op, ctx);
            };
            if op.args.is_empty() {
                DirectStep::Json {
                    body: dsl.raw.clone(),
                }
            } else {
                unsupported(op, ctx)
            }
        }
        table if ctx.tables.contains_key(table) && !op.predicate => {
            direct_rule_write(table, op, ctx).unwrap_or_else(|| unsupported(op, ctx))
        }
        table if ctx.tables.contains_key(table) && op.predicate => {
            direct_rule_query(table, op, ctx).unwrap_or_else(|| unsupported(op, ctx))
        }
        _ => unsupported(op, ctx),
    }
}

fn unsupported(op: &OpCall, ctx: &mut DirectProgramCtx) -> DirectStep {
    ctx.unsupported.insert(op.name.to_string());
    DirectStep::Unsupported {
        name: op.name.to_string(),
    }
}

fn direct_rule_write(table: &str, op: &OpCall, ctx: &DirectProgramCtx) -> Option<DirectStep> {
    let cols = ctx.tables.get(table)?;
    if op.args.is_empty() {
        return Some(DirectStep::RuleWrite {
            table: table.to_string(),
            assignments: Vec::new(),
        });
    }
    if op.args.len() > cols.len() {
        return None;
    }
    let mut assignments = Vec::with_capacity(op.args.len());
    for (idx, arg) in op.args.iter().enumerate() {
        let col = cols[idx].clone();
        let value = match slot_value(arg)? {
            SlotValue::Atom(s) if s == "&.value" => DirectAssignValue::Value,
            SlotValue::Atom(s) => DirectAssignValue::Literal(s),
            SlotValue::TermRead(s) => DirectAssignValue::Term(s),
            SlotValue::TermBind(_) | SlotValue::Unsupported => return None,
        };
        assignments.push(DirectAssign { col, value });
    }
    Some(DirectStep::RuleWrite {
        table: table.to_string(),
        assignments,
    })
}

fn direct_rule_query(table: &str, op: &OpCall, ctx: &DirectProgramCtx) -> Option<DirectStep> {
    let cols = ctx.tables.get(table)?;
    if op.args.len() > cols.len() {
        return None;
    }
    let sql = rule_query_sql(table, cols, &op.args)?;
    Some(DirectStep::RuleQuery {
        table: table.to_string(),
        sql,
    })
}

fn rule_query_sql(table: &str, cols: &[String], args: &[SlotText]) -> Option<String> {
    enum Mode {
        BoundTerm { col: String, term: String },
        BoundLiteral { col: String, value: String },
        Project { col: String, term: String },
    }
    let mut modes = Vec::with_capacity(args.len());
    for (idx, arg) in args.iter().enumerate() {
        let col = cols[idx].clone();
        match slot_value(arg)? {
            SlotValue::Atom(value) if value == "&.value" => modes.push(Mode::BoundTerm {
                col,
                term: "value".to_string(),
            }),
            SlotValue::Atom(value) => modes.push(Mode::BoundLiteral { col, value }),
            SlotValue::TermRead(term) => modes.push(Mode::BoundTerm { col, term }),
            SlotValue::TermBind(term) => modes.push(Mode::Project { col, term }),
            SlotValue::Unsupported => return None,
        }
    }
    let rule_alias = "__rule";
    let mut select_cols = vec!["input.__cursor_idx".to_string()];
    if modes.is_empty() {
        for col in cols {
            select_cols.push(format!(
                "{}.{} AS {}",
                quote_ident(rule_alias),
                quote_ident(col),
                quote_ident(col)
            ));
        }
    } else {
        for mode in &modes {
            match mode {
                Mode::Project { col, term } => select_cols.push(format!(
                    "{}.{} AS {}",
                    quote_ident(rule_alias),
                    quote_ident(col),
                    quote_ident(term)
                )),
                Mode::BoundLiteral { col, value } => select_cols.push(format!(
                    "{} AS {}",
                    quote_sql_literal(value),
                    quote_ident(col)
                )),
                Mode::BoundTerm { .. } => {}
            }
        }
    }
    let mut predicates = Vec::new();
    for mode in &modes {
        match mode {
            Mode::BoundTerm { col, term } => predicates.push(format!(
                "{}.{} = input.{}",
                quote_ident(rule_alias),
                quote_ident(col),
                quote_ident(term)
            )),
            Mode::BoundLiteral { col, value } => predicates.push(format!(
                "{}.{} = {}",
                quote_ident(rule_alias),
                quote_ident(col),
                quote_sql_literal(value)
            )),
            Mode::Project { .. } => {}
        }
    }
    let all_grounded =
        !modes.is_empty() && modes.iter().all(|m| !matches!(m, Mode::Project { .. }));
    let mut sql = format!(
        "{} {}\nFROM input JOIN {} AS {} ON 1=1",
        if all_grounded {
            "SELECT DISTINCT"
        } else {
            "SELECT"
        },
        select_cols.join(", "),
        quote_ident(table),
        rule_alias
    );
    if !predicates.is_empty() {
        sql.push_str("\nWHERE ");
        sql.push_str(&predicates.join(" AND "));
    }
    Some(sql)
}

fn slot_value(slot: &SlotText) -> Option<SlotValue> {
    let raw = slot.raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(rest) = raw.strip_prefix(':') {
        return Some(SlotValue::Atom(rest.to_string()));
    }
    if raw == "&.value" {
        return Some(SlotValue::Atom(raw.to_string()));
    }
    if raw.len() >= 2 {
        let bytes = raw.as_bytes();
        if bytes[0] == b'`' && bytes[raw.len() - 1] == b'`' {
            return Some(SlotValue::Atom(raw[1..raw.len() - 1].to_string()));
        }
    }
    if let Some(stripped) = raw.strip_suffix('?') {
        if is_caps_ident(stripped) {
            return Some(SlotValue::TermBind(stripped.to_string()));
        }
    }
    if is_caps_ident(raw) {
        return Some(SlotValue::TermRead(raw.to_string()));
    }
    if is_ident(raw) {
        return Some(SlotValue::Atom(raw.to_string()));
    }
    Some(SlotValue::Unsupported)
}

fn atom_arg(args: &[SlotText], idx: usize) -> Option<String> {
    match slot_value(args.get(idx)?)? {
        SlotValue::Atom(s) => Some(s),
        _ => None,
    }
}

fn render_source(spec: &RustDaemonSpec, pipes: &[DirectPipe], ctx: &DirectProgramCtx) -> String {
    let mut out = String::new();
    out.push_str(
        r#"use std::path::PathBuf;
use std::sync::Arc;

use effect_runtime::v2::Pipe;
use v4::app::{GeneratedPipe, GeneratedProgram, SprfState};
use v4::compile::lower::op_def::{DslInterp, InterpKind, InterpMode};
use v4::fact::{FactWrite, WriteAssign, WriteValue};
use v4::pipeline::{GlobComponent, StrConstComponent, StrTemplateComponent};
use v4::sql::SqlQueryComponent;
use v4::term::Term;
use v4::v2_ops::{FsComponent, JsonComponent, ReComponent, ReadComponent, SplitComponent};
use v4::Cursor;

"#,
    );
    out.push_str("fn main_bind_arg(args: &[String], flag: &str, default: &str) -> String {\n");
    out.push_str("    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone()).unwrap_or_else(|| default.to_string())\n");
    out.push_str("}\n\n");
    out.push_str("fn main_arg(args: &[String], flag: &str) -> Option<String> {\n");
    out.push_str("    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())\n");
    out.push_str("}\n\n");
    out.push_str("fn build_program(state: &SprfState) -> GeneratedProgram {\n");
    out.push_str("    let ctx = state.generated_ctx();\n");
    out.push_str("    let pipes = vec![\n");
    for (idx, pipe) in pipes.iter().enumerate() {
        let identity = pipe_identity(spec, pipe, idx);
        let _ = writeln!(
            out,
            "        GeneratedPipe {{ identity: {identity}, pipe: build_pipe_{idx}(&ctx) }},"
        );
    }
    out.push_str("    ];\n");
    out.push_str("    GeneratedProgram {\n");
    out.push_str("        tables: vec![");
    for (idx, name) in ctx.tables.keys().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "{}.to_string()", rust_string(name));
    }
    out.push_str("],\n");
    out.push_str("        pipes,\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    for (idx, pipe) in pipes.iter().enumerate() {
        render_pipe_fn(&mut out, idx, pipe);
    }
    out.push_str(
        r#"#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bind = main_bind_arg(&args, "--bind", "#,
    );
    out.push_str(&rust_string(&spec.bind));
    out.push_str(
        r#");
    let root = PathBuf::from(main_bind_arg(&args, "--root", "#,
    );
    out.push_str(&rust_string(&spec.root.display().to_string()));
    out.push_str(
        r#"));
    let fact_db = main_arg(&args, "--fact-db").map(PathBuf::from);
    let queue_db = main_arg(&args, "--queue-db").map(PathBuf::from);
    let state = Arc::new(match (fact_db, queue_db) {
        (Some(fact_db), Some(queue_db)) => SprfState::new_with_sqlite_backends(root, fact_db, queue_db),
        (Some(fact_db), None) => SprfState::new_with_sqlite_facts(root, fact_db),
        (None, Some(queue_db)) => SprfState::new_with_sqlite_queue(root, queue_db),
        (None, None) => SprfState::new(root),
    });
    let report = state.mount_generated_program(build_program(state.as_ref()));
    if !report.runtime_diags.is_empty() {
        eprintln!("{:?}", report.runtime_diags);
    }
    let router = v4::app::build_router(state);
    let listener = tokio::net::TcpListener::bind(&bind).await.expect("bind compiled sprf daemon");
    eprintln!("sprefa compiled daemon listening on http://{}", bind);
    axum::serve(listener, router).await.expect("serve compiled sprf daemon");
}
"#,
    );
    out
}

fn render_pipe_fn(out: &mut String, idx: usize, pipe: &DirectPipe) {
    let _ = writeln!(
        out,
        "fn build_pipe_{idx}(ctx: &v4::app::GeneratedCtx) -> Pipe<Cursor> {{"
    );
    out.push_str("    let mut pipe = Pipe::new();\n");
    for step in &pipe.steps {
        render_step(out, step);
    }
    out.push_str("    pipe\n");
    out.push_str("}\n\n");
}

fn render_step(out: &mut String, step: &DirectStep) {
    match step {
        DirectStep::Unsupported { name } => {
            let _ = writeln!(
                out,
                "    compile_error!({});",
                rust_string(&format!("op `{name}` has no direct Rust emitter yet"))
            );
        }
        DirectStep::StrConst { literal } => {
            let _ = writeln!(
                out,
                "    pipe = pipe.step(Arc::new(StrConstComponent {{ literal: Arc::<str>::from({}) }}));",
                rust_string(literal)
            );
        }
        DirectStep::TermRead { name } => {
            let _ = writeln!(
                out,
                "    pipe = pipe.step(Arc::new(Term::read(Arc::<str>::from({}))));",
                rust_string(name)
            );
        }
        DirectStep::TermBind { name } => {
            let _ = writeln!(
                out,
                "    pipe = pipe.step(Arc::new(Term::bind(Arc::<str>::from({}))));",
                rust_string(name)
            );
        }
        DirectStep::RuleDecl { name, cols } => {
            out.push_str("    ctx.facts.declare(");
            out.push_str(&rust_string(name));
            out.push_str(", &[");
            for (idx, col) in cols.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(&rust_string(col));
            }
            out.push_str("]);\n");
        }
        DirectStep::RuleWrite { table, assignments } => {
            if assignments.is_empty() {
                let _ = writeln!(
                    out,
                    "    pipe = pipe.step(Arc::new(FactWrite::new(ctx.facts.clone(), Arc::<str>::from({}))));",
                    rust_string(table)
                );
            } else {
                out.push_str("    pipe = pipe.step(Arc::new(FactWrite::projected(ctx.facts.clone(), Arc::<str>::from(");
                out.push_str(&rust_string(table));
                out.push_str("), vec![");
                for (idx, assignment) in assignments.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(", ");
                    }
                    out.push_str("WriteAssign { col: Arc::<str>::from(");
                    out.push_str(&rust_string(&assignment.col));
                    out.push_str("), value: ");
                    render_write_value(out, &assignment.value);
                    out.push_str(" }");
                }
                out.push_str("])));\n");
            }
        }
        DirectStep::RuleQuery { table, sql } => {
            let _ = writeln!(
                out,
                "    pipe = pipe.step(Arc::new(SqlQueryComponent::with_referenced_tables(ctx.facts.clone(), {}.to_string(), vec![{}.to_string()])));",
                rust_string(sql),
                rust_string(table)
            );
        }
        DirectStep::Split { sep, into } => match into {
            Some(into) => {
                let _ = writeln!(
                    out,
                    "    pipe = pipe.step(Arc::new(SplitComponent::on_value({}).into_term({})));",
                    rust_string(sep),
                    rust_string(into)
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "    pipe = pipe.step(Arc::new(SplitComponent::on_value({})));",
                    rust_string(sep)
                );
            }
        },
        DirectStep::Fs => {
            out.push_str("    let fs = FsComponent::new(ctx.root.clone(), 1024).with_sprf_store(ctx.sprf_store.clone()).with_config(ctx.config.clone());\n");
            out.push_str("    pipe = pipe.step(Arc::new(fs));\n");
        }
        DirectStep::Glob { regex } => {
            let _ = writeln!(
                out,
                "    let glob = GlobComponent::new(regex::Regex::new({}).expect(\"compiled sprf glob regex\")).with_sprf_store(ctx.sprf_store.clone());",
                rust_string(regex)
            );
            out.push_str("    pipe = pipe.step(Arc::new(glob));\n");
        }
        DirectStep::Read => {
            out.push_str("    let read = ReadComponent::new().with_root(ctx.root.clone()).with_sprf_store(ctx.sprf_store.clone()).with_config(ctx.config.clone());\n");
            out.push_str("    pipe = pipe.step(Arc::new(read));\n");
        }
        DirectStep::Re { pattern, captures } => {
            out.push_str("    let re_captures: Vec<&str> = vec![");
            for (idx, capture) in captures.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(&rust_string(capture));
            }
            out.push_str("];\n");
            let _ = writeln!(
                out,
                "    let re = ReComponent::new({}, &re_captures).with_sprf_store(ctx.sprf_store.clone());",
                rust_string(pattern)
            );
            out.push_str("    pipe = pipe.step(Arc::new(re));\n");
        }
        DirectStep::Json { body } => {
            let _ = writeln!(
                out,
                "    let json_compiled = v4::cst::dsls::json::JsonDsl::compile_typed({}.as_bytes()).expect(\"compiled sprf json body\");",
                rust_string(body)
            );
            out.push_str("    let json = JsonComponent::new(json_compiled).with_root(ctx.root.clone()).with_sprf_store(ctx.sprf_store.clone()).with_config(ctx.config.clone());\n");
            out.push_str("    pipe = pipe.step(Arc::new(json));\n");
        }
    }
}

fn render_write_value(out: &mut String, value: &DirectAssignValue) {
    match value {
        DirectAssignValue::Term(term) => {
            let _ = write!(
                out,
                "WriteValue::Term(Arc::<str>::from({}))",
                rust_string(term)
            );
        }
        DirectAssignValue::Value => out.push_str("WriteValue::Value"),
        DirectAssignValue::Literal(value) => {
            let _ = write!(
                out,
                "WriteValue::Literal(Arc::<str>::from({}))",
                rust_string(value)
            );
        }
    }
}

fn artifact_key(src: &str, spec: &RustDaemonSpec) -> String {
    let mut h = blake3::Hasher::new();
    h.update(src.as_bytes());
    h.update(spec.sprf_path.display().to_string().as_bytes());
    h.finalize().to_hex()[..16].to_string()
}

fn pipe_identity(spec: &RustDaemonSpec, pipe: &DirectPipe, idx: usize) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(spec.sprf_path.display().to_string().as_bytes());
    h.update(&pipe.span.lo.to_be_bytes());
    h.update(&pipe.span.hi.to_be_bytes());
    h.update(&(idx as u64).to_be_bytes());
    let bytes = h.finalize();
    u64::from_be_bytes(bytes.as_bytes()[..8].try_into().unwrap())
}

fn rendered_cargo_toml() -> String {
    r#"[package]
name = "sprefa-script-daemon"
version = "0.0.0"
edition = "2021"

[dependencies]
v4 = { path = "../../.." }
effect_runtime = { path = "../../../../v3/crates/effect_runtime", default-features = false, features = ["sqlite"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net"] }
axum = "0.8"
regex = "1"
"#
    .to_string()
}

fn rust_string(s: &str) -> String {
    format!("{s:?}")
}

fn quote_ident(s: &str) -> String {
    s.to_string()
}

fn quote_sql_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn scan_re_named_groups(raw: &str) -> Vec<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 4 < bytes.len() {
        let prefix_len = if bytes[i] == b'('
            && bytes[i + 1] == b'?'
            && bytes[i + 2] == b'P'
            && bytes[i + 3] == b'<'
        {
            4
        } else if bytes[i] == b'(' && bytes[i + 1] == b'?' && bytes[i + 2] == b'<' {
            3
        } else {
            i += 1;
            continue;
        };
        let name_lo = i + prefix_len;
        let mut j = name_lo;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
        }
        if j > name_lo && j < bytes.len() && bytes[j] == b'>' {
            out.push(raw[name_lo..j].to_string());
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

fn glob_body_to_regex(
    raw: &str,
    interps: &[super::lower::op_def::DslInterp],
) -> Result<String, ()> {
    let bytes = raw.as_bytes();
    let by_lo: BTreeMap<u32, &super::lower::op_def::DslInterp> =
        interps.iter().map(|i| (i.range.lo, i)).collect();
    let mut out = String::with_capacity(raw.len() + 8);
    let mut i = 0usize;
    while i < bytes.len() {
        let triple_dollar = i + 3 < bytes.len()
            && bytes[i] == b'$'
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'$'
            && bytes[i + 3] == b'{';
        let interp_lo = if triple_dollar {
            (i + 2) as u32
        } else {
            i as u32
        };
        if let Some(interp) = by_lo.get(&interp_lo) {
            match &interp.kind {
                InterpKind::Term { mode, field } => {
                    if field.is_some() {
                        return Err(());
                    }
                    match mode {
                        InterpMode::Bind => {
                            out.push_str("(?P<");
                            out.push_str(interp.name.as_ref());
                            out.push_str(if triple_dollar { ">.*)" } else { ">[^/]*)" });
                        }
                        InterpMode::Read => return Err(()),
                    }
                }
                InterpKind::SubPipe { .. } => return Err(()),
            }
            i = interp.range.hi as usize;
            continue;
        }
        match bytes[i] {
            b'*' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    out.push_str(".*");
                    i += 2;
                } else {
                    out.push_str("[^/]*");
                    i += 1;
                }
            }
            b'?' => {
                out.push_str("[^/]");
                i += 1;
            }
            b'[' => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b']' {
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(());
                }
                out.push_str(&raw[i..=j]);
                i = j + 1;
            }
            b'{' => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b'}' {
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(());
                }
                out.push_str("(?:");
                for (k, part) in raw[i + 1..j].split(',').enumerate() {
                    if k > 0 {
                        out.push('|');
                    }
                    out.push_str(&regex::escape(part));
                }
                out.push(')');
                i = j + 1;
            }
            _ => {
                out.push_str(&regex::escape(&raw[i..i + 1]));
                i += 1;
            }
        }
    }
    out.push('$');
    Ok(out)
}

fn is_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    !bytes.is_empty()
        && (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

fn is_caps_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_uppercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
}
