use super::*;

impl Engine {
    /// Resolve a declared rev to a stable commit SHA (WORK stays WORK).
    /// Cached per tick so a moving ref is re-resolved each tick.
    pub(crate) fn resolve_rev(&mut self, repo_root: &Path, rev: &str) -> Result<String> {
        if rev == "WORK" { return Ok("WORK".to_string()); }
        // Cache by (repo, rev): the same tag resolves to different shas per repo.
        // Immutable (hex-SHA) revs live in the cross-tick cache; movable refs in
        // the per-tick one. A hit in either means we already have it — no spawn.
        let key = format!("{}::{rev}", repo_root.display());
        if let Some(s) = self.rev_sha_cache.get(&key) { return Ok(s.clone()); }
        if let Some(s) = self.rev_cache.get(&key) { return Ok(s.clone()); }
        // Present rev: unchanged fast path — rev-parse resolves, cache, return
        // the identical sha the caller has always seen.
        if let Some(sha) = Self::rev_parse(repo_root, rev)? {
            self.cache_rev(key, rev, sha.clone());
            return Ok(sha);
        }
        // Miss: the rev isn't in this repo's object db. Offline mode throws
        // without touching the network; otherwise fetch this specific rev
        // on demand and re-resolve exactly once. rev_cache still only records
        // a successful resolution, so a fetched rev is cached like any other.
        if std::env::var_os("DL_NO_FETCH").is_some() {
            bail!("git rev-parse {rev} missing in {} and DL_NO_FETCH is set (offline; would have fetched)",
                repo_root.display());
        }
        // Escalating on-demand fetch, cheapest first, re-resolving after each
        // and stopping at the first hit. rev-parse of a NAME needs the ref to
        // exist locally and the object to be present — a bare `fetch origin
        // <rev>` only writes FETCH_HEAD (lands a full SHA's object, but creates
        // no tag/branch ref), so a tag/branch name needs a step that writes the
        // ref, and a shallow clone whose target is beyond the boundary needs
        // history deepened:
        //   1. `origin <rev>`        — full-SHA fast path (object into the odb).
        //   2. `origin tag <rev>`    — creates refs/tags/<rev> (the tag case).
        //   3. `--tags origin`       — all tag refs (name that step 2 missed).
        //   4. `--unshallow --tags`  — shallow only: deepen full history + tags.
        let mut resolved: Option<String> = None;
        let ladder: [&[&str]; 3] = [
            &["fetch", "--quiet", "origin", rev],
            &["fetch", "--quiet", "origin", "tag", rev],
            &["fetch", "--quiet", "--tags", "origin"],
        ];
        for args in ladder {
            let _ = Command::new("git").arg("-C").arg(repo_root).args(args).output()?;
            if let Some(sha) = Self::rev_parse(repo_root, rev)? { resolved = Some(sha); break; }
        }
        if resolved.is_none() && Self::is_shallow(repo_root)? {
            let _ = Command::new("git").arg("-C").arg(repo_root)
                .args(["fetch", "--quiet", "--unshallow", "--tags", "origin"]).output()?;
            resolved = Self::rev_parse(repo_root, rev)?;
        }
        match resolved {
            Some(sha) => { self.cache_rev(key, rev, sha.clone()); Ok(sha) }
            None => bail!("git rev-parse {rev} still missing in {} after fetching tags/unshallowing from origin",
                repo_root.display()),
        }
    }

