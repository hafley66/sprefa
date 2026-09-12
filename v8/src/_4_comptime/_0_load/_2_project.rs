//! `install_project_graph/6`. Port of
//! `v7/src/2_comptime/0b_filesystem_grapher.pl`.
//!
//! All or nothing: one diagnostic and the caller's basements and origins come
//! back untouched (`:38-39`, `:52-54`).

use super::api::{atom_text, diagnostic, owner_index, parts, sorted, Installed};
use super::paths::{
    directory_file_path, file_base_name, file_directory_name, file_stem_for_extension,
    relative_file_name,
};
use crate::_6_eval::term::{TermId, Universe};

/// `filesystem_claim(Owner, Label, Target, SourcePath)` before it is a term.
pub struct Claim {
    pub owner: TermId,
    pub label: TermId,
    pub target: TermId,
    pub source: TermId,
}

/// `:17`. `cwd` feeds `absolute_file_name/2` inside `relative_file_name/3`.
pub fn install_project_graph(
    u: &mut Universe,
    cwd: &str,
    project: TermId,
    basements: &[TermId],
    origins: &[TermId],
) -> Result<Installed, String> {
    let project_parts =
        parts(u, project, "dl7_project", 2).ok_or("project is not dl7_project/2")?;
    let root = project_parts[0];
    let root_text = atom_text(u, root)
        .ok_or("project root is not an atom")?
        .to_string();
    let units = u.as_list(project_parts[1]).ok_or("project units")?;

    let mut claims: Vec<Claim> = Vec::new();
    let mut directory_owners: Vec<TermId> = Vec::new();
    let mut file_owners: Vec<TermId> = Vec::new();
    let mut diagnostics: Vec<TermId> = Vec::new();
    for unit in &units {
        unit_path_claim(
            u,
            cwd,
            *unit,
            root,
            &root_text,
            &mut claims,
            &mut directory_owners,
            &mut file_owners,
            &mut diagnostics,
        );
    }

    if !diagnostics.is_empty() {
        return Ok(Installed {
            basements: basements.to_vec(),
            origins: origins.to_vec(),
            diagnostics,
        });
    }

    let claims = unique_claims(u, claims);
    let (edges, edge_origins) = indexed_claims(u, &claims);
    let root_owner = directory_owner(u, root);
    directory_owners.push(root_owner);
    let directory_owners = sorted(u, directory_owners);
    let file_owners = sorted(u, file_owners);
    let nodes = filesystem_nodes(u, &directory_owners, &file_owners);

    // :35. The project root owns the whole filesystem graph.
    let node_list = u.list(&nodes);
    let edge_list = u.list(&edges);
    let graph = u.compound("root_graph", vec![node_list, edge_list]);
    let empty = u.empty_list();
    let datalog = u.compound("datalog_program", vec![empty, empty, empty]);
    let program = u.compound("basement_program", vec![graph, datalog]);
    let basement = u.compound("module_basement", vec![root_owner, program]);
    let origin_list = u.list(&edge_origins);
    let module_origins = u.compound("module_origins", vec![root_owner, origin_list]);

    let mut out_basements = vec![basement];
    out_basements.extend_from_slice(basements);
    let mut out_origins = vec![module_origins];
    out_origins.extend_from_slice(origins);
    Ok(Installed {
        basements: out_basements,
        origins: out_origins,
        diagnostics: vec![],
    })
}

#[allow(clippy::too_many_arguments)]
fn unit_path_claim(
    u: &mut Universe,
    cwd: &str,
    unit: TermId,
    root: TermId,
    root_text: &str,
    claims: &mut Vec<Claim>,
    directory_owners: &mut Vec<TermId>,
    file_owners: &mut Vec<TermId>,
    diagnostics: &mut Vec<TermId>,
) {
    let Some(unit_parts) = parts(u, unit, "dl7_unit", 5) else {
        // :93. No origin to name, so the subject is the atom none.
        let payload = u.compound("invalid_project_unit", vec![unit]);
        let none = u.atom("none");
        let row = diagnostic(u, "module", none, payload);
        diagnostics.push(row);
        return;
    };
    let origin = unit_parts[0];
    let Some(path) = u.unary(origin, "file") else {
        // :89.
        let payload = u.atom("project_unit_without_file_origin");
        let row = diagnostic(u, "module", origin, payload);
        diagnostics.push(row);
        return;
    };
    // v7 hands whatever sits inside file/1 straight to relative_file_name/3,
    // which throws on a non-atom. Nothing in the corpus reaches it.
    let path_text = atom_text(u, path).unwrap_or("").to_string();
    let relative = relative_file_name(cwd, &path_text, root_text);
    if relative == ".." || relative.starts_with("../") {
        // :74.
        let payload = u.compound("outside_project_root", vec![root, path]);
        let subject = u.compound("filesystem", vec![path]);
        let row = diagnostic(u, "module", subject, payload);
        diagnostics.push(row);
        return;
    }
    let Some(segments) = module_path_segments(&relative) else {
        // :85.
        let relative_atom = u.atom(&relative);
        let payload = u.compound("invalid_dl7_module_path", vec![relative_atom]);
        let subject = u.compound("filesystem", vec![path]);
        let row = diagnostic(u, "module", subject, payload);
        diagnostics.push(row);
        return;
    };
    let root_owner = directory_owner(u, root);
    let file_owner = file_owner(u, path);
    path_claims(
        u,
        &segments,
        root_text,
        &[],
        root_owner,
        file_owner,
        path,
        &path_text,
        claims,
        directory_owners,
    );
    file_owners.push(file_owner);
}

