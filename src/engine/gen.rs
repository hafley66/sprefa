use super::*;

impl Engine {
    /// already match, so a converged tick leaves the tree untouched.
    // ARCH {"url":"engine/60-gen","role":"output"}
    #[tracing::instrument(skip_all, level = "debug")]
    pub(crate) fn run_gens(&mut self, prog: &Program, quiet: bool) -> Result<()> {
        let mut written: Vec<String> = Vec::new();
        // Candidate write roots for gen targets: self.root plus the `.git`
        // ancestor of every rule's origin — but ONLY when root_implicit (rootless
        // daemon), since that's the one mode where self.root is a placeholder and
        // a loaded script must write back to the repo it scans. Foreground/LSP
        // honor the explicit self.root.
        let mut write_roots: Vec<PathBuf> = vec![self.root.clone()];
        if self.root_implicit {
            for item in &prog.items {
                if let Item::Rule(r) = item {
                    if let Some(o) = &r.origin {
                        if let Some(a) = crate::repo::nearest_git(o) {
                            if !write_roots.contains(&a) {
                                write_roots.push(a);
                            }
                        }
                    }
                }
            }
        }
        // Splice regions accumulate across ALL gen rules and apply per file in
        // one bottom-up write: the line coordinates come from the pre-tick file
        // content, so a second rule splicing the same file must not re-read a
        // file an earlier rule already grew.
        let mut splices = Splices::default();
        // Byte cursors (gen(:mode, ...)) accumulate the same way, separate from
        // line splices because their coordinates and apply semantics differ.
        let mut cursors = Cursors::default();
        // Named-marker zones (gen(:zone, ...)) — `BEGIN:/END:` by name.
        let mut zones = Zones::default();
        // File targets claimed by a gen rule this tick. A second gen rule
        // rendering the SAME path would silently overwrite the first (the v0
        // last-wins trap: file emits don't concatenate across rules), so the
        // File arm bails loudly when a path is already claimed by another rule.
        // Within one rule, rows to one path concat via `groups`; the claim is
        // per rule, so that path stays legal.
        let mut claimed: HashMap<String, ()> = HashMap::new();
        // Whole-file :append buffers, flushed after every gen rule ran so rows
        // from all append rules to one path concatenate in program order.
        let mut appends = Appends::default();
        for item in &prog.items {
            if let Item::Gen(g) = item {
                self.run_gen(
                    g,
                    &mut written,
                    &mut splices,
                    &mut cursors,
                    &mut zones,
                    &mut claimed,
                    &mut appends,
                    &write_roots,
                )?;
            }
        }
        self.apply_splices(&splices, &mut written, &write_roots)?;
        self.apply_cursors(&cursors, &mut written, &write_roots)?;
        self.apply_zones(&zones, &mut written, &write_roots)?;
        self.apply_appends(&appends, &claimed, &mut written, &write_roots)?;
        if !written.is_empty() && !quiet {
            let files = written.join(", ");
            // @eprintln-ok: gen sink file-write summary is the command's stderr output contract
            eprintln!("[gen] wrote {files}");
        }
        Ok(())
    }

