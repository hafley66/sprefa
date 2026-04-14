//! Shared parsing utilities.
//!
//! Operators use these to parse patterns consistently.
//! Handles: captures with scan annotations, globs, etc.

use crate::{CaptureSource, CrossRef, Location, ScanAnnotation};

/// Parse capture spec like $NAME or $REPO[repo].
/// Returns (name, scan_annotation).
pub fn parse_capture(input: &str) -> Option<(&str, Option<ScanAnnotation>)> {
    // $NAME or $NAME[annotation]
    let input = input.strip_prefix('$')?;
    
    if let Some(bracket) = input.find('[') {
        let name = &input[..bracket];
        let ann = input[bracket+1..].strip_suffix(']')?;
        let scan = match ann {
            "repo" => Some(ScanAnnotation::Repo),
            "rev" => Some(ScanAnnotation::Rev),
            "repo.norm" => Some(ScanAnnotation::RepoNorm),
            "rev.norm" => Some(ScanAnnotation::RevNorm),
            _ => None,
        };
        Some((name, scan))
    } else {
        Some((input, None))
    }
}

/// Parse cross-ref like ${rule.$VAR}.
pub fn parse_cross_ref(input: &str, base_location: &Location) -> Option<CrossRef> {
    // ${rule.VAR}
    let inner = input.strip_prefix("${")?.strip_suffix('}')?;
    let dot = inner.find('.')?;
    let rule = &inner[..dot];
    let var = &inner[dot+1..].trim_start_matches('$');
    
    Some(CrossRef {
        rule: rule.to_string(),
        var: var.to_string(),
        span: base_location.clone(), // TODO: calculate precise offset
    })
}

/// Simple glob match for prototype.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.replace("**/", "");
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return text.starts_with(parts[0]) && text.ends_with(parts[1]);
        }
    }
    text.contains(&pattern)
}

/// Levenshtein distance for "did you mean".
pub fn levenshtein(a: &str, b: &str) -> usize {
    let len_a = a.chars().count();
    let len_b = b.chars().count();
    if len_a == 0 { return len_b; }
    if len_b == 0 { return len_a; }
    
    let mut prev = (0..=len_b).collect::<Vec<_>>();
    let mut curr = vec![0; len_b + 1];
    
    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (curr[j] + 1)
                .min(prev[j + 1] + 1)
                .min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[len_b]
}

/// Find closest match for "did you mean".
pub fn did_you_mean<'a>(input: &str, options: &'a [&str]) -> Option<&'a str> {
    options.iter()
        .min_by_key(|opt| levenshtein(input, opt))
        .copied()
}
