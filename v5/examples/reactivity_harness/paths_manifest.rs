use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub(crate) struct Args {
    fixture_root: PathBuf,
    fixture_manifest: PathBuf,
    database: PathBuf,
    temp_dir: PathBuf,
    output_dir: PathBuf,
    schema: PathBuf,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FixtureManifest {
    pub(crate) schema_version: u64,
    pub(crate) fixture_seed: String,
    pub(crate) fixture_files: usize,
    pub(crate) fixture_bytes: u64,
    pub(crate) fixture_digest: String,
    pub(crate) files: Vec<FixtureEntry>,
    pub(crate) edit: FixtureEdit,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FixtureEntry {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FixtureEdit {
    path: String,
    before_sha256: String,
    after_sha256: String,
    before: String,
    after: String,
}

pub(crate) struct ValidatedPaths {
    pub(crate) fixture_root: PathBuf,
    pub(crate) fixture_manifest: PathBuf,
    pub(crate) database: PathBuf,
    pub(crate) temp_dir: PathBuf,
    pub(crate) output_dir: PathBuf,
    pub(crate) schema: PathBuf,
    pub(crate) scratch_ancestor: PathBuf,
}

pub(crate) fn parse_args() -> Result<Args> {
    let mut values = BTreeMap::<String, PathBuf>::new();
    let mut args = env::args_os().skip(1);
    while let Some(flag) = args.next() {
        let flag = flag.to_string_lossy().into_owned();
        if !matches!(
            flag.as_str(),
            "--fixture-root"
                | "--fixture-manifest"
                | "--database"
                | "--temp-dir"
                | "--output-dir"
                | "--schema"
        ) {
            bail!("unknown argument {flag:?}");
        }
        let value = args.next().with_context(|| format!("missing value for {flag}"))?;
        if values.insert(flag.clone(), PathBuf::from(value)).is_some() {
            bail!("duplicate argument {flag}");
        }
    }
    let mut take = |name: &str| {
        values
            .remove(name)
            .with_context(|| format!("required argument {name} is missing"))
    };
    let parsed = Args {
        fixture_root: take("--fixture-root")?,
        fixture_manifest: take("--fixture-manifest")?,
        database: take("--database")?,
        temp_dir: take("--temp-dir")?,
        output_dir: take("--output-dir")?,
        schema: take("--schema")?,
    };
    if !values.is_empty() {
        bail!("unconsumed arguments: {:?}", values.keys().collect::<Vec<_>>());
    }
    Ok(parsed)
}

pub(crate) fn require_worker_contract() -> Result<()> {
    for (name, expected) in [("CARGO_BUILD_JOBS", "2"), ("DL_RAYON_THREADS", "2")] {
        let actual = env::var(name).with_context(|| format!("{name} must be set to {expected}"))?;
        if actual != expected {
            bail!("{name} must be {expected}, got {actual:?}");
        }
    }
    if env::var("SPREFA_REACTIVITY_DAEMON").as_deref() != Ok("disabled") {
        bail!("SPREFA_REACTIVITY_DAEMON=disabled is required");
    }
    Ok(())
}

pub(crate) fn validate_paths(args: Args) -> Result<ValidatedPaths> {
    let fixture_root = canonical_directory(&args.fixture_root, "fixture root")?;
    let fixture_manifest = canonical_file(&args.fixture_manifest, "fixture manifest")?;
    let temp_dir = canonical_directory(&args.temp_dir, "temp directory")?;
    let output_dir = canonical_directory(&args.output_dir, "artifact output directory")?;
    let schema = canonical_file(&args.schema, "schema")?;
    let scratch_ancestor = fixture_root
        .parent()
        .context("fixture root has no scratch ancestor")?
        .to_path_buf();
    require_descendant(&fixture_manifest, &scratch_ancestor, "fixture manifest")?;
    require_descendant(&temp_dir, &scratch_ancestor, "temp directory")?;
    let database = canonical_new_file(&args.database, "database")?;
    require_descendant(&database, &scratch_ancestor, "database")?;

    let cwd = env::current_dir()?.canonicalize()?;
    if cwd != fixture_root {
        bail!("working directory must equal fixture root: {} != {}", cwd.display(), fixture_root.display());
    }
    for name in ["TMPDIR", "SQLITE_TMPDIR"] {
        let raw = env::var_os(name).with_context(|| format!("{name} is required"))?;
        let path = canonical_directory(Path::new(&raw), name)?;
        require_descendant(&path, &scratch_ancestor, name)?;
    }
    Ok(ValidatedPaths {
        fixture_root,
        fixture_manifest,
        database,
        temp_dir,
        output_dir,
        schema,
        scratch_ancestor,
    })
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf> {
    reject_symlink_components(path)?;
    let canonical = path
        .canonicalize()
        .with_context(|| format!("{label} does not exist: {}", path.display()))?;
    if !canonical.is_dir() {
        bail!("{label} is not a directory: {}", canonical.display());
    }
    Ok(canonical)
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf> {
    reject_symlink_components(path)?;
    let canonical = path
        .canonicalize()
        .with_context(|| format!("{label} does not exist: {}", path.display()))?;
    if !canonical.is_file() {
        bail!("{label} is not a regular file: {}", canonical.display());
    }
    Ok(canonical)
}

pub(crate) fn canonical_new_file(path: &Path, label: &str) -> Result<PathBuf> {
    if path.exists() {
        bail!("{label} already exists: {}", path.display());
    }
    reject_symlink_components(path)?;
    let parent = path.parent().with_context(|| format!("{label} has no parent"))?;
    let parent = canonical_directory(parent, &format!("{label} parent"))?;
    let name = path.file_name().with_context(|| format!("{label} has no filename"))?;
    Ok(parent.join(name))
}

pub(crate) fn validate_new_file_under(path: &Path, ancestor: &Path, label: &str) -> Result<()> {
    let canonical = canonical_new_file(path, label)?;
    require_descendant(&canonical, ancestor, label)
}

fn reject_symlink_components(path: &Path) -> Result<()> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let mut current = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => current.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => bail!("parent traversal is forbidden: {}", path.display()),
            Component::Normal(part) => current.push(part),
        }
        if current.exists() && fs::symlink_metadata(&current)?.file_type().is_symlink() {
            bail!("symlinked path component is forbidden: {}", current.display());
        }
    }
    Ok(())
}

fn require_descendant(path: &Path, ancestor: &Path, label: &str) -> Result<()> {
    if path == ancestor || !path.starts_with(ancestor) {
        bail!("{label} escapes isolated scratch ancestor: {}", path.display());
    }
    Ok(())
}

pub(crate) fn load_and_validate_manifest(path: &Path, root: &Path) -> Result<FixtureManifest> {
    let bytes = fs::read(path)?;
    let manifest: FixtureManifest = serde_json::from_slice(&bytes).context("parse fixture manifest")?;
    if manifest.schema_version != 1 {
        bail!("unsupported fixture manifest schema {}", manifest.schema_version);
    }
    if !matches!(manifest.fixture_files, 10 | 100 | 1000) {
        bail!("fixture_files must be 10, 100, or 1000");
    }
    if manifest.files.len() != manifest.fixture_files {
        bail!("manifest file count does not match fixture_files");
    }
    validate_hex64(&manifest.fixture_digest, "fixture_digest")?;
    validate_safe_seed(&manifest.fixture_seed)?;
    validate_hex64(&manifest.edit.before_sha256, "edit.before_sha256")?;
    validate_hex64(&manifest.edit.after_sha256, "edit.after_sha256")?;

    let mut listed = BTreeSet::new();
    let mut total = 0u64;
    for entry in &manifest.files {
        validate_hex64(&entry.sha256, "file sha256")?;
        let relative = safe_relative(&entry.path)?;
        if !listed.insert(relative.clone()) {
            bail!("duplicate manifest path {}", entry.path);
        }
        let full = root.join(&relative);
        reject_symlink_components(&full)?;
        let metadata = fs::metadata(&full)
            .with_context(|| format!("manifest file is missing: {}", full.display()))?;
        if !metadata.is_file() || metadata.len() != entry.bytes {
            bail!("manifest size/type mismatch for {}", entry.path);
        }
        total = total.checked_add(metadata.len()).context("fixture byte total overflow")?;
    }
    if total != manifest.fixture_bytes {
        bail!("fixture byte total differs from manifest");
    }
    let actual = collect_files(&root.join("corpus"), root)?;
    if actual != listed {
        bail!("fixture corpus differs from the manifest file set");
    }
    let edit_relative = safe_relative(&manifest.edit.path)?;
    if !listed.contains(&edit_relative) {
        bail!("edit target is not a manifested corpus file");
    }
    let before = fs::read_to_string(root.join(edit_relative))?;
    if before != manifest.edit.before {
        bail!("edit target bytes do not equal manifest before content");
    }
    Ok(manifest)
}

fn collect_files(directory: &Path, root: &Path) -> Result<BTreeSet<PathBuf>> {
    reject_symlink_components(directory)?;
    let mut pending = vec![directory.to_path_buf()];
    let mut files = BTreeSet::new();
    while let Some(dir) = pending.pop() {
        let mut entries = fs::read_dir(&dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                bail!("fixture contains symlink: {}", entry.path().display());
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                files.insert(entry.path().strip_prefix(root)?.to_path_buf());
            } else {
                bail!("fixture contains non-regular entry: {}", entry.path().display());
            }
        }
    }
    Ok(files)
}

fn safe_relative(raw: &str) -> Result<PathBuf> {
    let path = Path::new(raw);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("manifest path is not a safe relative path: {raw:?}");
    }
    Ok(path.to_path_buf())
}