    fn run_gen(
        &mut self,
        g: &GenRule,
        written: &mut Vec<String>,
        splices: &mut Splices,
        cursors: &mut Cursors,
        zones: &mut Zones,
        claimed: &mut HashMap<String, ()>,
        appends: &mut Appends,
        write_roots: &[PathBuf],
    ) -> Result<()> {
        // Target vars lead the SELECT so grouping reads prefix columns; the
        // ORDER BY over all vars makes render order deterministic.
        let target_vars: Vec<String> = match &g.target {
            GenTarget::File { path_tmpl, .. } => tmpl_holes(path_tmpl),
            GenTarget::Splice { path, l0, l1 } => vec![var_of(path)?, var_of(l0)?, var_of(l1)?],
            GenTarget::Cursor { path, lo, hi, .. } => vec![var_of(path)?, var_of(lo)?, var_of(hi)?],
            GenTarget::Zone {
                path_tmpl,
                name_tmpl,
            } => {
                // Templates carry their own {var} holes; pull them in
                // first-appearance order (tmpl_holes dedups), ahead of the row
                // template's holes (the run_gen loop already merges target vars
                // first, then row vars, so this is consistent).
                let mut v: Vec<String> = Vec::new();
                for h in tmpl_holes(path_tmpl)
                    .into_iter()
                    .chain(tmpl_holes(name_tmpl))
                {
                    if !v.contains(&h) {
                        v.push(h);
                    }
                }
                v
            }
        };
        let mut vars = target_vars.clone();
        for v in tmpl_holes(&g.row_tmpl) {
            if !vars.contains(&v) {
                vars.push(v);
            }
        }
        let mut body = g.body.clone();
        crate::lower::resolve_work_alias_body(&mut body, &self.rels, &self.self_rev_text());
        let sql = crate::lower::lower_gen(&vars, &body, &self.rels)?;
        let rows: Vec<HashMap<String, String>> = {
            let values = self.db.query_values("gen", &sql, &[])?;
            values
                .into_iter()
                .map(|row| {
                    let mut m = HashMap::new();
                    for (i, v) in vars.iter().enumerate() {
                        let cell = match row.get(i) {
                            Some(crate::db::SqlVal::Text(s)) => s.clone(),
                            Some(crate::db::SqlVal::Int(n)) => n.to_string(),
                            Some(crate::db::SqlVal::Real(f)) => f.to_string(),
                            _ => String::new(),
                        };
                        m.insert(v.clone(), cell);
                    }
                    m
                })
                .collect()
        };

        match &g.target {
            GenTarget::File { path_tmpl, append } => {
                let mut order: Vec<String> = Vec::new();
                let mut groups: HashMap<String, Vec<String>> = HashMap::new();
                for m in &rows {
                    let path = render_tmpl(path_tmpl, m);
                    if path.starts_with('/') || path.split('/').any(|s| s == "..") {
                        bail!("gen path `{path}` escapes the root");
                    }
                    if !groups.contains_key(&path) {
                        order.push(path.clone());
                    }
                    groups
                        .entry(path)
                        .or_default()
                        .push(render_tmpl(&g.row_tmpl, m));
                }
                if *append {
                    // File-append: accumulate this rule's rows (already ORDER
                    // BY'd within the rule) onto the per-file buffer. Across
                    // rules they concatenate in program order — the for loop in
                    // run_all_gens visits gen items in source order — so a header
                    // rule, a separator rule, and a rows rule assemble one page.
                    // Flushed once in apply_appends after every gen rule ran.
                    for p in order {
                        let rows = groups.remove(&p).unwrap();
                        if !appends.groups.contains_key(&p) {
                            appends.keys.push(p.clone());
                        }
                        appends.groups.entry(p).or_default().extend(rows);
                    }
                    return Ok(());
                }
                // A path another gen rule already wrote this tick would be
                // clobbered, not concatenated. Bail loudly (mirrors the
                // mixed-source/derived bail): union the rows into one relation
                // and emit them from a single gen rule (or use :append).
                for p in &order {
                    if claimed.insert(p.clone(), ()).is_some() {
                        bail!(
                            "two gen rules write the same file `{p}`; file emits don't \
                               concatenate across rules (last would win). Union the rows into \
                               one relation and emit from a single gen rule, or use \
                               gen(:append, \"{p}\", ...) to concatenate in program order."
                        );
                    }
                }
                for p in order {
                    let content = format!("{}\n", groups[&p].join("\n"));
                    let full = resolve_write_full(write_roots, &p);
                    let bytes = content.len();
                    if std::fs::read(&full).ok().as_deref() == Some(content.as_bytes()) {
                        // Same bytes as last tick: no write happened. This is the
                        // branch the "15 changed paths, ten ticks, no mtime
                        // movement" hypothesis needs to be decidable from the
                        // trail — `wrote: false` here is the whole answer.
                        crate::eventlog::emit("gen_write", Some(&self.root), serde_json::json!({
                            "path": p, "target": "file", "bytes": bytes, "wrote": false,
                        }));
                        continue;
                    }
                    if let Some(dir) = full.parent() {
                        std::fs::create_dir_all(dir)?;
                    }
                    self.journaled_write(&full, &p, content.as_bytes())?;
                    crate::eventlog::emit("gen_write", Some(&self.root), serde_json::json!({
                        "path": p, "target": "file", "bytes": bytes, "wrote": true,
                    }));
                    written.push(p);
                }
            }
            GenTarget::Splice { .. } => {
                // Rows group by (path, l0, l1) — the `comment` op's paired
                // coordinates over WORK. Accumulate only; the file writes
                // happen once, in apply_splices, after every gen rule ran.
                for m in &rows {
                    let path = m[&vars[0]].clone();
                    let l0: i64 = m[&vars[1]].parse().map_err(|_| {
                        anyhow::anyhow!("gen splice l0 must be an int, got `{}`", m[&vars[1]])
                    })?;
                    let l1: i64 = m[&vars[2]].parse().map_err(|_| {
                        anyhow::anyhow!("gen splice l1 must be an int, got `{}`", m[&vars[2]])
                    })?;
                    if l1 <= l0 {
                        bail!(
                            "gen splice needs l1 > l0 (paired marker LINES with the replaced \
                             region between them), got {l0}..{l1} in {path} — one line cannot be \
                             both markers. Either give the region its own lines (marker / content \
                             / marker), or switch to the named-marker form `gen(:zone, path, ...)` \
                             (BEGIN:name/END:name pair, survives surrounding edits, no line \
                             numbers in the rule)"
                        );
                    }
                    let key = (path, l0, l1);
                    if !splices.groups.contains_key(&key) {
                        splices.keys.push(key.clone());
                    }
                    splices
                        .groups
                        .entry(key)
                        .or_default()
                        .push(render_tmpl(&g.row_tmpl, m));
                }
            }
            GenTarget::Cursor { mode, .. } => {
                // Byte-accurate splice into [lo, hi); rows group by (path, lo,
                // hi, mode) but apply per-file across the union of regions. lo
                // and hi come from the ref spine, so a typical rule joins match
                // -> ref(id, _, path, lo, hi) -> gen(:mode, path, lo, hi, ...).
                for m in &rows {
                    let path = m[&vars[0]].clone();
                    let lo: i64 = m[&vars[1]].parse().map_err(|_| {
                        anyhow::anyhow!(
                            "gen :{} lo must be an int, got `{}`",
                            mode.tag(),
                            m[&vars[1]]
                        )
                    })?;
                    let hi: i64 = m[&vars[2]].parse().map_err(|_| {
                        anyhow::anyhow!(
                            "gen :{} hi must be an int, got `{}`",
                            mode.tag(),
                            m[&vars[2]]
                        )
                    })?;
                    if hi < lo {
                        bail!(
                            "gen :{} needs hi >= lo, got {lo}..{hi} in {path}",
                            mode.tag()
                        );
                    }
                    let key = (path.clone(), lo, hi, *mode);
                    if !cursors.groups.contains_key(&key) {
                        cursors.keys.push(key.clone());
                    }
                    cursors
                        .groups
                        .entry(key)
                        .or_default()
                        .push(render_tmpl(&g.row_tmpl, m));
                }
            }
            GenTarget::Zone {
                path_tmpl,
                name_tmpl,
            } => {
                // Named-marker zone: path + name are {var}-hole templates,
                // rendered per row. At apply time the engine resolves each
                // rendered name to a `BEGIN: <name>`/`END:` line pair and
                // rewrites the content between them, so the rule is immune to
                // surrounding-text edits without re-anchoring line numbers.
                for m in &rows {
                    let path = render_tmpl(path_tmpl, m);
                    let name = render_tmpl(name_tmpl, m);
                    if name.is_empty() {
                        bail!("gen :zone name rendered empty in {path}");
                    }
                    let key = (path, name);
                    if !zones.groups.contains_key(&key) {
                        zones.keys.push(key.clone());
                    }
                    zones
                        .groups
                        .entry(key)
                        .or_default()
                        .push(render_tmpl(&g.row_tmpl, m));
                }
            }
        }
        Ok(())
    }

