//! MANIFEST-TO-MANIFEST dependency edges, one arm per manifest kind.
//! @comment-ok: the header states the two laws the arms below cannot show
//!
//! THE KEY IS A PATH, NOT A PACKAGE NAME. v5's `crate_edge`
//! (`src/graph/modgraph/rust.rs:468`) keyed on crate names, which needs a
//! second dictionary to reach a file and cannot express two packages of the
//! same name in one workspace. `file_edge` already keys on project-relative
//! paths, so keying this grain the same way lets the two join directly.
//!
//! WORKSPACE-INTERNAL ONLY. A dependency is an edge when its name is another
//! SUPPLIED manifest's own package name; everything else is a registry package
//! and has no manifest in the corpus to point at. The supplied path list is the
//! whole universe, exactly as `crate::deps` treats its own.

use std::collections::{BTreeMap, BTreeSet};

use gomod_parser::{GoMod, Replacement};

use crate::deps::{join_relative, project_relative};
use crate::project::{sorted_lines, ProjectError, ResolveRequest};
use crate::types::FlatFact;

/// Which manifest a path is, by file name. Any other name is skipped, never an
/// error: a caller may hand the whole corpus over.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ManifestKind {
    Cargo,
    Npm,
    GoMod,
}

impl ManifestKind {
    pub fn of_path(path: &str) -> Option<Self> {
        match path.rsplit_once('/').map(|(_, name)| name).unwrap_or(path) {
            "Cargo.toml" => Some(Self::Cargo),
            "package.json" => Some(Self::Npm),
            "go.mod" => Some(Self::GoMod),
            _ => None,
        }
    }
}

/// One manifest as the fold reads it: project-relative path, kind, raw text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub path: String,
    pub kind: ManifestKind,
    pub text: String,
}

/// Package name -> the manifest that declares it, per ecosystem. The kind rides
/// the key so a Cargo crate and an npm package of one name stay two nodes.
type PackageNames = BTreeMap<(ManifestKind, String), String>;

