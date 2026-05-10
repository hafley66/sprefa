use std::path::PathBuf;
use std::sync::Arc;

use effect_runtime::v2::{
    splice_into, Component, NextKey, Node, Pipe, QueueBackend, QueueRow, RenderCtx, Wake,
};
use rusqlite::{Connection, OpenFlags};

use crate::compile::lower::op_def::{ArgKind, ArgSig, OperatorDef};
use crate::compile::lower::value::{CallArg, Value};
use crate::compile::lower::{LowerCtx, LowerError};
use crate::git_watch::GIT_PR_DOMAIN;
use crate::mounted_query;
use crate::sprf_introspect::PipeIntrospect;
use crate::Cursor;

const PR_COLS: &[GhPrCol] = &[
    GhPrCol { arg: "REPO" },
    GhPrCol { arg: "NUMBER" },
    GhPrCol { arg: "TITLE" },
    GhPrCol { arg: "STATE" },
    GhPrCol { arg: "HEAD" },
    GhPrCol { arg: "BASE" },
    GhPrCol { arg: "UPDATED_AT" },
];

const GH_PRS_ARGS: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Any,
        name: "REPO",
        doc: "repo slug",
        required: false,
    },
    ArgSig {
        kind: ArgKind::Any,
        name: "NUMBER",
        doc: "PR number",
        required: false,
    },
    ArgSig {
        kind: ArgKind::Any,
        name: "TITLE",
        doc: "PR title",
        required: false,
    },
    ArgSig {
        kind: ArgKind::Any,
        name: "STATE",
        doc: "PR state",
        required: false,
    },
    ArgSig {
        kind: ArgKind::Any,
        name: "HEAD",
        doc: "head ref",
        required: false,
    },
    ArgSig {
        kind: ArgKind::Any,
        name: "BASE",
        doc: "base ref",
        required: false,
    },
    ArgSig {
        kind: ArgKind::Any,
        name: "UPDATED_AT",
        doc: "updated timestamp",
        required: false,
    },
];

#[derive(Clone, Copy)]
struct GhPrCol {
    arg: &'static str,
}

#[derive(Clone)]
enum GhArgMode {
    Omitted,
    Bind,
    Read(&'static str),
    Literal(Arc<str>),
}

pub struct GhPrsDef;

impl OperatorDef for GhPrsDef {
    fn name(&self) -> &'static str {
        "gh.prs"
    }
    fn paren_args(&self) -> &[ArgSig] {
        GH_PRS_ARGS
    }
    fn cursor_binds(&self) -> &'static [&'static str] {
        &[
            "REPO",
            "NUMBER",
            "TITLE",
            "STATE",
            "HEAD",
            "BASE",
            "UPDATED_AT",
        ]
    }

    fn lower(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        args: &[Value],
        block: Option<Pipe<Cursor>>,
        dsl: Option<&crate::compile::lower::DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let args: Vec<CallArg> = args.iter().cloned().map(CallArg::positional).collect();
        self.lower_call(ctx, flow, &args, block, dsl)
    }

    fn lower_call(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[CallArg],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&crate::compile::lower::DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let db_path = ctx
            .config
            .as_ref()
            .and_then(|c| c.daemon.ghcache_db.clone())
            .ok_or_else(|| {
                LowerError::Unknown("gh.prs: config.daemon.ghcache_db is required".into())
            })?;

        let mut modes = vec![GhArgMode::Omitted; PR_COLS.len()];
        for arg in args {
            let Some(keyword) = arg.keyword.as_deref() else {
                return Err(LowerError::Unknown(
                    "gh.prs: use keyword args, e.g. gh.prs(REPO: REPO?, STATE: `open`)".into(),
                ));
            };
            let Some(idx) = PR_COLS.iter().position(|col| col.arg == keyword) else {
                return Err(LowerError::Unknown(format!(
                    "gh.prs: unknown column {keyword}"
                )));
            };
            modes[idx] = classify_gh_arg(PR_COLS[idx].arg, &arg.value)?;
        }

        Ok(Pipe::new().step(Arc::new(GhPrsComponent {
            db_path,
            store: ctx.store.clone(),
            mount_sql: mount_sql_for_modes(&modes),
            modes,
        })))
    }

    fn lower_call_with_chain(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        args: &[CallArg],
        block: Option<Pipe<Cursor>>,
        dsl: Option<&crate::compile::lower::DslBody>,
        _chain_pos: usize,
    ) -> Result<Pipe<Cursor>, LowerError> {
        self.lower_call(ctx, flow, args, block, dsl)
    }
}

fn classify_gh_arg(col: &'static str, value: &Value) -> Result<GhArgMode, LowerError> {
    match value {
        Value::Atom(s) => Ok(GhArgMode::Literal(s.clone())),
        Value::Pipe(p) => {
            let binds = p.binds_terms();
            let reads = p.reads_terms();
            if !binds.is_empty() {
                return Ok(GhArgMode::Bind);
            }
            if !reads.is_empty() {
                return Ok(GhArgMode::Read(col));
            }
            Err(LowerError::Unknown(format!(
                "gh.prs: column {col} arg must be a term or literal"
            )))
        }
    }
}

struct GhPrsComponent {
    db_path: PathBuf,
    store: Arc<dyn effect_runtime::v2::FactStore<Cursor>>,
    mount_sql: String,
    modes: Vec<GhArgMode>,
}