    /// Replace the lines strictly between each region's marker lines; the
    /// markers stay. One read-modify-write per file, regions bottom-up so
    /// earlier line numbers stay valid across the whole batch.
    ///
    /// Overlap gate (same stance as `apply_cursors`): the bottom-up apply
    /// assumes every region's line window refers to the ORIGINAL line list.
    /// Two regions whose [l0, l1) windows intersect would corrupt, so bail
    /// loudly.
    fn apply_splices(
        &self,
        splices: &Splices,
        written: &mut Vec<String>,
        write_roots: &[PathBuf],
    ) -> Result<()> {
        let mut files: Vec<&str> = Vec::new();
        for (p, _, _) in &splices.keys {
            if !files.contains(&p.as_str()) {
                files.push(p);
            }
        }
        for p in files {
            let full = resolve_write_full(write_roots, p);
            let old = std::fs::read_to_string(&full)
                .map_err(|e| anyhow::anyhow!("gen splice target {p}: {e}"))?;
            let mut lines: Vec<String> = old.lines().map(|s| s.to_string()).collect();
            let mut regions: Vec<&(String, i64, i64)> =
                splices.keys.iter().filter(|k| k.0 == p).collect();
            // Disjointness gate: ascending by l0, require each region's l1 <=
            // the next region's l0.
            regions.sort_by_key(|k| k.1);
            for w in regions.windows(2) {
                if w[0].2 > w[1].1 {
                    bail!(
                        "gen splice regions overlap in {p}: lines [{},{}) and [{},{}) \
                          collide; merge into one rule or disjoint the windows",
                        w[0].1,
                        w[0].2,
                        w[1].1,
                        w[1].2
                    );
                }
            }
            regions.sort_by_key(|k| std::cmp::Reverse(k.1));
            for k in regions {
                let (l0, l1) = (k.1 as usize, k.2 as usize);
                if l1 > lines.len() {
                    bail!(
                        "gen splice region {l0}..{l1} out of range in {p} ({} lines)",
                        lines.len()
                    );
                }
                lines.splice(l0..l1 - 1, splices.groups[k].iter().cloned());
            }
            let content = format!("{}\n", lines.join("\n"));
            let bytes = content.len();
            if content != old {
                self.journaled_write(&full, p, content.as_bytes())?;
                crate::eventlog::emit("gen_write", Some(&self.root), serde_json::json!({
                    "path": p, "target": "splice", "bytes": bytes, "wrote": true,
                }));
                written.push(p.to_string());
            } else {
                crate::eventlog::emit("gen_write", Some(&self.root), serde_json::json!({
                    "path": p, "target": "splice", "bytes": bytes, "wrote": false,
                }));
            }
        }
        Ok(())
    }

