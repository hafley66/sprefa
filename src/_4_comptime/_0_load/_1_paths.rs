//! The SWI path predicates the filesystem grapher calls, as pure string work.
//! `absolute_file_name/2` takes the working directory as an argument rather
//! than reading the process state, which is what keeps the loader pure.

/// `absolute_file_name(+Path, -Absolute)`. Measured on swipl 10.0.2:
/// `/a/b/../c/./d` is `/a/c/d`, `/a//b` is `/a/b`, a trailing `/` survives,
/// and a relative path is resolved against the working directory.
pub fn absolute_file_name(cwd: &str, path: &str) -> String {
    let joined = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("{cwd}/{path}")
    };
    let trailing = joined.len() > 1 && joined.ends_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in joined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    let mut out = String::from("/");
    out.push_str(&segments.join("/"));
    if trailing && !out.ends_with('/') {
        out.push('/');
    }
    out
}

/// `relative_file_name(+Path, +RelToFile, -Relative)`, `filesex.pl:163-192`.
/// The second argument is documented as a FILE, so its last segment is
/// dropped. v7 passes a directory; PLAN fork 1.
pub fn relative_file_name(cwd: &str, path: &str, rel_to: &str) -> String {
    let absolute = absolute_file_name(cwd, path);
    let relative_to = absolute_file_name(cwd, rel_to);
    let left: Vec<&str> = absolute.split('/').collect();
    let right: Vec<&str> = relative_to.split('/').collect();
    let mut shared = 0;
    while shared < left.len() && shared < right.len() && left[shared] == right[shared] {
        shared += 1;
    }
    let rest = &left[shared..];
    let up = right.len() - shared;
    let mut segments: Vec<&str> = vec![".."; up.saturating_sub(1)];
    segments.extend_from_slice(rest);
    if segments.is_empty() {
        ".".to_string()
    } else {
        segments.join("/")
    }
}

/// `directory_file_path(+Dir, +File, -Path)`, `filesex.pl:206-217`.
pub fn directory_file_path(directory: &str, file: &str) -> String {
    if file.starts_with('/') || directory == "." || directory.is_empty() {
        return file.to_string();
    }
    if directory.ends_with('/') {
        format!("{directory}{file}")
    } else {
        format!("{directory}/{file}")
    }
}

fn without_trailing_slash(path: &str) -> &str {
    if path == "/" {
        return path;
    }
    path.trim_end_matches('/')
}

/// `file_base_name/2`. `/a/b/` is `b`, `/` is `/`, `a` is `a`.
pub fn file_base_name(path: &str) -> &str {
    let trimmed = without_trailing_slash(path);
    if trimmed == "/" {
        return "/";
    }
    match trimmed.rfind('/') {
        Some(cut) => &trimmed[cut + 1..],
        None => trimmed,
    }
}

/// `file_directory_name/2`. `/a` is `/`, `a` is `.`, `/a/b/` is `/a`.
pub fn file_directory_name(path: &str) -> &str {
    let trimmed = without_trailing_slash(path);
    if trimmed == "/" {
        return "/";
    }
    match trimmed.rfind('/') {
        Some(0) => "/",
        Some(cut) => &trimmed[..cut],
        None => ".",
    }
}

/// `file_name_extension(-Stem, +Extension, +Name)`, case sensitive on Unix.
pub fn file_stem_for_extension<'a>(name: &'a str, extension: &str) -> Option<&'a str> {
    name.strip_suffix(&format!(".{extension}"))
}
