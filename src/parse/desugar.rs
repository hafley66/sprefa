//! `$NAME` regex-hole desugaring, relocated from `parse.rs`
//! (decomposition plan step 11).

/// Desugar `$NAME` holes in a /regex/ literal to lazy named capture groups:
/// `/TODO\($WHO\)/` becomes `TODO\((?P<WHO>.*?)\)`. `\$` (escaped), a bare `$`
/// (the EOL anchor), and `$1`-style digit tails pass through untouched. Runs at
/// parse time so the rule digest, typecheck, and the engine all see the
/// desugared form.
pub(crate) fn desugar_regex_holes(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    // Names already turned into a capture group in THIS pattern. A repeated hole
    // would emit a second `(?P<name>...)`, which the regex crate rejects as a
    // duplicate capture group (christmas #30). The first occurrence captures;
    // repeats dedupe to a non-capturing `.*?` so the pattern compiles and the
    // var binds once. (The crate has no backreferences, so "same value twice"
    // can't be a regex constraint regardless.)
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(c);
            if let Some(c2) = chars.next() { out.push(c2); }
        } else if c == '$' {
            let mut name = String::new();
            while let Some(&c2) = chars.peek() {
                if c2.is_ascii_alphanumeric() || c2 == '_' { name.push(c2); chars.next(); } else { break; }
            }
            if name.is_empty() || name.as_bytes()[0].is_ascii_digit() {
                out.push('$');
                out.push_str(&name);
            } else if seen.insert(name.clone()) {
                out.push_str("(?P<");
                out.push_str(&name);
                out.push_str(">.*?)");
            } else {
                out.push_str("(?:.*?)");
            }
        } else {
            out.push(c);
        }
    }
    out
}