    /// Flush the whole-file `gen(:append, ...)` buffers: one write per path,
    /// rows joined in the program order they accumulated. Same byte-converge +
    /// dir-create as the non-append File arm. A path also written by a plain
    /// File rule (in `claimed`) is a clobber — bail, mirroring the claimed-path
    /// stance (mixing the two forms on one file would race last-wins).
    fn apply_appends(
        &self,
        appends: &Appends,
        claimed: &HashMap<String, ()>,
        written: &mut Vec<String>,
        write_roots: &[PathBuf],
    ) -> Result<()> {
        for p in &appends.keys {
            if claimed.contains_key(p) {
                bail!(
                    "file `{p}` is written by both a plain gen rule and gen(:append, ...); \
                       pick one form for a given file"
                );
            }
            let content = format!("{}\n", appends.groups[p].join("\n"));
            let full = resolve_write_full(write_roots, p);
            let bytes = content.len();
            if std::fs::read(&full).ok().as_deref() == Some(content.as_bytes()) {
                crate::eventlog::emit("gen_write", Some(&self.root), serde_json::json!({
                    "path": p, "target": "append", "bytes": bytes, "wrote": false,
                }));
                continue;
            }
            if let Some(dir) = full.parent() {
                std::fs::create_dir_all(dir)?;
            }
            self.journaled_write(&full, p, content.as_bytes())?;
            crate::eventlog::emit("gen_write", Some(&self.root), serde_json::json!({
                "path": p, "target": "append", "bytes": bytes, "wrote": true,
            }));
            written.push(p.clone());
        }
        Ok(())
    }

