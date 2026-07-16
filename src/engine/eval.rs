use super::*;

/// Bind `idv` to the located spine id of `[lo, hi)` in `content` and intern the
/// slice. The byte-range core of `bind_whole_match_span`; the `sg` arm calls it
/// with the TRUE match-node range (literal text included) so a `gen(:replace)`
/// keyed off the id rewrites the whole pattern, not just the captures' bbox.
#[allow(clippy::too_many_arguments)]
pub(crate) fn bind_span_id(
    ext: &mut Bind,
    idv: &Option<String>,
    lo: usize,
    hi: usize,
    content: &str,
    where_file: Option<spine::FileId>,
    repo: &str,
    path: &str,
    where_bytes: &mut Vec<(spine::WhereBytes, String)>,
) {
    if let Some(idv) = idv {
        if let Some(file) = where_file {
            if hi > lo && hi <= content.len() {
                let text = &content[lo..hi];
                if !text.is_empty() {
                    let wb = spine::WhereBytes {
                        string: spine::StringId::of(text),
                        file,
                        lo: lo as u32,
                        hi: hi as u32,
                        ..Default::default()
                    };
                    ext.insert(
                        idv.clone(),
                        Value::Text(spine::WhereBytesId::of_located(wb, repo, path).to_string()),
                    );
                    where_bytes.push((
                        spine::WhereBytes {
                            string: spine::StringId::of(text),
                            file,
                            lo: lo as u32,
                            hi: hi as u32,
                            ..Default::default()
                        },
                        text.to_string(),
                    ));
                }
            }
        }
    }
}

/// The dl authoring note appended to every regex compile error (parse-only AND
/// the runtime scan/eval path). Points at the Rust-regex escape: the crate has
/// no look-around or backrefs, so anchor instead.
pub const DL_REGEX_NOTE: &str =
    "\nnote: regexes are Rust-flavor: no lookahead/lookbehind/backrefs; \
     anchor with $, \\b, or character classes.";

/// Compile a dl regex literal EXACTLY as the scan/eval path does — the single
/// construction point so `--parse-only` and the runtime can never drift on
/// flags — and carry the dl authoring note on any compile error. Every
/// `match`/`comment`/`=~` regex goes through here.
pub fn compile_dl_regex(pattern: &str) -> Result<Regex> {
    Regex::new(pattern).map_err(|e| anyhow::anyhow!("{e}{DL_REGEX_NOTE}"))
}

