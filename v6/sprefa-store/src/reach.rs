//! The v5 graph covering set, on-disk over `cx_dep(parent_key, child_key)`.
//!
//! Each function here has a resident pure-Rust ORACLE in the repo root
//! (`src/graph/scc.rs`, `src/graph/walk.rs`) and MUST agree with it byte-for-byte
//! (partition-canonical for SCC). `tests/covering.rs` is the standing check.
//!
//! FROZEN CONTRACT + exact inclusion semantics: `v6/findings/INSIGHTS.md` §3, §A.
//! Recursive-CTE RAM is NOT guessable — every perf run goes through `measure.rs`.

use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, TransactionTrait,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

static WALK_TABLE_ID: AtomicU64 = AtomicU64::new(0);

fn statement(sql: String) -> Statement {
    Statement::from_string(DatabaseBackend::Sqlite, sql)
}

const REACH_CTE: &str = "WITH RECURSIVE reach(src,dst) AS (\
    SELECT parent_key,child_key FROM cx_dep \
    UNION \
    SELECT reach.src, dep.child_key FROM reach \
    JOIN cx_dep dep ON dep.parent_key = reach.dst\
)";

/// Condensation, all component ids expressed as MIN-member representative keys.
pub struct Condensed {
    pub comp_of: Vec<(i64, i64)>, // (node_key, comp_repr)
    pub size: Vec<(i64, i64)>,    // (comp_repr, member_count)
    pub cyclic: Vec<(i64, bool)>, // (comp_repr, is_cyclic)
    pub cadj: Vec<(i64, i64)>,    // (parent_comp_repr, child_comp_repr), deduped, no self
}

/// Forward transitive closure from `start` (strict; includes start iff its SCC is cyclic).
pub async fn reaches_from(db: &DatabaseConnection, start: i64) -> Result<Vec<i64>, DbErr> {
    let sql = format!(
        "WITH RECURSIVE reach(key) AS (\
            SELECT child_key FROM cx_dep WHERE parent_key = {start} \
            UNION \
            SELECT dep.child_key FROM cx_dep dep JOIN reach ON dep.parent_key = reach.key\
        ) SELECT key FROM reach ORDER BY key"
    );
    db.query_all_raw(statement(sql))
        .await?
        .iter()
        .map(|row| row.try_get_by_index::<i64>(0))
        .collect()
}

/// Reverse transitive closure into `target` (rides ix_cx_dep_child).
pub async fn reached_by(db: &DatabaseConnection, target: i64) -> Result<Vec<i64>, DbErr> {
    let sql = format!(
        "WITH RECURSIVE reach(key) AS (\
            SELECT parent_key FROM cx_dep WHERE child_key = {target} \
            UNION \
            SELECT dep.parent_key FROM cx_dep dep JOIN reach ON dep.child_key = reach.key\
        ) SELECT key FROM reach ORDER BY key"
    );
    db.query_all_raw(statement(sql))
        .await?
        .iter()
        .map(|row| row.try_get_by_index::<i64>(0))
        .collect()
}

/// Multi-source min-depth BFS. See INSIGHTS §3 for halt/depth_cap semantics.
pub async fn multi_source_walk(
    db: &DatabaseConnection,
    starts: &[(i64, i64, i64)],
    halt: Option<&[i64]>,
    depth_cap: Option<i64>,
) -> Result<Vec<(i64, i64, i64)>, DbErr> {
    let table_id = WALK_TABLE_ID.fetch_add(1, Ordering::Relaxed);
    let reached_table = format!("_reached_{table_id}");
    let halt_table = format!("_halt_{table_id}");
    let txn = db.begin().await?;

    txn.execute_unprepared(&format!(
        "CREATE TEMP TABLE {reached_table} (\
            tag INTEGER NOT NULL, node INTEGER NOT NULL, depth INTEGER NOT NULL, round INTEGER NOT NULL,\
            PRIMARY KEY(tag,node)\
        )"
    ))
    .await?;
    txn.execute_unprepared(&format!("CREATE TEMP TABLE {halt_table} (node INTEGER PRIMARY KEY)"))
        .await?;

    if let Some(halt_nodes) = halt {
        for &node in halt_nodes {
            txn.execute_unprepared(&format!("INSERT OR IGNORE INTO {halt_table} VALUES ({node})"))
                .await?;
        }
    }

    let mut ordered_starts = starts.to_vec();
    ordered_starts.sort_unstable();
    for (tag, node, depth) in ordered_starts {
        txn.execute_unprepared(&format!(
            "INSERT OR IGNORE INTO {reached_table}(tag,node,depth,round) VALUES ({tag},{node},{depth},0)"
        ))
        .await?;
    }

    let mut round = 0i64;
    loop {
        let expand_guard = match depth_cap {
            Some(cap) => format!(" AND reached.depth < {cap}"),
            None => String::new(),
        };
        txn.execute_unprepared(&format!(
            "INSERT OR IGNORE INTO {reached_table}(tag,node,depth,round) \
             SELECT reached.tag, dep.child_key, reached.depth + 1, {} \
             FROM {reached_table} reached JOIN cx_dep dep ON dep.parent_key = reached.node \
             WHERE reached.round = {round} \
             AND NOT EXISTS (SELECT 1 FROM {halt_table} halt WHERE halt.node = reached.node){}",
            round + 1,
            expand_guard,
        ))
        .await?;

        let inserted = txn
            .query_one_raw(statement(format!(
                "SELECT count(*) FROM {reached_table} WHERE round = {}",
                round + 1
            )))
            .await?
            .map(|row| row.try_get_by_index::<i64>(0))
            .transpose()?
            .unwrap_or(0);
        if inserted == 0 {
            break;
        }
        round += 1;
    }

    let rows = txn
        .query_all_raw(statement(format!(
            "SELECT tag,node,depth FROM {reached_table} ORDER BY tag,node"
        )))
        .await?;
    let result = rows
        .iter()
        .map(|row| {
            Ok((
                row.try_get_by_index::<i64>(0)?,
                row.try_get_by_index::<i64>(1)?,
                row.try_get_by_index::<i64>(2)?,
            ))
        })
        .collect::<Result<Vec<_>, DbErr>>()?;
    txn.execute_unprepared(&format!("DROP TABLE {reached_table}; DROP TABLE {halt_table}"))
        .await?;
    txn.commit().await?;
    Ok(result)
}