    /// Apply byte-accurate gen(:mode, ...) cursors per file. Port of v4's
    /// `WriteCursorComponent::render_batch`: group by path, sort regions
    /// right-to-left by lo so earlier offsets stay valid as the buffer grows,
    /// then dispatch on mode. :wrap inserts at both endpoints; the other three
    /// modes splice into the [lo, hi) window, at lo, at hi, or replace it
    /// outright. One read-modify-write per file. Skipped silently if the
    /// resulting bytes match the original. `write_roots` resolves the target
    /// per file (parity with `apply_splices`): under `root_implicit` a cursor
    /// rule from a loaded script writes back to that script's repo, not the
    /// placeholder root.
    ///
    /// Overlap gate: the right-to-left apply assumes every region's window
    /// refers to the ORIGINAL buffer. Two regions whose [lo, hi) windows
    /// intersect would see the second operate on already-mutated bytes (silent
    /// corruption), so bail loudly — same stance as the file-emit claimed-path
    /// bail. Regions keyed identically (same path/lo/hi/mode) already concat
    /// their rows into one payload, so this only fires across DISTINCT windows.
    fn apply_cursors(
        &self,
        cursors: &Cursors,
        written: &mut Vec<String>,
        write_roots: &[PathBuf],
    ) -> Result<()> {
        let mut files: Vec<&str> = Vec::new();
        for (p, _, _, _) in &cursors.keys {
            if !files.contains(&p.as_str()) {
                files.push(p);
            }
        }
        for p in files {
            let full = resolve_write_full(write_roots, p);
            let original =
                std::fs::read(&full).map_err(|e| anyhow::anyhow!("gen :mode target {p}: {e}"))?;
            let mut regions: Vec<&(String, i64, i64, SpliceMode)> =
                cursors.keys.iter().filter(|k| k.0 == p).collect();
            // Disjointness gate: ascending by lo, require each region's hi <=
            // the next region's lo. Adjacent-equal (hi == next lo) is allowed
            // (windows touch but don't overlap).
            regions.sort_by_key(|k| k.1);
            for w in regions.windows(2) {
                if w[0].2 > w[1].1 {
                    bail!(
                        "gen :{} and :{} regions overlap in {p}: [{},{}) and [{},{}) \
                          collide; merge into one rule or disjoint the windows",
                        w[0].3.tag(),
                        w[1].3.tag(),
                        w[0].1,
                        w[0].2,
                        w[1].1,
                        w[1].2
                    );
                }
            }
            // Right-to-left by lo so prior inserts don't shift later offsets.
            regions.sort_by_key(|k| std::cmp::Reverse(k.1));
            let mut buf: Vec<u8> = original.clone();
            for k in regions {
                let (lo, hi, mode) = (k.1 as usize, k.2 as usize, k.3);
                let lo = lo.min(buf.len());
                let hi = hi.min(buf.len()).max(lo);
                // Each row in the group becomes one insertion/splice. Rows
                // already share (path, lo, hi, mode); render their templates in
                // order and concatenate so multi-row groups behave like
                // line-splice (rows join into one payload).
                let payload: Vec<u8> = cursors.groups[k]
                    .iter()
                    .flat_map(|s| s.as_bytes().iter().copied())
                    .collect();
                match mode {
                    SpliceMode::Replace => {
                        buf.splice(lo..hi, payload);
                    }
                    SpliceMode::Append => {
                        buf.splice(hi..hi, payload);
                    }
                    SpliceMode::Prepend => {
                        buf.splice(lo..lo, payload);
                    }
                    SpliceMode::Wrap => {
                        // Insert at hi first so the lo offset stays valid, then lo.
                        buf.splice(hi..hi, payload.clone());
                        buf.splice(lo..lo, payload);
                    }
                }
            }
            let bytes = buf.len();
            if buf != original {
                self.journaled_write(&full, p, &buf)?;
                crate::eventlog::emit("gen_write", Some(&self.root), serde_json::json!({
                    "path": p, "target": "cursor", "bytes": bytes, "wrote": true,
                }));
                written.push(p.to_string());
            } else {
                crate::eventlog::emit("gen_write", Some(&self.root), serde_json::json!({
                    "path": p, "target": "cursor", "bytes": bytes, "wrote": false,
                }));
            }
        }
        Ok(())
    }

