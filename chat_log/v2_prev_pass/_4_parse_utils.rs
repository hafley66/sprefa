//! Parsing utilities.

/// Simple glob match.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" || pattern == "**/*" {
        return true;
    }
    if pattern.starts_with("**/") {
        return text.ends_with(&pattern[3..]);
    }
    if let Some(star) = pattern.find('*') {
        let prefix = &pattern[..star];
        let suffix = &pattern[star+1..];
        return text.starts_with(prefix) && text.ends_with(suffix);
    }
    text == pattern
}

/// Levenshtein distance.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let len_a = a.chars().count();
    let len_b = b.chars().count();
    if len_a == 0 { return len_b; }
    if len_b == 0 { return len_a; }
    
    let mut prev: Vec<usize> = (0..=len_b).collect();
    let mut curr = vec![0; len_b + 1];
    
    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (curr[j] + 1).min(prev[j + 1] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[len_b]
}