    /// `git rev-parse --is-shallow-repository` == "true". A shallow clone may be
    /// missing a target object even after a tag fetch (it lives below the
    /// shallow boundary), so `resolve_rev` deepens with `--unshallow` only when
    /// this holds — `--unshallow` errors on a complete repo.
    pub(crate) fn is_shallow(repo_root: &Path) -> Result<bool> {
        let out = Command::new("git").arg("-C").arg(repo_root)
            .args(["rev-parse", "--is-shallow-repository"]).output()?;
        Ok(out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true")
    }

    /// `git rev-parse <rev>` in `repo_root`: `Some(sha)` when the rev is present,
    /// `None` when it is missing — the signal `resolve_rev` uses to decide
    /// whether to fetch. Two miss shapes: an unknown ref/name (rev-parse exits
    /// non-zero), and a pinned full SHA whose object is absent (rev-parse echoes
    /// it back regardless, so a `cat-file -e` existence check is required). The
    /// returned value is the PLAIN rev-parse sha — unchanged for a present rev
    /// (an annotated tag stays the tag-object sha, not the peeled commit).
    /// Propagates only a spawn failure (git absent).
    pub(crate) fn rev_parse(repo_root: &Path, rev: &str) -> Result<Option<String>> {
        let out = Command::new("git").arg("-C").arg(repo_root)
            .args(["rev-parse", rev]).output()?;
        if !out.status.success() { return Ok(None); }
        let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
        // Only a hex-SHA rev echoes back from rev-parse WITHOUT proving the
        // object exists, so the existence probe is needed just there. A name
        // (tag/branch/HEAD) only rev-parses when its ref — hence object — is
        // present, so skip the second spawn for it.
        if Self::is_immutable_rev(rev) {
            let present = Command::new("git").arg("-C").arg(repo_root)
                .args(["cat-file", "-e", &sha]).output()?;
            if !present.status.success() { return Ok(None); }
        }
        Ok(Some(sha))
    }

    /// A rev whose object mapping can't change: a full or prefix hex SHA. Movable
    /// refs (branch/tag/HEAD names) are not hex. Gates both the cross-tick cache
    /// and the `cat-file` existence probe.
    pub(crate) fn is_immutable_rev(rev: &str) -> bool {
        rev.len() >= 7 && rev.chars().all(|c| c.is_ascii_hexdigit())
    }

    /// Record a resolution in the cross-tick cache for an immutable SHA, else the
    /// per-tick cache for a movable ref.
    pub(crate) fn cache_rev(&mut self, key: String, rev: &str, sha: String) {
        if Self::is_immutable_rev(rev) { self.rev_sha_cache.insert(key, sha); }
        else { self.rev_cache.insert(key, sha); }
    }

    /// Resolve a `scan` repo coordinate to `(slug, root)`. The slug is the
    /// repo's stable identity in the `_file` cache and the `repo`/`rev`/`file`
    /// relations (the third coordinate alongside path+rev). "." / "" / "self" =
    /// this engine's own repo (slug = root dir name); a config slug names that
    /// repo; otherwise an existing path (slug = its dir name). A config slug
    /// with `allow_missing = true` resolves even when its root is absent: the
    /// caller's `scan` walks a missing dir and gets zero rows.
    pub(crate) fn resolve_repo(&self, repo: &str) -> Result<(String, PathBuf)> {
        if repo.is_empty() || repo == "." || repo == "self" {
            return Ok((self.self_slug(), self.root.clone()));
        }
        if let Some(rc) = self.repos.iter().find(|r| r.slug == repo) {
            self.ensure_cloned_or_missing(rc)?;
            return Ok((rc.slug.clone(), rc.root.clone()));
        }
        let p = PathBuf::from(repo);
        if p.exists() {
            let slug = p.file_name().map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| repo.to_string());
            return Ok((slug, p));
        }
        bail!("unknown repo {repo:?} (expected \".\", a config slug, or an existing path)")
    }