    /// Flush the named-marker `gen(:zone, ...)` regions: per file, locate each
    /// zone by name (a `BEGIN: <name>` line through the next `END:` line,
    /// tolerant of comment prefixes), then replace the lines STRICTLY between
    /// them. The markers themselves stay. One read-modify-write per file;
    /// multiple zones in one file apply bottom-up so earlier line numbers stay
    /// valid. An unknown name bails loudly (a typo should never silently drop
    /// the rewrite). Indentation: each emitted row inherits the BEGIN marker's
    /// leading whitespace when present, so a zone inside an indented block
    /// stays indented without per-row whitespace in the template.
    fn apply_zones(
        &self,
        zones: &Zones,
        written: &mut Vec<String>,
        write_roots: &[PathBuf],
    ) -> Result<()> {
        if zones.keys.is_empty() {
            return Ok(());
        }
        // Group zone keys by file (one read per file).
        let mut files: Vec<&str> = Vec::new();
        for (p, _) in &zones.keys {
            if !files.contains(&p.as_str()) {
                files.push(p);
            }
        }
        for p in files {
            let full = resolve_write_full(write_roots, p);
            let old = std::fs::read_to_string(&full)
                .map_err(|e| anyhow::anyhow!("gen :zone target {p}: {e}"))?;
            let mut lines: Vec<String> = old.lines().map(|s| s.to_string()).collect();
            // Zones of this file, bottom-up by BEGIN line so earlier zones' line
            // numbers stay valid as later ones splice (mirrors apply_splices).
            let mut found: Vec<(usize, usize, &str, &Vec<String>)> = Vec::new();
            for (path, name) in zones.keys.iter().filter(|(pp, _)| pp == p) {
                let payload = &zones.groups[&(path.clone(), name.clone())];
                let (begin_line, end_line) = find_zone(&lines, name).ok_or_else(|| {
                    anyhow::anyhow!(
                        "gen :zone `{name}` not found in {p}; expected a `BEGIN: {name}` line \
                         followed by an `END:` line (any comment prefix works: // # /* ; <!--)"
                    )
                })?;
                found.push((begin_line, end_line, name.as_str(), payload));
            }
            found.sort_by_key(|(b, _, _, _)| std::cmp::Reverse(*b));
            for (begin, end, name, payload) in found {
                // Indentation inherited from the BEGIN marker line, applied to
                // every payload row that does not already carry it (avoid
                // double-indenting a template that already includes leading
                // whitespace).
                let indent: String = lines[begin]
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                let rendered: Vec<String> = payload
                    .iter()
                    .map(|row| {
                        if !indent.is_empty() && !row.starts_with(&indent) {
                            format!("{indent}{row}")
                        } else {
                            row.clone()
                        }
                    })
                    .collect();
                // Replace lines STRICTLY between begin and end (exclusive both):
                // indices [begin+1, end). The markers stay.
                lines.splice((begin + 1)..end, rendered);
                let _ = name; // surfaced in the error above; nothing else to do
            }
            let content = format!("{}\n", lines.join("\n"));
            let bytes = content.len();
            if content != old {
                self.journaled_write(&full, p, content.as_bytes())?;
                crate::eventlog::emit("gen_write", Some(&self.root), serde_json::json!({
                    "path": p, "target": "zone", "bytes": bytes, "wrote": true,
                }));
                written.push(p.to_string());
            } else {
                crate::eventlog::emit("gen_write", Some(&self.root), serde_json::json!({
                    "path": p, "target": "zone", "bytes": bytes, "wrote": false,
                }));
            }
        }
        Ok(())
    }
}
/// Locate a NAMED zone in a file's line list. Returns `(begin_idx, end_idx)`
/// 0-based LINE INDICES where `lines[begin_idx]` carries `BEGIN: <name>` and
/// `lines[end_idx]` carries the matching `END:`. The caller splices the
/// strictly-inside range `[begin_idx+1, end_idx)`. Comment-prefix-tolerant:
/// `// BEGIN: name`, `# BEGIN: name`, `/* BEGIN: name */`, `; BEGIN: name`,
/// `<!-- BEGIN: name -->`, or a bare `BEGIN: name` all match. The first END
/// after the BEGIN closes the zone (END carries no name). `None` if no pair.