/// Fold supplied manifests into workspace-internal `package_edge` rows.
///
/// An UNPARSEABLE manifest contributes no name and no edges, which under-reports
/// and never mis-reports: the same degradation direction `TsconfigPaths` takes.
// @comment-ok: the degradation rule is a policy the signature cannot show
pub fn fold_package_edges(manifests: &[Manifest]) -> Vec<FlatFact> {
    let names = package_names(manifests);
    let paths: BTreeSet<&str> = manifests.iter().map(|one| one.path.as_str()).collect();
    let mut edges: BTreeSet<(&str, String, &'static str)> = BTreeSet::new();
    for manifest in manifests {
        let arm = match manifest.kind {
            ManifestKind::Cargo => cargo_edges(manifest, &names),
            ManifestKind::Npm => npm_edges(manifest, &names),
            ManifestKind::GoMod => gomod_edges(manifest, &names, &paths),
        };
        for (destination, kind) in arm {
            if destination != manifest.path {
                edges.insert((manifest.path.as_str(), destination, kind));
            }
        }
    }
    edges
        .into_iter()
        .map(|(source, destination, kind)| FlatFact::PackageEdgeRow {
            src_manifest: source.to_string(),
            dst_manifest: destination,
            kind: kind.to_string(),
        })
        .collect()
}

/// Each manifest's own package name. A manifest with no name (a Cargo virtual
/// workspace root) declares nothing and is no edge target.
fn package_names(manifests: &[Manifest]) -> PackageNames {
    let mut names = PackageNames::new();
    for manifest in manifests {
        let declared = match manifest.kind {
            ManifestKind::Cargo => cargo_value(&manifest.text)
                .and_then(|value| json_string(&value, &["package", "name"])),
            ManifestKind::Npm => {
                npm_value(&manifest.text).and_then(|value| json_string(&value, &["name"]))
            }
            ManifestKind::GoMod => go_value(&manifest.text).map(|parsed| parsed.module),
        };
        if let Some(name) = declared {
            names.insert((manifest.kind, name), manifest.path.clone());
        }
    }
    names
}

/// Cargo arm. `package = "..."` inside a dependency spec is the RENAME form, so
/// the code name is the local alias and the package name is the edge target.
fn cargo_edges(manifest: &Manifest, names: &PackageNames) -> Vec<(String, &'static str)> {
    let Some(value) = cargo_value(&manifest.text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (section, kind) in [
        ("dependencies", "normal"),
        ("dev-dependencies", "dev"),
        ("build-dependencies", "build"),
    ] {
        let Some(table) = value.get(section).and_then(serde_json::Value::as_object) else {
            continue;
        };
        for (code, spec) in table {
            let dependency = spec
                .get("package")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(code);
            if let Some(path) = names.get(&(ManifestKind::Cargo, dependency.to_string())) {
                out.push((path.clone(), kind));
            }
        }
    }
    out
}

/// npm arm. `peerDependencies` is its own kind rather than folded into `normal`:
/// a peer edge states a host requirement, not an installed one.
fn npm_edges(manifest: &Manifest, names: &PackageNames) -> Vec<(String, &'static str)> {
    let Some(value) = npm_value(&manifest.text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (section, kind) in [
        ("dependencies", "normal"),
        ("devDependencies", "dev"),
        ("peerDependencies", "peer"),
    ] {
        let Some(table) = value.get(section).and_then(serde_json::Value::as_object) else {
            continue;
        };
        for dependency in table.keys() {
            if let Some(path) = names.get(&(ManifestKind::Npm, dependency.clone())) {
                out.push((path.clone(), kind));
            }
        }
    }
    out
}

/// go.mod arm. A `replace` naming a DIRECTORY is resolved lexically against the
/// manifest's own directory, so the edge lands on that directory's `go.mod`.
fn gomod_edges(
    manifest: &Manifest,
    names: &PackageNames,
    paths: &BTreeSet<&str>,
) -> Vec<(String, &'static str)> {
    let Some(parsed) = go_value(&manifest.text) else {
        return Vec::new();
    };
    let directory = manifest
        .path
        .rsplit_once('/')
        .map(|(head, _)| head)
        .unwrap_or("");
    let mut out = Vec::new();
    for dependency in &parsed.require {
        if let Some(path) = names.get(&(ManifestKind::GoMod, dependency.module.module_path.clone()))
        {
            out.push((path.clone(), "require"));
        }
    }
    for replacement in &parsed.replace {
        match &replacement.replacement {
            Replacement::FilePath(text) if !text.starts_with('/') => {
                let candidate = format!("{}/go.mod", join_relative(directory, text));
                if paths.contains(candidate.as_str()) {
                    out.push((candidate, "replace"));
                }
            }
            Replacement::FilePath(_) => {}
            Replacement::Module(module) => {
                if let Some(path) = names.get(&(ManifestKind::GoMod, module.module_path.clone())) {
                    out.push((path.clone(), "replace"));
                }
            }
        }
    }
    out
}

/// A Cargo manifest as JSON. basic-toml over the whole file, so a `[package]`
/// header and a dotted key read the same.
fn cargo_value(text: &str) -> Option<serde_json::Value> {
    basic_toml::from_str::<serde_json::Value>(text).ok()
}

fn npm_value(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(text).ok()
}

fn go_value(text: &str) -> Option<GoMod> {
    text.parse::<GoMod>().ok()
}

/// A string at a key path, absent when any hop is missing or not a string.
fn json_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    let mut cursor = value;
    for key in keys {
        cursor = cursor.get(key)?;
    }
    cursor.as_str().map(str::to_string)
}

/// Read the supplied manifests and fold them to `package_edge` rows. Paths that
/// are not manifests are skipped; a path outside the root is a named error.
pub fn package_edges(request: &ResolveRequest) -> Result<Vec<FlatFact>, ProjectError> {
    let Some(root) = request.project_root else {
        return Err(ProjectError::ManifestsNeedRoot);
    };
    let root_absolute =
        std::fs::canonicalize(root).map_err(|err| ProjectError::Read(root.to_path_buf(), err))?;
    let mut manifests = Vec::new();
    for path in request.paths {
        let relative = project_relative(&path.to_string_lossy(), &root_absolute)?;
        let Some(kind) = ManifestKind::of_path(&relative) else {
            continue;
        };
        let text =
            std::fs::read_to_string(path).map_err(|err| ProjectError::Read(path.clone(), err))?;
        manifests.push(Manifest {
            path: relative,
            kind,
            text,
        });
    }
    Ok(fold_package_edges(&manifests))
}

/// Serialize package edges to sorted JSONL lines.
pub fn package_edges_jsonl(request: &ResolveRequest) -> Result<Vec<String>, ProjectError> {
    Ok(sorted_lines(package_edges(request)?))
}