pub(crate) fn apply_manifest_edit(root: &Path, manifest: &FixtureManifest) -> Result<PathBuf> {
    let relative = safe_relative(&manifest.edit.path)?;
    let path = root.join(relative);
    reject_symlink_components(&path)?;
    if fs::read_to_string(&path)? != manifest.edit.before {
        bail!("refusing edit: target no longer matches manifest before content");
    }
    let mut file = OpenOptions::new().write(true).truncate(true).open(&path)?;
    file.write_all(manifest.edit.after.as_bytes())?;
    file.sync_all()?;
    drop(file);
    if fs::read_to_string(&path)? != manifest.edit.after {
        bail!("edited target does not match manifest after content");
    }
    path.canonicalize().context("canonicalize edited path")
}

pub(crate) fn refuse_database_family(path: &Path) -> Result<()> {
    for candidate in [path.to_path_buf(), wal_path(path), suffix_path(path, "-shm")] {
        if candidate.exists() {
            bail!("database artifact already exists: {}", candidate.display());
        }
    }
    Ok(())
}

pub(crate) fn wal_path(path: &Path) -> PathBuf {
    suffix_path(path, "-wal")
}

fn suffix_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

pub(crate) fn file_bytes(path: &Path) -> Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(metadata.len()),
        Ok(_) => bail!("expected regular file: {}", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn tree_bytes(root: &Path) -> Result<u64> {
    let mut total = 0u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                bail!("temporary tree contains symlink: {}", entry.path().display());
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                total = total.checked_add(entry.metadata()?.len()).context("temp byte total overflow")?;
            } else {
                bail!("temporary tree contains non-regular entry: {}", entry.path().display());
            }
        }
    }
    Ok(total)
}

pub(crate) fn path_text(path: &Path) -> Result<&str> {
    path.to_str().with_context(|| format!("path is not UTF-8: {}", path.display()))
}

fn validate_safe_seed(seed: &str) -> Result<()> {
    if seed.is_empty()
        || !seed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        bail!("fixture seed is not filename-safe");
    }
    Ok(())
}

fn validate_hex64(value: &str, label: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("{label} must be 64 lowercase hexadecimal characters");
    }
    Ok(())
}