fn find_zone(lines: &[String], name: &str) -> Option<(usize, usize)> {
    let mut begin: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        if begin.is_none() {
            if zone_marker_name(line, "BEGIN").as_deref() == Some(name) {
                begin = Some(i);
            }
        } else if zone_marker_name(line, "END").is_some() {
            return Some((begin.unwrap(), i));
        }
    }
    None
}

/// Pull `<name>` out of a `BEGIN: <name>` / `END: <name>` marker line, stripped
/// of any comment prefix (`//`, `#`, `/*`, `*`, `;`, `<!--`, `-->`, `*/`),
/// leading/trailing whitespace, and a trailing comment closer. `None` when the
/// line is not a marker of the requested kind or has no name on `END:` form
/// (END: always matches regardless of name — name is informational there).
fn zone_marker_name(line: &str, kind: &str) -> Option<String> {
    let trimmed = line.trim();
    // Strip a leading comment prefix if present (any of the common forms).
    let body = trimmed
        .trim_start_matches("//")
        .trim_start_matches('#')
        .trim_start_matches("/*")
        .trim_start_matches('*')
        .trim_start_matches(';')
        .trim_start_matches("<!--")
        .trim();
    // Strip a trailing comment closer.
    let body = body.trim_end_matches("-->").trim_end_matches("*/").trim();
    let keyword = format!("{kind}:");
    let after = body.strip_prefix(&keyword)?;
    let name = after.trim();
    Some(name.to_string())
}

/// Splice regions collected across every gen rule in a tick: key order is
/// first appearance, rows per key append in rule order.
#[derive(Default)]
struct Splices {
    keys: Vec<(String, i64, i64)>,
    groups: HashMap<(String, i64, i64), Vec<String>>,
}

/// Byte-cursor regions collected across every gen(:mode, ...) rule in a tick.
/// The key is (path, lo, hi, mode); rows per key join into one payload at
/// apply time. Separate from `Splices` because the coordinates and apply
/// semantics differ (byte offsets vs line numbers, four modes vs implicit
/// replace). Ported from v4's `WriteCursorComponent` accumulator.
#[derive(Default)]
struct Cursors {
    keys: Vec<(String, i64, i64, SpliceMode)>,
    groups: HashMap<(String, i64, i64, SpliceMode), Vec<String>>,
}