/// Compile every regex literal the program carries (`match`, `comment` open/
/// close, `=~` body constraints) through `compile_dl_regex`, turning each
/// compile failure into an error `TypeDiag`. Lets `--parse-only` reject an
/// unsupported pattern (`/(?!-)/`) without paying a scan — the runtime would
/// otherwise be the first to fail, mid-scan. `path` attributes the diags (line
/// 1, the same coarseness as the other parse-only diagnostics). Reports ALL bad
/// regexes, not the first only.
pub fn regex_literal_diags(prog: &Program, path: &str) -> Vec<TypeDiag> {
    fn push(out: &mut Vec<TypeDiag>, path: &str, pat: &str) {
        if let Err(e) = compile_dl_regex(pat) {
            out.push(TypeDiag {
                path: path.to_string(),
                span: (0, 0),
                severity: Severity::Error,
                code: "regex".to_string(),
                msg: e.to_string(),
            });
        }
    }
    let mut out = Vec::new();
    for item in &prog.items {
        let Item::Rule(r) = item else { continue };
        for b in &r.body {
            match b {
                BodyItem::Match { regex, .. } => push(&mut out, path, regex),
                BodyItem::Comment { open, close, .. } => {
                    push(&mut out, path, open);
                    if let Some(c) = close {
                        push(&mut out, path, c);
                    }
                }
                BodyItem::Cmp(c) if c.op == CmpOp::Match => {
                    if let Term::Str(s) = &c.rhs {
                        push(&mut out, path, s);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// `match(regex, line, [id])` body op. For each input bind, scan every line of
/// `content`; for each regex capture set produce an extended bind: the line
/// number (into `line`), each named capture (into its name), and — when `id`
/// is requested (5-arg form) — a whole-match spine id plus its span. Extracted
/// from `parse_file`'s Match arm (iteration-2 god-fn split). `re_cache`
/// memoizes compiled regexes across items. `push_span`'s guard (only when the
/// file has a content-addressed id and the text is non-empty) is inlined.
pub(crate) fn bind_match_op(
    binds: &[Bind],
    regex: &str,
    mlv: &Option<String>,
    idv: &Option<String>,
    colv: &Option<String>,
    ecv: &Option<String>,
    content: &str,
    where_file: Option<spine::FileId>,
    re_cache: &mut HashMap<String, Regex>,
    where_bytes: &mut Vec<(spine::WhereBytes, String)>,
    repo: &str,
    path: &str,
) -> Result<Vec<Bind>> {
    if !re_cache.contains_key(regex) {
        re_cache.insert(regex.to_string(), compile_dl_regex(regex)?);
    }
    let re = &re_cache[regex];
    let names: Vec<&str> = re.capture_names().flatten().collect();
    let mut next: Vec<Bind> = Vec::new();
    let base = content.as_ptr() as usize;
    for b in binds {
        for (lineno, ln) in content.lines().enumerate() {
            let line_off = ln.as_ptr() as usize - base;
            for caps in re.captures_iter(ln) {
                let mut ext = b.clone();
                if let Some(v) = mlv {
                    ext.insert(v.clone(), Value::Int((lineno + 1) as i64));
                }
                if colv.is_some() || ecv.is_some() {
                    if let Some(m0) = caps.get(0) {
                        // Whole-match span, 0-based byte columns within the line.
                        if let Some(v) = colv {
                            ext.insert(v.clone(), Value::Int(m0.start() as i64));
                        }
                        if let Some(v) = ecv {
                            ext.insert(v.clone(), Value::Int(m0.end() as i64));
                        }
                    }
                }
                if let Some(idv) = idv {
                    if let Some(file) = where_file {
                        if let Some(m0) = caps.get(0) {
                            let text = m0.as_str();
                            let lo = line_off + m0.start();
                            let hi = line_off + m0.end();
                            if !text.is_empty() {
                                let wb = spine::WhereBytes {
                                    string: spine::StringId::of(text),
                                    file,
                                    lo: lo as u32,
                                    hi: hi as u32,
                                    ..Default::default()
                                };
                                ext.insert(
                                    idv.clone(),
                                    Value::Text(
                                        spine::WhereBytesId::of_located(wb, repo, path).to_string(),
                                    ),
                                );
                                where_bytes.push((
                                    spine::WhereBytes {
                                        string: spine::StringId::of(text),
                                        file,
                                        lo: lo as u32,
                                        hi: hi as u32,
                                        ..Default::default()
                                    },
                                    text.to_string(),
                                ));
                            }
                        }
                    }
                }
                for n in &names {
                    if let Some(m) = caps.name(n) {
                        let text = m.as_str();
                        ext.insert((*n).to_string(), Value::Text(text.to_string()));
                        if let Some(file) = where_file {
                            if !text.is_empty() {
                                where_bytes.push((
                                    spine::WhereBytes {
                                        string: spine::StringId::of(text),
                                        file,
                                        lo: (line_off + m.start()) as u32,
                                        hi: (line_off + m.end()) as u32,
                                        ..Default::default()
                                    },
                                    text.to_string(),
                                ));
                            }
                        }
                    }
                }
                next.push(ext);
            }
        }
    }
    Ok(next)
}

/// True when an engine read error is a non-UTF-8 decode failure (the io error
/// `read_to_string` raises as `ErrorKind::InvalidData`), as opposed to a
/// missing file or a git-object read failure.
fn is_invalid_utf8(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|io| io.kind() == std::io::ErrorKind::InvalidData)
}

/// Parse one file for one source rule (no DB access); returns (rows, dropped).
/// Safe to call in parallel: reads file content, runs extractors, builds rows.
#[tracing::instrument(skip_all, fields(repo = repo, path = path), level = "trace")]
pub(crate) fn parse_file(
    rule: &Rule,
    repo: &str,
    path: &str,
    rev: &str,
    hash: &str,
    root: &Path,
    rels: &Rels,
    rev_index: &HashSet<(String, String, String)>,
    head_binds: &[(String, String)],
    content: Option<&str>,
) -> Result<(Vec<Vec<Value>>, Vec<(spine::WhereBytes, String)>, usize)> {
    let spec = scan_spec_of(rule)?;
    let pathvar = spec.path_var;
    let revvar = spec.rev_out_var;
    let cmps: Vec<&Constraint> = rule
        .body
        .iter()
        .filter_map(|i| {
            if let BodyItem::Cmp(c) = i {
                Some(c)
            } else {
                None
            }
        })
        .collect();
    let content: std::borrow::Cow<'_, str> = match content {
        Some(content) => std::borrow::Cow::Borrowed(content),
        None => match read_content(root, rev, path) {
            Ok(content) => std::borrow::Cow::Owned(content),
            // A non-UTF-8 source file (e.g. a Latin-1 test fixture in a C repo)
            // is out of scope for text/ast/sg extraction: skip it (empty content
            // -> zero rows) rather than aborting the whole tick. Every other
            // content reader degrades read errors to empty via unwrap_or_default;
            // this narrows that to the invalid-UTF-8 case so a genuinely missing
            // file still errors loudly. Lossy decode is rejected on purpose: it
            // shifts byte offsets and would emit wrong match/ast spans.
            Err(error) if is_invalid_utf8(&error) => std::borrow::Cow::Borrowed(""),
            Err(error) => return Err(error),
        },
    };
    // Ref-spine: locate each capture's bytes in the file content. The file id is
    // derived from the same stored content address `_files` uses (blake3 for
    // WORK, blob OID for a git rev), so located rows join `_files` for both.
    let where_file = spine::FileId::from_content_address(hash, content.len() as i64)
        .filter(|f| *f != spine::FileId::SYNTHETIC);
    let mut where_bytes: Vec<(spine::WhereBytes, String)> = Vec::new();
    let push_span =
        |text: &str, lo: usize, hi: usize, where_bytes: &mut Vec<(spine::WhereBytes, String)>| {
            if let Some(file) = where_file {
                if !text.is_empty() {
                    // Carry the located text alongside its span so the flush interns
                    // BOTH `_where_bytes` (the span) AND `_strings` (the text, under
                    // the SAME StringId the WhereBytes hashes). Without the text, a
                    // located id (capture span, `match`/`ast` whole-match id) resolves
                    // through `ref(id,_,_,lo,hi)` but NOT `string(id,text,norm)`.
                    where_bytes.push((
                        spine::WhereBytes {
                            string: spine::StringId::of(text),
                            file,
                            lo: lo as u32,
                            hi: hi as u32,
                            ..Default::default()
                        },
                        text.to_string(),
                    ));
                }
            }
        };
    let bind_captures =
        |ext: &mut Bind,
         caps: &[(String, String, usize, usize)],
         where_bytes: &mut Vec<(spine::WhereBytes, String)>| {
            for (n, t, lo, hi) in caps {
                ext.insert(n.clone(), Value::Text(t.clone()));
                push_span(t, *lo, *hi, where_bytes);
            }
        };
    let head_meta = rels
        .get(&rule.head.rel)
        .ok_or_else(|| anyhow::anyhow!("unknown head relation {}", rule.head.rel))?;
    let mut re_cache: HashMap<String, Regex> = HashMap::new();

    let mut binds: Vec<Bind> = vec![{
        let mut b = Bind::new();
        b.insert(pathvar.clone(), Value::Text(path.to_string()));
        if let Some(rv) = &revvar {
            b.insert(rv.clone(), Value::Text(rev.to_string()));
        }
        // Data-driven coordinate values (the variable repo/rev this file was
        // scanned under): seed them so the rule head can reference them.
        for (k, v) in head_binds {
            b.insert(k.clone(), Value::Text(v.clone()));
        }
        b
    }];

    for item in &rule.body {
        match item {
            BodyItem::Match {
                regex,
                line,
                id,
                col,
                end_col,
                ..
            } => {
                let mlv = opt_var(line)?;
                let idv = id.as_ref().map(var_of).transpose()?;
                let colv = col.as_ref().map(opt_var).transpose()?.flatten();
                let ecv = end_col.as_ref().map(opt_var).transpose()?.flatten();
                binds = bind_match_op(
                    &binds,
                    regex,
                    &mlv,
                    &idv,
                    &colv,
                    &ecv,
                    &content,
                    where_file,
                    &mut re_cache,
                    &mut where_bytes,
                    repo,
                    path,
                )?;
            }
            BodyItem::Ast {
                lang,
                query,
                line,
                end,
                id,
                ..
            } => {
                let alv = opt_var(line)?;
                let elv = end.as_ref().map(opt_var).transpose()?.flatten();
                // Optional 7th arg: the spine id of the WHOLE ast match span (the
                // captures' min..max byte range). Joins `ref(id, _, _, lo, hi)`
                // for the codemod anchor — the bytes this match covered — same
                // located-id shape as `match`'s 5th arg (christmas #9). The text
                // interned for both the id and the span is the literal source
                // slice over that range, so `node`/`ref`/`string` all agree.
                let idv = id.as_ref().map(var_of).transpose()?;
                let hits = run_ts(&content, lang, query)?;
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (start, endln, caps) in &hits {
                        let mut ext = b.clone();
                        if let Some(v) = &alv {
                            ext.insert(v.clone(), Value::Int(*start));
                        }
                        if let Some(ev) = &elv {
                            ext.insert(ev.clone(), Value::Int(*endln));
                        }
                        // Whole-match span = the captures' min lo .. max hi. Push
                        // it (interning the contiguous source slice) and bind its
                        // located id before the per-capture spans, mirroring the
                        // `match` arm. Skipped when no captures carry a span.
                        bind_whole_match_span(
                            &mut ext,
                            &idv,
                            caps,
                            &content,
                            where_file,
                            repo,
                            path,
                            &mut where_bytes,
                        );
                        bind_captures(&mut ext, caps, &mut where_bytes);
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::Sg {
                lang,
                pattern,
                line,
                col,
                end_line,
                end_col,
                id,
                ..
            } => {
                let slv = opt_var(line)?;
                let clv = opt_var(col)?;
                let ellv = opt_var(end_line)?;
                let eclv = opt_var(end_col)?;
                // Optional trailing `id`: the spine id of the whole sg match span
                // (captures' min lo .. max hi), same located-id shape as `ast`/
                // `match` (christmas #9, decision 3). Resolves via `ref` AND
                // `string` (rides step 1's intern of the slice text).
                let idv = id.as_ref().map(var_of).transpose()?;
                // prefilter: a file lacking any literal token cannot match
                let lits = pattern_literals(pattern);
                if !lits.iter().all(|t| content.contains(t.as_str())) {
                    binds = Vec::new();
                    continue;
                }
                let hits = crate::sg::run_sg(&content, lang, pattern)?;
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (ln, c, eln, ec, mlo, mhi, caps) in &hits {
                        let mut ext = b.clone();
                        if let Some(v) = &slv {
                            ext.insert(v.clone(), Value::Int(*ln));
                        }
                        if let Some(v) = &clv {
                            ext.insert(v.clone(), Value::Int(*c));
                        }
                        if let Some(v) = &ellv {
                            ext.insert(v.clone(), Value::Int(*eln));
                        }
                        if let Some(v) = &eclv {
                            ext.insert(v.clone(), Value::Int(*ec));
                        }
                        // id = the TRUE whole-match byte range (literal text incl.),
                        // so gen(:replace, ref(id)) rewrites the entire pattern.
                        bind_span_id(
                            &mut ext,
                            &idv,
                            *mlo,
                            *mhi,
                            &content,
                            where_file,
                            repo,
                            path,
                            &mut where_bytes,
                        );
                        bind_captures(&mut ext, caps, &mut where_bytes);
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::AstYaml {
                lang,
                yaml,
                line,
                col,
                end_line,
                end_col,
                ..
            } => {
                let slv = opt_var(line)?;
                let clv = opt_var(col)?;
                let ellv = opt_var(end_line)?;
                let eclv = opt_var(end_col)?;
                // No literal-prefilter (the YAML body is structural, not a
                // plain token set like a pattern); the RuleCore matcher is
                // already cheap on a non-matching file.
                let hits = crate::sg::run_ast_yaml(&content, lang, yaml)?;
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (ln, c, eln, ec, _mlo, _mhi, caps) in &hits {
                        let mut ext = b.clone();
                        if let Some(v) = &slv {
                            ext.insert(v.clone(), Value::Int(*ln));
                        }
                        if let Some(v) = &clv {
                            ext.insert(v.clone(), Value::Int(*c));
                        }
                        if let Some(v) = &ellv {
                            ext.insert(v.clone(), Value::Int(*eln));
                        }
                        if let Some(v) = &eclv {
                            ext.insert(v.clone(), Value::Int(*ec));
                        }
                        bind_captures(&mut ext, caps, &mut where_bytes);
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::Cmd {
                template,
                line,
                out,
                ..
            } => {
                let lv = opt_var(line)?;
                let ov = opt_var(out)?;
                // Budget guard: one cmd rule shells out once per matched file, so
                // a broad glob is a subprocess storm. Over budget = a loud bail
                // naming the command, never a silent truncation of the relation.
                if let Some(budget) = cmd_budget() {
                    let n = CMD_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if n > budget {
                        bail!(
                            "cmd budget exceeded: tick needs more than {budget} `cmd` \
                               invocation(s) (next: `{template}` on {path}) — raise \
                               --cmd-budget / DL_CMD_BUDGET or narrow the scan glob"
                        );
                    }
                }
                // {file}: WORK reads the on-disk path; a git rev materializes the
                // cached content to a content-addressed temp file (reused across ticks)
                let file_arg = if rev == "WORK" {
                    root.join(path).display().to_string()
                } else {
                    let tmp = std::env::temp_dir().join(format!("dl_cmd_{hash}"));
                    if !tmp.is_file() {
                        std::fs::write(&tmp, content.as_bytes())?;
                    }
                    tmp.display().to_string()
                };
                let cmdline = template
                    .replace("{file}", &file_arg)
                    .replace("{path}", path)
                    .replace("{root}", &root.display().to_string());
                let t_cmd = std::time::Instant::now();
                let output = Command::new("sh")
                    .arg("-c")
                    .arg(&cmdline)
                    .current_dir(root)
                    .output()?;
                if crate::db::profiling() && t_cmd.elapsed().as_millis() >= 250 {
                    eprintln!(
                        "[cmd {:.0}ms] {cmdline}",
                        t_cmd.elapsed().as_secs_f64() * 1000.0
                    );
                }
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                // nonzero exit WITH stdout is the diff-tool convention (findings
                // exist); nonzero with empty stdout is a broken command, be loud
                if !output.status.success() && stdout.trim().is_empty() {
                    bail!(
                        "cmd `{cmdline}` failed (exit {:?}): {}",
                        output.status.code(),
                        String::from_utf8_lossy(&output.stderr).trim()
                    );
                }
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (i, ln) in stdout.lines().enumerate() {
                        let mut ext = b.clone();
                        if let Some(v) = &lv {
                            ext.insert(v.clone(), Value::Int((i + 1) as i64));
                        }
                        if let Some(v) = &ov {
                            ext.insert(v.clone(), Value::Text(ln.to_string()));
                        }
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::Comment {
                open,
                close,
                l0,
                l1,
                label,
                ..
            } => {
                let l0v = opt_var(l0)?;
                let l1v = opt_var(l1)?;
                let labv = opt_var(label)?;
                if !re_cache.contains_key(open) {
                    re_cache.insert(open.clone(), compile_dl_regex(open)?);
                }
                if let Some(c) = close {
                    if !re_cache.contains_key(c) {
                        re_cache.insert(c.clone(), compile_dl_regex(c)?);
                    }
                }
                let open_re = &re_cache[open];
                let close_re = close.as_ref().map(|c| &re_cache[c]);
                let regions = crate::comment::run_comment(&content, open_re, close_re);
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for r in &regions {
                        let mut ext = b.clone();
                        if let Some(v) = &l0v {
                            ext.insert(v.clone(), Value::Int(r.l0));
                        }
                        if let Some(v) = &l1v {
                            ext.insert(v.clone(), Value::Int(r.l1));
                        }
                        if let Some(v) = &labv {
                            ext.insert(v.clone(), Value::Text(r.label.clone()));
                        }
                        if let Some((lo, hi)) = r.label_span {
                            push_span(&r.label, lo, hi, &mut where_bytes);
                        }
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::JsonP { jpath, out, id, .. } => {
                let ov = opt_var(out)?;
                // Optional trailing `id`: the spine id of the matched value's byte
                // span. For json the value span IS the whole match (christmas #9,
                // decision 3). Resolves via `ref` AND `string`.
                let idv = id.as_ref().map(var_of).transpose()?;
                let vals = crate::datapath::run_data(path, &content, jpath);
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (v, lo, hi) in &vals {
                        let mut ext = b.clone();
                        if let Some(ov) = &ov {
                            ext.insert(ov.clone(), Value::Text(v.clone()));
                        }
                        if let Some(idv) = &idv {
                            if let Some(file) = where_file {
                                if !v.is_empty() {
                                    let wb = spine::WhereBytes {
                                        string: spine::StringId::of(v),
                                        file,
                                        lo: *lo as u32,
                                        hi: *hi as u32,
                                        ..Default::default()
                                    };
                                    ext.insert(
                                        idv.clone(),
                                        Value::Text(
                                            spine::WhereBytesId::of_located(wb, repo, path)
                                                .to_string(),
                                        ),
                                    );
                                }
                            }
                        }
                        push_span(v, *lo, *hi, &mut where_bytes);
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::Json { pat, .. } => {
                // Declarative brace pattern. The body was validated at parse
                // time; re-parse to get the Step tree (cheap; pattern is tiny)
                // and walk it. Each match binds N captures by name into the
                // row, like match's named groups.
                let (steps, _) = crate::datapath::parse_pattern(pat)
                    .map_err(|e| anyhow::anyhow!("json pattern error: {e}"))?;
                let ms = crate::datapath::run_pattern(path, &content, &steps);
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for m in &ms {
                        let mut ext = b.clone();
                        for (cap, text, lo, hi) in m {
                            ext.insert(cap.clone(), Value::Text(text.clone()));
                            push_span(text, *lo, *hi, &mut where_bytes);
                        }
                        next.push(ext);
                    }
                }
                binds = next;
            }
            _ => {}
        }
    }

    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut dropped = 0usize;
    'bind: for b in binds {
        for c in &cmps {
            if !eval_cmp(c, &b)? {
                continue 'bind;
            }
        }
        let mut row = Vec::with_capacity(head_meta.cols.len());
        for (i, term) in rule.head.terms.iter().enumerate() {
            let v = match term {
                Term::Var(v) => b.get(v).cloned()
                    .ok_or_else(|| {
                        let mut msg = format!(
                            "head var `{v}` is not bound by any source op in this rule. A source rule \
                             (scan/match/ast/sg/json) binds head vars only from the source op's own \
                             captures — a join to `repo(...)`/`file(...)` in the body cannot supply it. \
                             To fan a scan over every configured repo AND capture which repo each row \
                             came from, put `{v}` in scan's repo slot: \
                             `... <- repo({v}, _, _), scan({v}, rev, glob, path, rev_out).`");
                        // sg/ast_yaml `$$$NAME` is a MULTI metavar (pattern
                        // structure), never a single-node capture, so it binds no
                        // head var. Name the fix when that is what happened.
                        let is_structural_metavar = rule.body.iter().any(|bi| match bi {
                            BodyItem::Sg { pattern, .. } => pattern.contains(&format!("$$${v}")),
                            BodyItem::AstYaml { yaml, .. } => yaml.contains(&format!("$$${v}")),
                            _ => false,
                        });
                        if is_structural_metavar {
                            msg.push_str(&format!(
                                "\nnote: $$${v} is pattern structure only; bind a single node with \
                                 ${v} or use the span outputs."));
                        }
                        anyhow::anyhow!(msg)
                    })?,
                Term::Str(s) => Value::Text(s.clone()),
                Term::Int(n) => Value::Int(*n),
                Term::Interp(parts) => interp_value(parts, &b)?,
                // A Wild head slot is head named-arg padding (a diag rule that
                // names only some columns). Emit NULL; the reader defaults it.
                Term::Wild => Value::Null,
                Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
                Term::Arith { .. } => val_of(term, &b)?,
                Term::Call { .. } => val_of(term, &b)?,
            };
            // NULL (a padded column) has no type to check; the file/path checks
            // would drop it. Only type-check present values.
            if !matches!(v, Value::Null)
                && !check_type(head_meta.cols[i].ty, &v, repo, rev, root, rev_index)
            {
                dropped += 1;
                continue 'bind;
            }
            row.push(v);
        }
        rows.push(row);
    }
    Ok((rows, where_bytes, dropped))
}

pub(crate) fn row_hash(row: &[Value]) -> String {
    let mut s = String::new();
    for (i, v) in row.iter().enumerate() {
        if i > 0 {
            s.push('\u{1}');
        }
        s.push_str(&v.as_str());
    }
    blake3::hash(s.as_bytes()).to_hex().to_string()
}

pub(crate) fn str_of(t: &Term) -> Result<String> {
    match t {
        Term::Str(s) => Ok(s.clone()),
        _ => bail!("expected string literal, got {t:?}"),
    }
}
pub(crate) fn var_of(t: &Term) -> Result<String> {
    match t {
        Term::Var(v) => Ok(v.clone()),
        _ => bail!("expected variable, got {t:?}"),
    }
}

/// Like `var_of` but accepts `Term::Wild` (`_`) — returns None so the caller
/// skips binding that output. Backs the kwarg/`_` output forms: an unmentioned
/// or `_` op output produces its row value but binds nothing.
pub(crate) fn opt_var(t: &Term) -> Result<Option<String>> {
    match t {
        Term::Var(v) => Ok(Some(v.clone())),
        Term::Wild => Ok(None),
        other => bail!("expected variable or `_`, got {other:?}"),
    }
}

/// Build an interpolated string from bindings: `"${ty}::${name}"` -> "Foo::bar".
pub(crate) fn interp_value(parts: &[InterpPart], b: &Bind) -> Result<Value> {
    let mut s = String::new();
    for p in parts {
        match p {
            InterpPart::Lit(l) => s.push_str(l),
            InterpPart::Var(v) => s.push_str(
                &b.get(v)
                    .ok_or_else(|| anyhow::anyhow!("unbound var {v} in interpolation"))?
                    .as_str(),
            ),
        }
    }
    Ok(Value::Text(s))
}

/// text -> int the way SQLite `CAST(x AS INTEGER)` does: skip leading whitespace,
/// take an optional sign and the leading digit run, parse that; no digits -> 0.
/// Keeps the source-rule (Rust) path identical to the derived (SQL) path.
pub(crate) fn cast_int(s: &str) -> i64 {
    let t = s.trim_start();
    let bytes = t.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
        i += 1;
    }
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return 0;
    }
    t[..i].parse::<i64>().unwrap_or(0)
}

pub(crate) fn val_of(t: &Term, b: &Bind) -> Result<Value> {
    match t {
        Term::Var(v) => b.get(v).cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "unbound var {v} in constraint\nnote: to compute a new value in a SOURCE rule \
             (scan/match/ast/...), put the expression in the rule head: head(path, line+1) <- ...; \
             body binds (`ext = split(path, \".\", -1)`) work in derived-rule bodies only"
            )
        }),
        Term::Str(s) => Ok(Value::Text(s.clone())),
        Term::Int(n) => Ok(Value::Int(*n)),
        Term::Interp(parts) => interp_value(parts, b),
        Term::Wild => bail!("'_' in constraint"),
        Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
        Term::Arith { op, lhs, rhs } => {
            let (l, r) = (val_of(lhs, b)?, val_of(rhs, b)?);
            // `+` over two text values concatenates (the source-rule twin of the
            // derived `||` lowering); every other combination stays int-only.
            if let (ArithOp::Add, Value::Text(ls), Value::Text(rs)) = (op, &l, &r) {
                return Ok(Value::Text(format!("{ls}{rs}")));
            }
            let (Value::Int(a), Value::Int(c)) = (&l, &r) else {
                if matches!(op, ArithOp::Add) {
                    bail!("cannot `+` int and text — interpolate (\"${{count}}${{name}}\") or convert with int(..)");
                }
                bail!(
                    "arithmetic needs int operands, got {l:?} {} {r:?}",
                    op.sql()
                );
            };
            Ok(Value::Int(match op {
                ArithOp::Add => a + c,
                ArithOp::Sub => a - c,
                ArithOp::Mul => a * c,
                ArithOp::Div => {
                    if *c == 0 {
                        bail!("division by zero in source-rule arithmetic");
                    }
                    a / c
                }
                ArithOp::Mod => {
                    if *c == 0 {
                        bail!("modulo by zero in source-rule arithmetic");
                    }
                    a % c
                }
            }))
        }
        Term::Call { name, args } => {
            let vals: Vec<Value> = args.iter().map(|a| val_of(a, b)).collect::<Result<_>>()?;
            let str_at = |i: usize| {
                vals.get(i)
                    .and_then(|v| match v {
                        Value::Text(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .ok_or_else(|| anyhow::anyhow!("function `{name}` arg {i} must be text"))
            };
            let int_at = |i: usize| {
                vals.get(i)
                    .and_then(|v| match v {
                        Value::Int(n) => Some(*n),
                        _ => None,
                    })
                    .ok_or_else(|| anyhow::anyhow!("function `{name}` arg {i} must be int"))
            };
            match name.as_str() {
                "replace" => {
                    let (text, from, to) = (str_at(0)?, str_at(1)?, str_at(2)?);
                    Ok(Value::Text(text.replace(from, to)))
                }
                "split" => {
                    let (text, sep) = (str_at(0)?, str_at(1)?);
                    let idx = int_at(2)?;
                    if sep.is_empty() {
                        bail!("function split: empty separator");
                    }
                    let parts: Vec<&str> = text.split(sep).collect();
                    let n = parts.len() as i64;
                    let i = if idx >= 0 { idx } else { idx + n };
                    if i < 0 || i >= n {
                        bail!("function split: idx {idx} out of range ({n} parts)");
                    }
                    Ok(Value::Text(parts[i as usize].to_string()))
                }
                // text -> int, mirroring SQLite `CAST(.. AS INTEGER)`: leading
                // optional sign + digit run, anything else (incl. garbage) -> 0.
                "int" => Ok(Value::Int(cast_int(str_at(0)?))),
                other => bail!("unknown function `{other}` (known: split, replace, int)"),
            }
        }
    }
}

pub(crate) fn eval_cmp(c: &Constraint, b: &Bind) -> Result<bool> {
    let l = val_of(&c.lhs, b)?;
    let r = val_of(&c.rhs, b)?;
    // Pattern ops: lhs value tested against rhs pattern (a literal string).
    match c.op {
        CmpOp::Match => {
            let re = compile_dl_regex(&r.as_str())?;
            return Ok(re.is_match(&l.as_str()));
        }
        CmpOp::Glob => {
            let g = globset::Glob::new(&r.as_str())?.compile_matcher();
            return Ok(g.is_match(l.as_str()));
        }
        _ => {}
    }
    let ord = match (&l, &r) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        _ => l.as_str().cmp(&r.as_str()),
    };
    Ok(match c.op {
        CmpOp::Eq => ord.is_eq(),
        CmpOp::Ne => ord.is_ne(),
        CmpOp::Lt => ord.is_lt(),
        CmpOp::Le => ord.is_le(),
        CmpOp::Gt => ord.is_gt(),
        CmpOp::Ge => ord.is_ge(),
        CmpOp::Match | CmpOp::Glob => unreachable!("handled above"),
    })
}

// Vendored grammar entry points, compiled by build.rs from vendor/grammars/.
// The C signature is `const TSLanguage *tree_sitter_X(void)`; declared here as
// `*const ()` (opaque) so tree_sitter_language::LanguageFn::from_raw accepts
// it. go-template has no crate; dockerfile's only crate pins tree-sitter 0.20.
extern "C" {
    pub(crate) fn tree_sitter_gotmpl() -> *const ();
    pub(crate) fn tree_sitter_dockerfile() -> *const ();
}

// LANG-JUNCTION(ast-grammars): one table row = `ast` op support (tree-sitter constructor keyed by label); `comment_node` and the CST node/child rels also dispatch through `ts_lang`, via `cst::lang_label_for_path`
/// The tree-sitter grammar table for the `ast` op (S-expression queries):
/// `(canonical name, [extra aliases], constructor)`. Single source of truth so
/// `ts_lang` (the resolver), the bail message, and `ast_langs` (the list the
/// skill language matrix must match) can never drift. Adding a grammar here
/// without updating the skill matrix fails the matrix-honesty test. Distinct
/// from `sg`'s table: the `ast` op runs tree-sitter, `sg`/`ast_yaml` run
/// ast-grep — the language sets differ (e.g. `ast` has bash/hcl/gotmpl but no
/// tsx; `sg` has tsx/typescript/cpp but no bash). The non-capturing closures
/// coerce to `fn` pointers, so this promotes to a `&'static` slice.
/// Run a tree-sitter S-expression query over file content.
/// Returns (start_line, end_line, captures) per match; start = min capture start
/// row, end = max capture end row (the matched region's span). Each capture is
/// `(name, text, lo, hi)` where `[lo, hi)` is the node's byte range in `content`.
pub(crate) fn run_ts(
    content: &str,
    lang: &str,
    query_str: &str,
) -> Result<Vec<(i64, i64, Vec<(String, String, usize, usize)>)>> {
    use streaming_iterator::StreamingIterator;
    let language = ts_lang(lang)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language)?;
    let tree = parser
        .parse(content, None)
        .ok_or_else(|| anyhow::anyhow!("ast parse failed"))?;
    let query = tree_sitter::Query::new(&language, query_str)?;
    let names = query.capture_names();
    let src = content.as_bytes();
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut out = Vec::new();
    let mut it = cursor.matches(&query, tree.root_node(), src);
    while let Some(m) = it.next() {
        let mut caps = Vec::new();
        let mut line = i64::MAX;
        let mut end = 1i64;
        for c in m.captures {
            let name = names[c.index as usize].to_string();
            let text = c.node.utf8_text(src).unwrap_or("").to_string();
            line = line.min(c.node.start_position().row as i64 + 1);
            end = end.max(c.node.end_position().row as i64 + 1);
            caps.push((name, text, c.node.start_byte(), c.node.end_byte()));
        }
        if line == i64::MAX {
            line = 1;
        }
        out.push((line, end, caps));
    }
    Ok(out)
}
/// Bind the spine id of a whole-match span (captures' min lo .. max hi) and
/// intern the slice. Shared by the `ast` and `sg` arms of `parse_file`, which
/// carried identical copies; extracted as the first measured refactor of the
/// reward-validated consolidation policy (verbatim block dup → one helper).
/// Mirrors `match`'s 5th-arg id binding. No-op when the id var is absent, the
/// file has no content-addressed id, or the span is empty/invalid.
pub(crate) fn bind_whole_match_span(
    ext: &mut Bind,
    idv: &Option<String>,
    caps: &[(String, String, usize, usize)],
    content: &str,
    where_file: Option<spine::FileId>,
    repo: &str,
    path: &str,
    where_bytes: &mut Vec<(spine::WhereBytes, String)>,
) {
    let lo = caps.iter().map(|(_, _, lo, _)| *lo).min();
    let hi = caps.iter().map(|(_, _, _, hi)| *hi).max();
    if let (Some(lo), Some(hi)) = (lo, hi) {
        bind_span_id(
            ext,
            idv,
            lo,
            hi,
            content,
            where_file,
            repo,
            path,
            where_bytes,
        );
    }
}
