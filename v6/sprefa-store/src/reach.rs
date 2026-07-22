//! The v5 graph covering set, on-disk over `cx_dep(parent_key, child_key)`.
//!
//! Each function here has a resident pure-Rust ORACLE in the repo root
//! (`src/graph/scc.rs`, `src/graph/walk.rs`) and MUST agree with it byte-for-byte
//! (partition-canonical for SCC). `tests/covering.rs` is the standing check.
//!
//! FROZEN CONTRACT + exact inclusion semantics: `v6/findings/INSIGHTS.md` §3, §A.
//! Recursive-CTE RAM is NOT guessable — every perf run goes through `measure.rs`.

use sea_orm::{DatabaseConnection, DbErr};

/// Condensation, all component ids expressed as MIN-member representative keys.
pub struct Condensed {
    pub comp_of: Vec<(i64, i64)>, // (node_key, comp_repr)
    pub size: Vec<(i64, i64)>,    // (comp_repr, member_count)
    pub cyclic: Vec<(i64, bool)>, // (comp_repr, is_cyclic)
    pub cadj: Vec<(i64, i64)>,    // (parent_comp_repr, child_comp_repr), deduped, no self
}

/// Forward transitive closure from `start` (strict; includes start iff its SCC is cyclic).
pub async fn reaches_from(_db: &DatabaseConnection, _start: i64) -> Result<Vec<i64>, DbErr> {
    todo!("job A: WITH RECURSIVE forward over cx_dep, seed from start's out-neighbors")
}

/// Reverse transitive closure into `target` (rides ix_cx_dep_child).
pub async fn reached_by(_db: &DatabaseConnection, _target: i64) -> Result<Vec<i64>, DbErr> {
    todo!("job A: WITH RECURSIVE reverse over cx_dep child->parent")
}

/// Multi-source min-depth BFS. See INSIGHTS §3 for halt/depth_cap semantics.
pub async fn multi_source_walk(
    _db: &DatabaseConnection,
    _starts: &[(i64, i64, i64)],
    _halt: Option<&[i64]>,
    _depth_cap: Option<i64>,
) -> Result<Vec<(i64, i64, i64)>, DbErr> {
    todo!("job A: recursive CTE carrying MIN(depth), halt filter, cap guard")
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
pub async fn scc_labels(_db: &DatabaseConnection) -> Result<Vec<(i64, i64)>, DbErr> {
    todo!("job A: SCC via forward∩reverse reach (lab method, small shapes)")
}

/// Condensation derived from `scc_labels` + cx_dep group-bys.
pub async fn build_condensed(_db: &DatabaseConnection) -> Result<Condensed, DbErr> {
    todo!("job A: derive size/cyclic/cadj from scc_labels")
}

/// Reachable ordered-pair count; matches scc::count_pairs. i128 (exceeds i64 at scale).
pub async fn count_pairs(_db: &DatabaseConnection) -> Result<i128, DbErr> {
    todo!("job A: COUNT over the reach relation incl cyclic self-pairs")
}
