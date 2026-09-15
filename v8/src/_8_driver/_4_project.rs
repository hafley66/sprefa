//! `load_dl7_project/4` (`0_reader/4_module_loader.pl:29`), `load_tsi_streams/3`
//! (`2_compiler.pl:277`), `install_graphs/7` (`:601`).

use super::_0_read::{canonical_dir, program_text};
use super::_1_unit::file_unit;
use crate::_0_read::expand::Stop;
use crate::_2_lower::units::install_module_aliases;
use crate::_4_comptime::load::read::read_tsi_stream;
use crate::_4_comptime::load::{install_project_graph, install_tsi_graph, load_tsi_lines};
use crate::_6_eval::term::{TermId, Universe};
use std::collections::HashSet;
use std::path::Path;

/// `project(Project, TsiRows)` at `2_compiler.pl:580`, plus the working
/// directory `relative_file_name/3` reads.
pub struct Project {
    pub project: TermId,
    pub tsi_rows: Vec<TermId>,
    pub cwd: String,
}

pub struct Loaded {
    pub project: TermId,
    pub units: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

/// `4_module_loader.pl:29`. One unit per path, in path order.
pub fn load_dl7_project(
    u: &mut Universe,
    root: &Path,
    paths: &[&Path],
) -> Result<Result<Loaded, String>, Stop> {
    let canonical_root = match canonical_dir(root) {
        Ok(root) => root,
        Err(e) => return Ok(Err(format!("{}: {e}", root.display()))),
    };
    let mut units = Vec::with_capacity(paths.len());
    let mut diagnostics = Vec::new();
    for path in paths {
        let (canonical, text) = match program_text(path) {
            Ok(pair) => pair,
            Err(e) => return Ok(Err(format!("{}: {e}", path.display()))),
        };
        let unit = file_unit(u, &canonical, &text)?;
        units.push(unit.unit);
        diagnostics.extend(unit.diagnostics);
    }
    let root = u.atom(&canonical_root);
    let unit_list = u.list(&units);
    Ok(Ok(Loaded {
        project: u.compound("dl7_project", vec![root, unit_list]),
        units,
        diagnostics,
    }))
}

/// `:277`. Rows and diagnostics concatenated in stream order.
pub fn load_tsi_streams(
    u: &mut Universe,
    paths: &[&Path],
) -> Result<(Vec<TermId>, Vec<TermId>), String> {
    let mut rows = Vec::new();
    let mut diagnostics = Vec::new();
    for path in paths {
        let lines = read_tsi_stream(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let origin = u.atom(&path.display().to_string());
        let loaded = load_tsi_lines(u, origin, &lines);
        rows.extend(loaded.rows);
        diagnostics.extend(loaded.diagnostics);
    }
    Ok((rows, diagnostics))
}

/// Basements, origins, diagnostics: what every installer hands back.
pub type Installed = (Vec<TermId>, Vec<TermId>, Vec<TermId>);

/// `:601`. Foreign rows enter after the filesystem products.
pub fn install_graphs(
    u: &mut Universe,
    project: &Project,
    basements: &[TermId],
    origins: &[TermId],
) -> Result<Installed, String> {
    let installed = install_project_graph(u, &project.cwd, project.project, basements, origins)?;
    if !installed.diagnostics.is_empty() {
        return Ok((
            installed.basements,
            installed.origins,
            installed.diagnostics,
        ));
    }
    let before: HashSet<TermId> = owners(u, &installed.basements);
    let tsi = install_tsi_graph(
        u,
        &project.tsi_rows,
        &installed.basements,
        &installed.origins,
    );
    if !tsi.diagnostics.is_empty() {
        return Ok((tsi.basements, tsi.origins, tsi.diagnostics));
    }
    // :615. Every module owner the loaders added is aliased into every source
    // unit of the project.
    let Some(("dl7_project", args)) = u.functor(project.project) else {
        return Ok((tsi.basements, tsi.origins, Vec::new()));
    };
    let units = u.as_list(args[1]).unwrap_or_default();
    let importers: Vec<TermId> = units
        .iter()
        .filter_map(|unit| match u.functor(*unit) {
            Some(("dl7_unit", parts)) => Some(parts[0]),
            _ => None,
        })
        .collect();
    let importers: Vec<TermId> = importers
        .into_iter()
        .map(|origin| u.compound("module", vec![origin]))
        .collect();
    let exporters: Vec<TermId> = owner_order(u, &tsi.basements)
        .into_iter()
        .filter(|owner| !before.contains(owner))
        .collect();
    let mut basements = tsi.basements;
    let mut origins = tsi.origins;
    for exporter in exporters {
        let (b, o) = install_module_aliases(u, exporter, &importers, &basements, &origins);
        basements = b;
        origins = o;
    }
    Ok((basements, origins, Vec::new()))
}

fn owner_order(u: &Universe, basements: &[TermId]) -> Vec<TermId> {
    basements
        .iter()
        .filter_map(|row| match u.functor(*row) {
            Some(("module_basement", args)) => Some(args[0]),
            _ => None,
        })
        .collect()
}

fn owners(u: &Universe, basements: &[TermId]) -> HashSet<TermId> {
    owner_order(u, basements).into_iter().collect()
}