impl Component for GhPrsComponent {
    type Next = Cursor;

    fn dispatch(
        &self,
        ctx: &RenderCtx,
        rows: &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let inputs: Vec<&Cursor> = rows.iter().map(|r| r.value.as_ref()).collect();
        let nodes = self.render_batch(ctx, &inputs);
        for (row, node) in rows.iter().zip(nodes) {
            splice_into(row, node, ctx.depth + 1, ctx.expand_tick, queue);
            queue.enqueue_replacing_parked(QueueRow {
                id: 0,
                parent_id: Some(row.id),
                batch_idx: row.batch_idx,
                path: row.path.clone(),
                pipe_hash: row.pipe_hash,
                instance_id: row.instance_id,
                depth: row.depth,
                value: row.value.clone(),
                wake: Wake::Key {
                    domain: GIT_PR_DOMAIN.into(),
                    key: mount_key(&self.mount_sql),
                },
                expand_tick: ctx.expand_tick,
                enqueued_at_ns: row.enqueued_at_ns,
            });
        }
    }

    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let prs = match query_pr_rows(&self.db_path) {
            Ok(rows) => rows,
            Err(_) => return batch.iter().map(|_| Node::Done).collect(),
        };

        let nodes: Vec<Node<Cursor>> = batch
            .iter()
            .map(|input| self.render_input(input, &prs))
            .collect();
        let added = mounted_query::record_sql_outputs(
            &self.store,
            &self.mount_sql,
            _ctx.expand_tick,
            batch,
            &["ghcache:pull_request".to_string()],
            &nodes,
        );
        added
    }
}

impl GhPrsComponent {
    fn render_input(&self, input: &Cursor, prs: &[GhPrRow]) -> Node<Cursor> {
        let mut out = Vec::new();
        for pr in prs {
            if !self.matches(input, pr) {
                continue;
            }
            let mut child = input.clone();
            for (idx, mode) in self.modes.iter().enumerate() {
                if matches!(mode, GhArgMode::Omitted) {
                    continue;
                }
                let value = pr.value(PR_COLS[idx].arg);
                child.set(PR_COLS[idx].arg, value);
            }
            child.value = Arc::from(format!("{}#{}", pr.repo, pr.number).as_str());
            out.push(child);
        }
        nodes_from(out)
    }

    fn matches(&self, input: &Cursor, pr: &GhPrRow) -> bool {
        for (idx, mode) in self.modes.iter().enumerate() {
            let expected = match mode {
                GhArgMode::Omitted | GhArgMode::Bind => continue,
                GhArgMode::Literal(s) => s.as_ref(),
                GhArgMode::Read(term) => match input.get(term) {
                    Some(value) => value,
                    None => return false,
                },
            };
            if pr.value(PR_COLS[idx].arg) != expected {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Debug)]
struct GhPrRow {
    repo: String,
    number: String,
    title: String,
    state: String,
    head: String,
    base: String,
    updated_at: String,
}

impl GhPrRow {
    fn value(&self, col: &str) -> &str {
        match col {
            "REPO" => &self.repo,
            "NUMBER" => &self.number,
            "TITLE" => &self.title,
            "STATE" => &self.state,
            "HEAD" => &self.head,
            "BASE" => &self.base,
            "UPDATED_AT" => &self.updated_at,
            _ => "",
        }
    }
}

fn query_pr_rows(db_path: &std::path::Path) -> Result<Vec<GhPrRow>, rusqlite::Error> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut stmt = conn.prepare(
        "SELECT r.owner || '/' || r.name AS repo_slug,
                pr.number,
                pr.title,
                pr.state,
                pr.head_ref,
                pr.base_ref,
                pr.updated_at
         FROM pull_request pr
         JOIN repo r ON r.id = pr.repo_id
         ORDER BY pr.updated_at DESC, r.owner, r.name, pr.number",
    )?;
    let rows = stmt.query_map([], |row| {
        let number: i64 = row.get(1)?;
        Ok(GhPrRow {
            repo: row.get(0)?,
            number: number.to_string(),
            title: row.get(2)?,
            state: row.get(3)?,
            head: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            base: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            updated_at: row.get(6)?,
        })
    })?;
    rows.collect()
}

fn mount_sql_for_modes(modes: &[GhArgMode]) -> String {
    let mut out = String::from("gh.prs");
    for (idx, mode) in modes.iter().enumerate() {
        let col = PR_COLS[idx].arg;
        match mode {
            GhArgMode::Omitted => {}
            GhArgMode::Bind => {
                out.push_str("|bind:");
                out.push_str(col);
            }
            GhArgMode::Read(term) => {
                out.push_str("|read:");
                out.push_str(col);
                out.push('=');
                out.push_str(term);
            }
            GhArgMode::Literal(value) => {
                out.push_str("|lit:");
                out.push_str(col);
                out.push('=');
                out.push_str(value);
            }
        }
    }
    out
}

fn mount_key(mount_sql: &str) -> NextKey {
    NextKey(*blake3::hash(mount_sql.as_bytes()).as_bytes())
}

fn nodes_from(out: Vec<Cursor>) -> Node<Cursor> {
    match out.len() {
        0 => Node::Done,
        1 => Node::Emit(Arc::new(out.into_iter().next().unwrap())),
        _ => Node::Many(out.into_iter().map(|c| Node::Emit(Arc::new(c))).collect()),
    }
}
