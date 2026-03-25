use sprefa_config::NormalizeConfig;

/// Primary normalization: lowercase + trim.
/// Every string gets this. Used for FTS search.
pub fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

/// Secondary normalization: strip configured suffixes, then lowercase + trim.
/// Returns None if the result is empty.
/// Used for fuzzy matching across naming conventions (e.g. "auth-service" -> "auth").
pub fn normalize2(value: &str, config: &NormalizeConfig) -> Option<String> {
    let mut s = value.trim().to_lowercase();

    for suffix in &config.strip_suffixes {
        let lower_suffix = suffix.to_lowercase();
        if s.ends_with(&lower_suffix) && s.len() > lower_suffix.len() {
            s.truncate(s.len() - lower_suffix.len());
            break; // strip at most one suffix
        }
    }

    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(suffixes: &[&str]) -> NormalizeConfig {
        NormalizeConfig {
            strip_suffixes: suffixes.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn normalize_lowercases_and_trims() {
        insta::assert_yaml_snapshot!("normalize_basic", vec![
            normalize("Express"),
            normalize("  LODASH  "),
            normalize("myOrg/Frontend"),
            normalize(""),
        ]);
    }

    #[test]
    fn normalize2_strips_suffix() {
        let c = cfg(&["-service", "-api"]);
        insta::assert_yaml_snapshot!("normalize2_strip", vec![
            normalize2("auth-service", &c),
            normalize2("payments-api", &c),
            normalize2("frontend", &c),       // no matching suffix
            normalize2("-service", &c),        // would empty -> None
            normalize2("AUTH-SERVICE", &c),    // case-insensitive
        ]);
    }

    #[test]
    fn normalize2_strips_at_most_one_suffix() {
        let c = cfg(&["-service", "-api"]);
        // "auth-service-api" should only strip the first matching suffix
        let result = normalize2("auth-service-api", &c);
        insta::assert_yaml_snapshot!("normalize2_one_suffix", result);
    }

    #[test]
    fn normalize2_no_config_is_identity() {
        let c = cfg(&[]);
        assert_eq!(normalize2("Auth-Service", &c), Some("auth-service".to_string()));
        assert_eq!(normalize2("", &c), None);
    }
}