/// halt-only, depth-agnostic special case of `multi_source_walk`.
pub async fn multi_source_halt_bfs(
    db: &DatabaseConnection,
    starts: &[(i64, i64)],
    halt: &[i64],
) -> Result<Vec<(i64, i64)>, DbErr> {
    let starts3: Vec<(i64, i64, i64)> = starts.iter().map(|&(t, n)| (t, n, 0)).collect();
    Ok(multi_source_walk(db, &starts3, Some(halt), None)
        .await?
        .into_iter()
        .map(|(t, n, _)| (t, n))
        .collect())
}

/// SCC partition as (node_key, comp_repr = MIN member key). Compare on the partition.
pub async fn scc_labels(db: &DatabaseConnection) -> Result<Vec<(i64, i64)>, DbErr> {
    let sql = format!(
        "{REACH_CTE} \
         SELECT node.key, COALESCE(MIN(CASE WHEN backward.src IS NOT NULL THEN forward.dst END), node.key) \
         FROM cx_row node \
         LEFT JOIN reach forward ON forward.src = node.key \
         LEFT JOIN reach backward ON backward.src = forward.dst AND backward.dst = node.key \
         GROUP BY node.key ORDER BY node.key"
    );
    db.query_all_raw(statement(sql))
        .await?
        .iter()
        .map(|row| Ok((row.try_get_by_index::<i64>(0)?, row.try_get_by_index::<i64>(1)?)))
        .collect()
}

/// Condensation derived from `scc_labels` + cx_dep group-bys.
pub async fn build_condensed(db: &DatabaseConnection) -> Result<Condensed, DbErr> {
    let comp_of = scc_labels(db).await?;
    let repr_by_node: BTreeMap<i64, i64> = comp_of.iter().copied().collect();
    let mut member_counts: BTreeMap<i64, i64> = BTreeMap::new();
    for &(_, repr) in &comp_of {
        *member_counts.entry(repr).or_default() += 1;
    }

    let edges = db
        .query_all_raw(statement("SELECT parent_key,child_key FROM cx_dep".to_owned()))
        .await?;
    let mut self_loops = BTreeSet::new();
    let mut condensed_edges = BTreeSet::new();
    for edge in &edges {
        let parent = edge.try_get_by_index::<i64>(0)?;
        let child = edge.try_get_by_index::<i64>(1)?;
        let parent_repr = repr_by_node[&parent];
        let child_repr = repr_by_node[&child];
        if parent == child {
            self_loops.insert(parent_repr);
        }
        if parent_repr != child_repr {
            condensed_edges.insert((parent_repr, child_repr));
        }
    }

    let size: Vec<(i64, i64)> = member_counts.iter().map(|(&repr, &count)| (repr, count)).collect();
    let cyclic = member_counts
        .iter()
        .map(|(&repr, &count)| (repr, count > 1 || self_loops.contains(&repr)))
        .collect();
    Ok(Condensed {
        comp_of,
        size,
        cyclic,
        cadj: condensed_edges.into_iter().collect(),
    })
}

/// Reachable ordered-pair count; matches scc::count_pairs. i128 (exceeds i64 at scale).
pub async fn count_pairs(db: &DatabaseConnection) -> Result<i128, DbErr> {
    let row = db
        .query_one_raw(statement(format!("{REACH_CTE} SELECT count(*) FROM reach")))
        .await?;
    Ok(row
        .map(|result| result.try_get_by_index::<i64>(0))
        .transpose()?
        .unwrap_or(0) as i128)
}