/// `:114-120`. The last part must carry the `dl7` extension and a non-empty
/// stem; every part then loses its author prefix.
pub fn module_path_segments(relative: &str) -> Option<Vec<String>> {
    let mut parts: Vec<&str> = relative.split('/').collect();
    let file_name = parts.pop()?;
    let stem = file_stem_for_extension(file_name, "dl7")?;
    if stem.is_empty() {
        return None;
    }
    let mut raw: Vec<&str> = parts;
    raw.push(stem);
    Some(raw.iter().map(|part| semantic_segment(part)).collect())
}

/// `:122-134`. A leading run of decimals then `_` then a non-empty rest is an
/// author prefix. `append/3` walks the splits left to right, so only the first
/// `_` can ever match: a later one has the earlier `_` in its prefix.
pub fn semantic_segment(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let Some(cut) = bytes.iter().position(|b| *b == b'_') else {
        return raw.to_string();
    };
    if cut == 0 || cut + 1 >= bytes.len() {
        return raw.to_string();
    }
    if bytes[..cut].iter().all(|b| b.is_ascii_digit()) {
        raw[cut + 1..].to_string()
    } else {
        raw.to_string()
    }
}

#[allow(clippy::too_many_arguments)]
fn path_claims(
    u: &mut Universe,
    segments: &[String],
    root_text: &str,
    actual_prefix: &[String],
    parent_owner: TermId,
    file_owner: TermId,
    source: TermId,
    source_text: &str,
    claims: &mut Vec<Claim>,
    directory_owners: &mut Vec<TermId>,
) {
    let Some((label, rest)) = segments.split_first() else {
        return;
    };
    let label_atom = u.atom(label);
    if rest.is_empty() {
        // :136. The last segment names the file product itself.
        claims.push(Claim {
            owner: parent_owner,
            label: label_atom,
            target: file_owner,
            source,
        });
        return;
    }
    // :154-166. The on-disk spelling comes from the source file's own chain.
    let actual_name = directory_ancestor_name(file_directory_name(source_text), rest.len());
    let mut child_prefix = actual_prefix.to_vec();
    child_prefix.push(actual_name.to_string());
    let directory_path = directory_path(root_text, &child_prefix);
    let directory_atom = u.atom(&directory_path);
    let directory = directory_owner(u, directory_atom);
    claims.push(Claim {
        owner: parent_owner,
        label: label_atom,
        target: directory,
        source,
    });
    directory_owners.push(directory);
    path_claims(
        u,
        rest,
        root_text,
        &child_prefix,
        directory,
        file_owner,
        source,
        source_text,
        claims,
        directory_owners,
    );
}

fn directory_ancestor_name(directory: &str, remaining: usize) -> &str {
    let mut here = directory;
    for _ in 1..remaining {
        here = file_directory_name(here);
    }
    file_base_name(here)
}

fn directory_path(root: &str, segments: &[String]) -> String {
    let mut here = root.to_string();
    for segment in segments {
        here = directory_file_path(&here, segment);
    }
    here
}

fn directory_owner(u: &mut Universe, path: TermId) -> TermId {
    let inner = u.compound("directory", vec![path]);
    u.compound("module", vec![inner])
}

fn file_owner(u: &mut Universe, path: TermId) -> TermId {
    let inner = u.compound("file", vec![path]);
    u.compound("module", vec![inner])
}

/// `:45`, `:176-187`. `sort/2` first, so claims that differ only in their
/// source path are adjacent and the first in standard order survives.
fn unique_claims(u: &mut Universe, claims: Vec<Claim>) -> Vec<Claim> {
    let mut terms: Vec<TermId> = claims
        .iter()
        .map(|claim| {
            u.compound(
                "filesystem_claim",
                vec![claim.owner, claim.label, claim.target, claim.source],
            )
        })
        .collect();
    terms.sort_by(|a, b| u.cmp(*a, *b));
    terms.dedup();
    let mut out: Vec<Claim> = Vec::new();
    for term in terms {
        let args = parts(u, term, "filesystem_claim", 4).expect("claim shape");
        let same = out.last().is_some_and(|last| {
            last.owner == args[0] && last.label == args[1] && last.target == args[2]
        });
        if !same {
            out.push(Claim {
                owner: args[0],
                label: args[1],
                target: args[2],
                source: args[3],
            });
        }
    }
    out
}

/// `:189-204`.
fn indexed_claims(u: &mut Universe, claims: &[Claim]) -> (Vec<TermId>, Vec<TermId>) {
    let mut edges = Vec::with_capacity(claims.len());
    let mut origins = Vec::with_capacity(claims.len());
    let mut previous: Option<TermId> = None;
    let mut running = 0;
    for claim in claims {
        let index = owner_index(claim.owner, &mut previous, &mut running);
        let index_term = u.int(index);
        let target = u.compound("target", vec![claim.target]);
        edges.push(u.compound(
            "pending_edge",
            vec![claim.owner, claim.label, target, index_term],
        ));
        let edge = u.compound("edge", vec![claim.owner, claim.label, index_term]);
        let source = u.compound("filesystem", vec![claim.source]);
        origins.push(u.compound("origin", vec![edge, source]));
    }
    (edges, origins)
}

/// `:206-218`.
fn filesystem_nodes(
    u: &mut Universe,
    directory_owners: &[TermId],
    file_owners: &[TermId],
) -> Vec<TermId> {
    let mut nodes = Vec::new();
    for owner in directory_owners {
        nodes.push(u.compound("node", vec![*owner]));
        nodes.push(u.compound("module", vec![*owner]));
        nodes.push(u.compound("product", vec![*owner]));
    }
    for owner in file_owners {
        nodes.push(u.compound("product", vec![*owner]));
    }
    nodes
}