    /// `ensure_cloned` with the `allow_missing` escape hatch. A configured repo
    /// whose root is absent AND `allow_missing = true` resolves to Ok: the
    /// engine prints one stderr line and `scan` against it returns zero rows.
    /// Without the flag, the underlying clone / missing-root error propagates.
    pub(crate) fn ensure_cloned_or_missing(&self, rc: &crate::config::RepoConfig) -> Result<()> {
        match Self::ensure_cloned(rc) {
            Ok(()) => Ok(()),
            Err(e) if rc.allow_missing && !rc.root.exists() => {
                eprintln!(
                    "[missing] repo {:?} (allow_missing); scan returns zero rows. details: {e}",
                    rc.slug,
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Materialize a configured repo whose `root` is not yet on disk by cloning
    /// its `url` (full clone — pinned `(repo, rev)` scans stay deterministic by
    /// OID once cloned). No-op when the root already exists; an error when the
    /// root is missing and no `url` is configured.
    pub fn ensure_cloned(rc: &crate::config::RepoConfig) -> Result<()> {
        if rc.root.exists() { return Ok(()); }
        let Some(url) = rc.url.as_deref() else {
            bail!("repo {:?} root {} does not exist and no url is configured to clone it",
                  rc.slug, rc.root.display());
        };
        if let Some(parent) = rc.root.parent() { std::fs::create_dir_all(parent)?; }
        eprintln!("[clone] {} <- {url}", rc.root.display());
        let out = Command::new("git").args(["clone", url]).arg(&rc.root).output()?;
        if !out.status.success() {
            bail!("git clone {url} into {} failed: {}", rc.root.display(),
                  String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }

    /// Expand a `scan` repo coordinate into the concrete `(slug, root)` set to
    /// scan this tick. `"*"` / `"all"` fans out over every configured repo (the
    /// config-folder query: one program, the whole repo set), cloning any that
    /// are not yet on disk; anything else resolves to a single repo. A repo
    /// marked `allow_missing = true` is included even when its root is absent;
    /// its `scan` returns zero rows.
    pub(crate) fn resolve_scan_repos(&self, repo: &str) -> Result<Vec<(String, PathBuf)>> {
        if repo == "*" || repo == "all" {
            if self.repos.is_empty() {
                return Ok(vec![(self.self_slug(), self.root.clone())]);
            }
            let mut out = Vec::with_capacity(self.repos.len());
            for rc in &self.repos {
                self.ensure_cloned_or_missing(rc)?;
                out.push((rc.slug.clone(), rc.root.clone()));
            }
            return Ok(out);
        }
        Ok(vec![self.resolve_repo(repo)?])
    }

    /// Resolve a source rule's scan to its concrete coordinate bindings: one for
    /// a literal-coord scan (the existing shape), or many for a data-driven scan
    /// whose `repo`/`rev` are `Term::Var`. In the variable case the rule's
    /// Pos/Neg/Cmp body atoms are compiled to a SELECT and run over the
    /// previous tick's tables (the coordinate relation is derived, and derived
    /// runs after source — so a data-driven scan reads last tick's coordinates,
    /// one-tick latency, no fixpoint rewrite). Each binding seeds `head_binds`
    /// with the variable slot's value so the rule head can reference the
    /// repo/rev each row was scanned under. Glob stays literal (variable glob is
    /// rejected): the file set varies per (repo, rev) but the pattern is fixed.
    #[tracing::instrument(skip_all, level = "debug")]
    pub(crate) fn resolve_scan_bindings(&mut self, rule: &Rule) -> Result<Vec<ScanBinding>> {
        let spec = scan_spec_of(rule)?;
        let glob = str_of(&spec.glob)?;
        let repo_var: Option<&str> = if let Term::Var(v) = &spec.repo { Some(v.as_str()) } else { None };
        let rev_var: Option<&str> = if let Term::Var(v) = &spec.rev { Some(v.as_str()) } else { None };
        tracing::debug!(head = %rule.head.rel, repo_var = ?repo_var, rev_var = ?rev_var,
            origin = ?rule.origin.as_ref().map(|p| p.to_string_lossy().to_string()), "scan bindings");
        if repo_var.is_none() && rev_var.is_none() {
            let repo_lit = str_of(&spec.repo)?;
            let rev_lit = str_of(&spec.rev)?;
            // A self-form scan (`scan("WORK", …)` / `.` / `""` / `self`) resolves
            // to the rule's own `.git` ancestor when `root_implicit` is set —
            // i.e. ONLY the rootless daemon, whose self.root is a placeholder.
            // Foreground (`--root`/cwd) and LSP (`rootUri`) pass an explicit root
            // that always wins; the script's location is irrelevant there.
            let repo_coord = if self.root_implicit && matches!(repo_lit.as_str(), "." | "" | "self") {
                rule.origin.as_deref()
                    .and_then(crate::repo::nearest_git)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or(repo_lit.clone())
            } else {
                repo_lit.clone()
            };
            tracing::debug!(head = %rule.head.rel, repo_coord = %repo_coord, rev_lit = %rev_lit, "resolved self-form scan");
            let mut out = Vec::new();
            for (slug, root) in self.resolve_scan_repos(&repo_coord)? {
                let rev = self.resolve_rev(&root, &rev_lit)?;
                out.push(ScanBinding { slug, root, rev, glob: glob.clone(), head_binds: vec![] });
            }
            return Ok(out);
        }
        let mut sel_vars: Vec<String> = Vec::new();
        if let Some(v) = repo_var { sel_vars.push(v.to_string()); }
        if let Some(v) = rev_var { sel_vars.push(v.to_string()); }
        let binding_atoms: Vec<BodyItem> = rule.body.iter()
            .filter(|b| matches!(b, BodyItem::Pos(_) | BodyItem::Neg(_) | BodyItem::Cmp(_)))
            .cloned().collect();
        if binding_atoms.is_empty() {
            bail!("data-driven scan needs a coordinate-providing body atom (e.g. \
                   pin(R,V)) binding the variable repo/rev in rule {}", rule.head.rel);
        }
        let sql = crate::lower::lower_gen(&sel_vars, &binding_atoms, &self.rels)?;
        // Collect the coordinate tuples fully (drop the statement borrow) before
        // the resolve loop: resolve_rev takes &mut self (rev_cache).
        let tuples: Vec<Vec<String>> = {
            let conn = self.db.conn();
            let mut s = match conn.prepare(&sql) {
                Ok(s) => s,
                Err(_) => return Ok(Vec::new()),
            };
            let ncol = sel_vars.len();
            let rows = s.query_map([], |r| {
                let mut v = Vec::with_capacity(ncol);
                for i in 0..ncol { v.push(r.get::<_, String>(i)?); }
                Ok(v)
            });
            rows.map(|iter| iter.filter_map(|x| x.ok()).collect()).unwrap_or_default()
        };
        let mut out = Vec::new();
        let repo_lit = str_of(&spec.repo).ok();
        let rev_lit = str_of(&spec.rev).ok();
        for row in tuples {
            let mut col = 0usize;
            let repo_val = if repo_var.is_some() { row[col].clone() } else { repo_lit.clone().unwrap() };
            if repo_var.is_some() { col += 1; }
            let rev_val = if rev_var.is_some() { row[col].clone() } else { rev_lit.clone().unwrap() };
            let mut head_binds: Vec<(String, String)> = Vec::new();
            if let Some(v) = repo_var { head_binds.push((v.to_string(), repo_val.clone())); }
            if let Some(v) = rev_var { head_binds.push((v.to_string(), rev_val.clone())); }
            for (slug, root) in self.resolve_scan_repos(&repo_val)? {
                let rev = self.resolve_rev(&root, &rev_val)?;
                out.push(ScanBinding { slug, root, rev, glob: glob.clone(),
                    head_binds: head_binds.clone() });
            }
        }
        Ok(out)
    }

    /// Persist the registered repo set into `_repo` (wipe + insert). `registered_at`
    /// stamps the flush; the daemon calls this when it loads or reloads config so
    /// a restart can diff the previously-registered repos against the new set.
    pub fn save_repos_meta(&self) -> Result<()> {
        let now = unix_secs();
        let rows: Vec<Vec<Value>> = self.repos.iter().map(|rc| vec![
            Value::Text(rc.slug.clone()),
            Value::Text(rc.root.to_string_lossy().into_owned()),
            Value::Text(rc.url.clone().unwrap_or_default()),
            Value::Int(now),
        ]).collect();
        self.db.exec("DELETE FROM _repo")?;
        self.db.insert_rows("_repo", &["slug", "root", "url", "registered_at"], &rows)?;
        Ok(())
    }

    /// Snapshot of the registered repo set (config + dynamically pulled). The
    /// daemon diffs this after a tick to add notify watches on newly-pulled
    /// roots, so edits in a dynamically-reached repo react.
    pub fn snapshot_repos(&self) -> Vec<crate::config::RepoConfig> {
        self.repos.clone()
    }
    /// Drain `repo`-sink rules: compile each sink's body as a SELECT (the gen
    /// lowering), collect `(slug, root, url)` rows, and for each row whose
    /// github org is in the `org` allowlist, clone (if missing) + register the
    /// repo into `self.repos`. Registered repos appear in the `repo` builtin and
    /// `scan("*")` on the NEXT tick; idempotent (a slug/root already registered
    /// is skipped, so re-asserting each tick is cheap).
    ///
    /// The `org` allowlist is a hard filter: a row pulls only when
    /// `parse_github_org(url)` is `Some(org)` AND `org(name)` contains it. Rows
    /// with no parseable github org, or whose org is not listed, are skipped
    /// with a stderr line. A missing/empty `org` relation pulls nothing.
    #[tracing::instrument(skip_all, fields(n_sinks = sinks.len()), level = "debug")]
    pub(crate) fn run_repo_pulls(&mut self, sinks: &[&Rule]) -> Result<()> {
        if sinks.is_empty() { return Ok(()); }
        let allowlist: HashSet<String> = if self.rels.contains_key("org") {
            self.db.conn()
                .prepare(&format!("SELECT DISTINCT \"name\" FROM {}", tbl("org")))?
                .query_map([], |r| r.get::<_, String>(0))?
                .filter_map(|x| x.ok()).collect()
        } else {
            HashSet::new()
        };
        let mut pulled = false;
        for rule in sinks {
            // A GROUND FACT — `repo("slug", "/root", "url").` — has an empty body
            // and an all-literal head. Take the literals directly; the SELECT-body
            // path (lower_gen) can't express a bodiless rule, which is why a bare
            // fact used to be rejected (the author had to route through an extra
            // rel). Otherwise the head must be all variables (slug, root, url),
            // selected in order: a literal head term over a body is a filter the
            // gen lowering can't express, so reject it loudly.
            // `explicit` = this row is an author-written ground fact (not derived
            // from a body over the org corpus). An explicit repo bypasses the
            // github-org allowlist (the allowlist gates DYNAMIC pulls, not a repo
            // the author named by hand) and registers a present root without a
            // clone.
            let rows: Vec<(String, String, String, bool)> = if rule.body.is_empty() {
                let lit = |t: &Term| match t {
                    Term::Str(s) => Some(s.clone()),
                    Term::Wild => Some(String::new()),
                    _ => None,
                };
                let vals: Option<Vec<String>> = rule.head.terms.iter().map(lit).collect();
                match vals.as_deref() {
                    Some([slug]) => vec![(slug.clone(), String::new(), String::new(), true)],
                    Some([slug, root]) => vec![(slug.clone(), root.clone(), String::new(), true)],
                    Some([slug, root, url]) => vec![(slug.clone(), root.clone(), url.clone(), true)],
                    _ => {
                        eprintln!("[repo-sink] ground-fact head must be literal (slug[, root[, url]]); skipping");
                        continue;
                    }
                }
            } else {
                let vars: Vec<String> = rule.head.terms.iter().filter_map(|t| match t {
                    Term::Var(v) => Some(v.clone()),
                    _ => None,
                }).collect();
                if vars.len() != rule.head.terms.len() {
                    eprintln!("[repo-sink] head must be all variables (slug, root, url) or an all-literal ground fact; skipping");
                    continue;
                }
                let sql = crate::lower::lower_gen(&vars, &rule.body, &self.rels)?;
                self.db.conn().prepare(&sql)?
                    .query_map([], |r| Ok((r.get::<_, String>(0)?,
                                           r.get::<_, String>(1)?,
                                           r.get::<_, String>(2)?, false)))?
                    .filter_map(|x| x.ok()).collect()
            };
            for (slug, root_str, url, explicit) in rows {
                if slug.is_empty() {
                    eprintln!("[repo-sink] skip row with empty slug");
                    continue;
                }
                if self.repos.iter().any(|r| r.slug == slug
                    || (!root_str.is_empty() && r.root == PathBuf::from(&root_str)))
                {
                    continue; // already registered
                }
                let org = Self::parse_github_org(&url);
                let allowed = explicit || org.as_ref().is_some_and(|o| allowlist.contains(o));
                if !allowed {
                    eprintln!("[repo-sink] skip {slug}: org {:?} not in allowlist ({} listed)",
                        org, allowlist.len());
                    continue;
                }
                let root = if root_str.is_empty() {
                    crate::daemon::daemon_home().join("repos").join(&slug)
                } else {
                    PathBuf::from(&root_str)
                };
                let rc = crate::config::RepoConfig {
                    slug: slug.clone(), root: root.clone(),
                    url: Some(url.clone()), allow_missing: false,
                };
                match Self::ensure_cloned(&rc) {
                    Ok(()) => {
                        eprintln!("[repo-sink] pulled {slug} -> {} (org {org:?})",
                            root.display());
                        self.repos.push(rc);
                        pulled = true;
                    }
                    Err(e) => eprintln!("[repo-sink] clone {slug} failed: {e}"),
                }
            }
        }
        if pulled { self.save_repos_meta()?; }
        Ok(())
    }

    /// Drain `checkout` demand-sink rows: keep each named repo's checkout current
    /// on its default branch NON-DESTRUCTIVELY. The sink never stashes, never
    /// `reset --hard`s, and never moves HEAD off its current branch. On the
    /// default branch it fast-forwards via `merge --ff-only` only when the
    /// working tree is clean (dirty or diverged → skip, surface why). On any
    /// other branch it moves only the ref pointer (`git branch -f`); the working
    /// tree is left exactly as it is.
    ///
    /// Min seconds between fetches of the SAME repo by the checkout sink
    /// (DL_CHECKOUT_MIN_SECS, default 300). 0 disables the gate. Stops a short
    /// clock from re-fetching every repo every tick.
    pub(crate) fn checkout_min_secs() -> u64 {
        std::env::var("DL_CHECKOUT_MIN_SECS").ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(300)
    }

    /// Per-repo last-fetch timestamps (in-process; the daemon is one warm
    /// engine for its lifetime). Used by the min-interval gate.
    pub(crate) fn checkout_last_fetch() -> &'static Mutex<HashMap<String, std::time::Instant>> {
        static LAST: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();
        LAST.get_or_init(|| Mutex::new(HashMap::new()))
    }

    /// A dedicated, narrow rayon pool for checkout sweeps (DL_CHECKOUT_WIDTH,
    /// default 2). `git fetch` is network+disk bound, so a 2-wide sweep is as
    /// fast as a full-core one and keeps the sink from eating the whole default
    /// pool (and every CPU core) when many repos are in view.
    pub(crate) fn checkout_pool() -> &'static rayon::ThreadPool {
        static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
        POOL.get_or_init(|| {
            let n = std::env::var("DL_CHECKOUT_WIDTH").ok()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|&n| n > 0)
                .unwrap_or(2);
            rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .thread_name(|i| format!("dl-checkout-{i}"))
                .build()
                .expect("checkout thread pool")
        })
    }

    /// Count rows in whichever checkout outcome rel currently exists (done or
    /// plan). Used by `drain_external_sinks` to surface "did this drain land
    /// new facts the next tick must derive from" without juggling schema.
    pub(crate) fn checkout_outcome_count(&self) -> Result<usize> {
        let conn = self.db.conn();
        for rel in ["checkout_done", "checkout_plan"] {
            if self.rels.contains_key(rel) {
                let n: i64 = conn.query_row(
                    &format!("SELECT COUNT(*) FROM {}", tbl(rel)), [], |r| r.get(0))?;
                return Ok(n as usize);
            }
        }
        Ok(0)
    }

    /// on disk (the ghcacher `checkout.rs` half). For every derived
    /// `checkout(repo, branch, pr_heads)` row we resolve the repo to a root
    /// (cloning a missing config repo via `resolve_repo`'s `ensure_cloned`), then
    /// sweep the roots in parallel — each an independent disk/network op. Runs
    /// AFTER the fixpoint + `run_repo_pulls` so a repo pulled this tick is
    /// sweepable, and so the rows reflect this tick's derivations.
    pub(crate) fn run_checkout_sweeps(&mut self) -> Result<()> {
        let Some(meta) = self.rels.get("checkout").cloned() else { return Ok(()); };
        if meta.cols.len() < 3 {
            bail!("checkout needs 3 columns (repo, branch, pr_heads); found {}", meta.cols.len());
        }
        let rows: Vec<(String, String, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT DISTINCT \"{}\",\"{}\",\"{}\" FROM {} ORDER BY 1,2",
                meta.col_name(0), meta.col_name(1), meta.col_name(2), tbl("checkout")))?;
            let rs = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                                    r.get::<_, String>(2)?)))?;
            rs.filter_map(|x| x.ok()).collect()
        };
        if rows.is_empty() { return Ok(()); }
        let offline = std::env::var_os("DL_NO_FETCH").is_some();
        // Resolve (and clone-if-missing) each repo up front — resolve_repo needs
        // &self, so keep it out of the parallel section.
        let mut jobs: Vec<(String, PathBuf, String, bool)> = Vec::new();
        for (repo, branch, pr) in rows {
            let pr_heads = matches!(pr.as_str(), "1" | "true" | "yes" | "pr");
            match self.resolve_repo(&repo) {
                Ok((slug, root)) => jobs.push((slug, root, branch, pr_heads)),
                Err(e) => eprintln!("[checkout] skip {repo}: {e}"),
            }
        }
        // Min-interval gate: a `checkout` rule driven by a short clock used to
        // `git fetch` every repo on every tick that crossed the clock boundary,
        // pinning all cores every 5 minutes. Now a repo can't be fetched more
        // often than DL_CHECKOUT_MIN_SECS (default 300s) regardless of the rule.
        // Throttled repos surface a `skip` checkout_done row so a program sees
        // they were intentionally held, not silently dropped.
        let min_secs = Self::checkout_min_secs();
        let now = std::time::Instant::now();
        let mut throttled: Vec<(String, String)> = Vec::new();
        let eligible: Vec<(String, PathBuf, String, bool)> = if min_secs == 0 {
            jobs
        } else {
            let last = Self::checkout_last_fetch();
            let mut guard = last.lock().unwrap_or_else(|p| p.into_inner());
            jobs.into_iter().filter(|(slug, _, branch, _)| {
                let allowed = guard.get(slug)
                    .map(|t| now.duration_since(*t).as_secs() >= min_secs)
                    .unwrap_or(true);
                if allowed {
                    guard.insert(slug.clone(), now);
                    true
                } else {
                    throttled.push((slug.clone(), branch.clone()));
                    false
                }
            }).collect()
        };
        // Run the sweep on a DEDICATED narrow pool (DL_CHECKOUT_WIDTH, default
        // 2). `git fetch` is network+disk bound, so a 2-wide sweep is as fast as
        // a full-core one and stops the checkout sink from consuming the whole
        // rayon pool (and every CPU core) when many repos are in view.
        // DL_CHECKOUT_DRY_RUN=1 previews the sweep: each outcome reports what
        // WOULD happen (ff/branch-f/skip-dirty/skip-diverged) without running
        // `merge --ff-only` or `git branch -f`, and the rows land in
        // `checkout_plan` instead of `checkout_done`.
        let dry_run = std::env::var_os("DL_CHECKOUT_DRY_RUN").is_some();
        let mut results: Vec<(String, String, CheckoutOutcome)> = if eligible.is_empty() {
            Vec::new()
        } else {
            Self::checkout_pool().install(|| {
                eligible.par_iter()
                    .map(|(slug, root, branch, pr)| {
                        let out = Self::checkout_one(root, branch, *pr, offline, dry_run);
                        (slug.clone(), branch.clone(), out)
                    }).collect()
            })
        };
        // Surface throttled repos so the program can tell fetch-skipped from
        // git-failed (ok=1, action="skip", detail names the gate).
        for (slug, branch) in throttled {
            results.push((slug, branch, CheckoutOutcome {
                action: "skip", ok: true,
                detail: format!("min-interval {min_secs}s (DL_CHECKOUT_MIN_SECS)"),
            }));
        }
        // Log AND surface an outcome rel: stderr goes to daemon.log under the
        // daemon (invisible to a query), so the rel is how a program / live
        // query confirms the sweep fired and reacts to failures. Dry-run emits
        // `checkout_plan` (what would happen); apply emits `checkout_done`.
        let sink_rel = if dry_run { "checkout_plan" } else { "checkout_done" };
        let mut done_rows: Vec<Vec<Value>> = Vec::with_capacity(results.len());
        for (slug, branch, out) in &results {
            eprintln!("[checkout{}] {slug}: {} {}", if dry_run { " (plan)" } else { "" }, out.action, out.detail);
            done_rows.push(vec![
                Value::Text(slug.clone()),
                Value::Text(branch.clone()),
                Value::Text(out.action.to_string()),
                Value::Int(if out.ok { 1 } else { 0 }),
                Value::Text(out.detail.clone()),
            ]);
        }
        self.refresh_rel(sink_rel,
            &["repo", "branch", "action", "ok", "detail"], &done_rows)?;
        Ok(())
    }

    /// One repo's keep-current sweep, NON-DESTRUCTIVE. Static (no `&self`) so the
    /// caller can run it under rayon. Fetches origin (unless `offline`), resolves
    /// the default branch (given, else `origin/HEAD`), then:
    ///   * on that branch + clean working tree → `merge --ff-only origin/<branch>`
    ///     (the only mutation; fails loud on divergence, leaving everything as-is);
    ///   * on that branch + dirty working tree → SKIP (the operator's work — we do
    ///     not stash, overwrite, or reset anything);
    ///   * on any other branch or detached → `git branch -f <branch> origin/<branch>`
    ///     (move ONLY the ref pointer; HEAD + working tree are untouched).
    /// When `dry_run` is true the mutations are skipped and the outcome carries
    /// what WOULD have happened (driven by `merge-base --is-ancestor`), so a
    /// program/CLI can preview the sweep without touching any checkout.
    pub(crate) fn checkout_one(root: &Path, branch: &str, pr_heads: bool, offline: bool, dry_run: bool) -> CheckoutOutcome {
        let skip = |detail: String| CheckoutOutcome { action: "skip", ok: false, detail };
        if !root.exists() { return skip(format!("root {} missing", root.display())); }
        let git = |args: &[&str]| Command::new("git").arg("-C").arg(root).args(args).output();
        if !offline {
            let _ = git(&["fetch", "--quiet", "origin"]);
            if pr_heads {
                let _ = git(&["fetch", "--prune", "--quiet", "origin",
                              "+refs/pull/*/head:refs/remotes/pr/*/head"]);
            }
        }
        let default = if !branch.is_empty() {
            branch.to_string()
        } else {
            match git(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]) {
                Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                    .trim().rsplit('/').next().unwrap_or("").to_string(),
                _ => String::new(),
            }
        };
        if default.is_empty() {
            return skip("no branch given and origin/HEAD unset (run `git remote set-head origin -a`)".into());
        }
        let remote_ref = format!("origin/{default}");
        let have_remote = git(&["rev-parse", "--verify", "--quiet", &remote_ref])
            .map(|o| o.status.success()).unwrap_or(false);
        if !have_remote {
            return skip(format!("no {remote_ref}{}",
                if offline { " (DL_NO_FETCH; never fetched)" } else { " (fetch failed / wrong branch)" }));
        }
        let cur = git(&["symbolic-ref", "--short", "HEAD"]).ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        if cur.as_deref() == Some(default.as_str()) {
            // On the default branch. NEVER stash, NEVER reset --hard. The only
            // mutation is a clean fast-forward; dirty or diverged checkouts are
            // the operator's work and are left untouched.
            let dirty = git(&["status", "--porcelain"]).map(|o| !o.stdout.is_empty()).unwrap_or(false);
            if dirty {
                return CheckoutOutcome { action: "skip", ok: true,
                    detail: format!("{default}: working tree dirty; left untouched") };
            }
            if dry_run {
                // merge-base --is-ancestor HEAD <remote>: exit 0 = HEAD is an
                // ancestor of remote (ff would succeed); non-zero = diverged.
                // No mutation: the rel name (checkout_plan) carries the preview.
                let ff_ok = git(&["merge-base", "--is-ancestor", "HEAD", &remote_ref])
                    .map(|o| o.status.success()).unwrap_or(false);
                return if ff_ok {
                    CheckoutOutcome { action: "ff", ok: true,
                        detail: format!("{default} -> {remote_ref}") }
                } else {
                    CheckoutOutcome { action: "skip", ok: false,
                        detail: format!("{default}: diverged from {remote_ref}; left untouched") }
                };
            }
            match git(&["merge", "--ff-only", &remote_ref]) {
                Ok(o) if o.status.success() => CheckoutOutcome { action: "ff", ok: true,
                    detail: format!("{default} -> {remote_ref}") },
                Ok(o) => CheckoutOutcome { action: "skip", ok: false,
                    detail: format!("{default}: --ff-only {remote_ref} failed (diverged?); left untouched: {}",
                        String::from_utf8_lossy(&o.stderr).trim()) },
                Err(e) => CheckoutOutcome { action: "skip", ok: false,
                    detail: format!("{default}: merge --ff-only {remote_ref} errored; left untouched: {e}") },
            }
        } else {
            // NOT on the default branch: move ONLY the ref pointer. HEAD + the
            // working tree stay exactly where they are.
            if dry_run {
                return CheckoutOutcome { action: "branch-f", ok: true, detail: format!(
                    "{default} -> {remote_ref} (checkout left on {})",
                    cur.as_deref().unwrap_or("detached HEAD")) };
            }
            match git(&["branch", "-f", &default, &remote_ref]) {
                Ok(o) if o.status.success() => CheckoutOutcome { action: "branch-f", ok: true, detail: format!(
                    "{default} -> {remote_ref} (checkout left on {})", cur.as_deref().unwrap_or("detached HEAD")) },
                Ok(o) => CheckoutOutcome { action: "branch-f", ok: false,
                    detail: format!("branch -f {default} failed: {}", String::from_utf8_lossy(&o.stderr).trim()) },
                Err(e) => CheckoutOutcome { action: "branch-f", ok: false,
                    detail: format!("branch -f {default} errored: {e}") },
            }
        }
    }

    /// Parse the org from a github URL: `https://github.com/<org>/<repo>(.git)?`
    /// (ssh `git@github.com:<org>/<repo>` accepted). `None` for non-github hosts
    /// or a path too short to carry an org. This is the allowlist key: a pulled
    /// repo's org must appear in the `org` relation.
    pub(crate) fn parse_github_org(url: &str) -> Option<String> {
        let u = url.trim();
        let path = if let Some(rest) = u.strip_prefix("https://github.com/")
            .or_else(|| u.strip_prefix("http://github.com/"))
            .or_else(|| u.strip_prefix("git@github.com:"))
        {
            rest
        } else {
            return None;
        };
        let mut parts = path.split('/');
        let org = parts.next()?;
        if org.is_empty() { return None; }
        let repo = parts.next()?;
        if repo.is_empty() { return None; }
        Some(org.to_string())
    }

    /// Resolve `name` (HEAD or a ref) to an oid in `repo_root`, compare to the
    /// last-seen oid stored in `_ref`, and persist the new value. Returns
    /// `Some((old, new))` when the oid changed (old is `None` on first sight),
    /// appending one row to `_rev_log`; `None` when unchanged. The single-event
    /// write path: one ref observed per call, not an N+1 batch.
    pub fn observe_ref(&self, repo: &str, repo_root: &Path, name: &str)
        -> Result<Option<(Option<String>, String)>>
    {
        let out = Command::new("git").arg("-C").arg(repo_root)
            .args(["rev-parse", name]).output()?;
        if !out.status.success() { return Ok(None); }
        let new = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if new.is_empty() { return Ok(None); }
        let old: Option<String> = self.db.conn().query_row(
            "SELECT oid FROM _ref WHERE repo = ?1 AND name = ?2",
            rusqlite::params![repo, name], |r| r.get(0)).ok();
        if old.as_deref() == Some(new.as_str()) { return Ok(None); }
        let now = unix_secs();
        self.db.conn().execute(
            "INSERT INTO _ref(repo, name, oid, observed_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(repo, name) DO UPDATE SET oid=excluded.oid, observed_at=excluded.observed_at",
            rusqlite::params![repo, name, new, now])?;
        self.db.conn().execute(
            "INSERT INTO _rev_log(repo, name, old, new, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![repo, name, old.clone().unwrap_or_default(), new, now])?;
        Ok(Some((old, new)))
    }

    /// Paths that differ between two revs in `repo_root`, intersected with the
    /// `_file` path index for `repo` (so a rev advance only reports files the
    /// engine actually tracks). `old` empty = first sight, every tracked path
    /// for that repo is considered changed.
    pub fn files_changed_between(&self, repo: &str, repo_root: &Path, old: &str, new: &str)
        -> Result<Vec<String>>
    {
        let mut tracked = self.db.conn().prepare(
            "SELECT DISTINCT path FROM _file WHERE repo = ?1")?;
        let tracked: HashSet<String> = tracked
            .query_map(rusqlite::params![repo], |r| r.get::<_, String>(0))?
            .filter_map(|x| x.ok()).collect();
        if old.is_empty() {
            let mut v: Vec<String> = tracked.into_iter().collect();
            v.sort();
            return Ok(v);
        }
        let out = Command::new("git").arg("-C").arg(repo_root)
            .args(["diff", "--name-only", old, new]).output()?;
        if !out.status.success() { return Ok(Vec::new()); }
        let mut changed: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|p| !p.is_empty() && tracked.contains(p))
            .collect();
        changed.sort();
        changed.dedup();
        Ok(changed)
    }




















    // LANG-JUNCTION(manifest-probe): the manifest filename list probed above the scanned file set; a language whose module resolver reads a manifest (Cargo.toml, package.json, go.mod) must add its name here or the resolver gets no manifest content
    /// Read the Cargo.toml / package.json / go.mod manifests above the file set,
    /// at this rev, into a map (manifest path -> contents) for the resolver's
    /// crate / package / module registries. Probes the distinct ancestor
    /// directories of the files; `read_content` errors (no such manifest) are
    /// skipped. Rev-correct (git show for a git rev, disk for WORK).
    pub(crate) fn collect_manifests(&self, rev: &str, files: &HashSet<String>) -> HashMap<String, String> {
        let mut dirs: HashSet<String> = HashSet::new();
        for f in files {
            let mut d = Path::new(f);
            while let Some(p) = d.parent() {
                dirs.insert(p.to_string_lossy().replace('\\', "/"));
                d = p;
            }
        }
        let mut out = HashMap::new();
        for dir in dirs {
            for name in ["Cargo.toml", "package.json", "go.mod"] {
                let rel = if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") };
                if let Ok(content) = read_content(&self.root, rev, &rel) {
                    out.insert(rel, content);
                }
            }
        }
        out
    }

}