/// Whole-file `gen(:append, "path", ...)` buffers collected across every append
/// rule in a tick. The key is the rendered path; rows per path concatenate in
/// program order (the gen-item visit order in run_all_gens), so a header rule +
/// a rows rule build one ordered page. Flushed once in `apply_appends` after
/// every gen rule ran, mirroring `Splices`.
#[derive(Default)]
struct Appends {
    keys: Vec<String>,
    groups: HashMap<String, Vec<String>>,
}

/// Named-marker zones collected across every `gen(:zone, p, name, ...)` rule in
/// a tick. The key is (path, name); rows per zone join into one payload at
/// apply time. Flushed once in `apply_zones` after every gen rule ran, mirroring
/// `Splices`/`Appends`. The engine resolves `name` to a `BEGIN: name`/`END:`
/// line pair in the file at apply time, so the rule stays valid across
/// surrounding-text edits without re-anchoring line numbers.
#[derive(Default)]
struct Zones {
    keys: Vec<(String, String)>,
    groups: HashMap<(String, String), Vec<String>>,
}

/// `{var}` holes in a gen template, first-appearance order, deduped. Honors
/// mustache-style escapes: `{{` and `}}` are literal braces, NOT a hole open/
/// close, so `{{name}}` in a template emits the literal text `{name}` (lets a
/// template target language with its own `{x}` syntax — JS template literals,
/// JSON, CSS classes, Go templates — survive into the output verbatim).
fn tmpl_holes(t: &str) -> Vec<String> {
    let b = t.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            // `{{` → literal `{`, skip past both so the inner text is not parsed
            // as a hole name.
            if b.get(i + 1) == Some(&b'{') {
                i += 2;
                continue;
            }
            // `{name}` → hole ONLY if every byte from i+1 to the closing `}` is
            // a plain identifier char. Bail on the first non-identifier byte
            // (treat the `{` as literal); otherwise we'd greedy-scan past an
            // inner `{{x}}` mustache run and mis-parse it as a hole.
            let mut j = i + 1;
            while j < b.len() && b[j] != b'}' && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            if j < b.len() && b[j] == b'}' && j > i + 1 {
                let name = &t[i + 1..j];
                if !out.iter().any(|v| v == name) {
                    out.push(name.to_string());
                }
                i = j + 1;
                continue;
            }
            // Not a hole (no closing `}` in identifier position): literal `{`.
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Render a gen template: `{var}` holes filled from `vals`, `{{` → `{`, `}}`
/// → `}`. Walks left to right so the mustache escapes bind greedily and a hole
/// never matches inside a literal-brace run.
fn render_tmpl(t: &str, vals: &HashMap<String, String>) -> String {
    let b = t.as_bytes();
    let mut out = String::with_capacity(t.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            if b.get(i + 1) == Some(&b'{') {
                out.push('{');
                i += 2;
                continue;
            }
            // Hole ONLY if every byte from i+1 to the closing `}` is a plain
            // identifier char. Same bail-on-non-identifier rule as tmpl_holes,
            // so a `{` followed by `"`, `{`, whitespace, etc. is literal text.
            let mut j = i + 1;
            while j < b.len() && b[j] != b'}' && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            if j < b.len() && b[j] == b'}' && j > i + 1 {
                let name = &t[i + 1..j];
                out.push_str(vals.get(name).map(String::as_str).unwrap_or(""));
                i = j + 1;
                continue;
            }
            out.push('{');
            i += 1;
        } else if b[i] == b'}' {
            if b.get(i + 1) == Some(&b'}') {
                out.push('}');
                i += 2;
                continue;
            }
            out.push('}');
            i += 1;
        } else {
            // Byte-by-byte would corrupt multibyte UTF-8; slice the next run of
            // non-brace bytes (same approach as the lexer's run scanner).
            let start = i;
            while i < b.len() && b[i] != b'{' && b[i] != b'}' {
                i += 1;
            }
            out.push_str(&t[start..i]);
        }
    }
    out
}