struct GitBatch {
    // Held only so the process handle isn't dropped while the pipes live.
    _child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

impl GitBatch {
    pub(crate) fn open(root: &Path) -> Result<GitBatch> {
        let mut child = Command::new("git")
            .arg("-C").arg(root)
            .args(["cat-file", "--batch"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = std::io::BufReader::new(child.stdout.take().expect("piped stdout"));
        Ok(GitBatch { _child: child, stdin, stdout })
    }

    pub(crate) fn read(&mut self, rev: &str, path: &str) -> Result<String> {
        use std::io::{BufRead, Read, Write};
        writeln!(self.stdin, "{rev}:{path}")?;
        self.stdin.flush()?;
        // header: `<oid> <type> <size>` or `<object> missing` / `... ambiguous`
        let mut header = String::new();
        self.stdout.read_line(&mut header)?;
        let header = header.trim_end();
        let size: usize = match header.rsplit(' ').next().and_then(|s| s.parse().ok()) {
            Some(n) => n,
            None => bail!("git cat-file failed for {rev}:{path} ({header})"),
        };
        let mut buf = vec![0u8; size + 1]; // content + trailing LF
        self.stdout.read_exact(&mut buf)?;
        buf.pop();
        Ok(String::from_utf8_lossy(&buf).to_string())
    }
}

pub(crate) fn git_batch_read(root: &Path, rev: &str, path: &str) -> Result<String> {
    static BATCHES: OnceLock<std::sync::Mutex<HashMap<PathBuf, std::sync::Arc<std::sync::Mutex<GitBatch>>>>> = OnceLock::new();
    let map = BATCHES.get_or_init(Default::default);
    let batch = {
        let mut m = map.lock().unwrap();
        match m.entry(root.to_path_buf()) {
            std::collections::hash_map::Entry::Occupied(e) => e.get().clone(),
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(std::sync::Arc::new(std::sync::Mutex::new(GitBatch::open(root)?))).clone()
            }
        }
    };
    let mut b = batch.lock().unwrap();
    b.read(rev, path)
}
