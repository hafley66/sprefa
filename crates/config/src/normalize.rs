use crate::NormalizeConfig;

/// Primary normalization: strip every non-ASCII-alphanumeric char and lowercase.
/// Collapses naming conventions to one form so cross-repo joins and FTS queries
/// see `AuthService`, `auth_service`, `auth-service`, `auth.service` as identical.
///
/// Used both at ingest time (to write the `strings.norm` column) and at query
/// time (to normalize the MATCH term before handing it to FTS5 trigram). Keep
/// the two paths in sync: if one changes, both must.
pub fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Secondary normalization: apply `normalize`, then strip configured suffixes.
/// Returns None if the result is empty. Suffix strings are normalized the same
/// way before comparison, so `"-service"` matches the `"service"` suffix of an
/// already-normalized `"authservice"`.
pub fn normalize2(value: &str, config: &NormalizeConfig) -> Option<String> {
    let mut s = normalize(value);

    for suffix in &config.strip_suffixes {
        let norm_suffix = normalize(suffix);
        if !norm_suffix.is_empty() && s.ends_with(&norm_suffix) && s.len() > norm_suffix.len() {
            s.truncate(s.len() - norm_suffix.len());
            break;
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
    fn normalize_strips_punctuation_and_whitespace() {
        assert_eq!(normalize("Express"), "express");
        assert_eq!(normalize("  LODASH  "), "lodash");
        assert_eq!(normalize("myOrg/Frontend"), "myorgfrontend");
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("AuthService"), "authservice");
        assert_eq!(normalize("auth_service"), "authservice");
        assert_eq!(normalize("auth-service"), "authservice");
        assert_eq!(normalize("auth service"), "authservice");
        assert_eq!(normalize("Auth.Service"), "authservice");
        assert_eq!(normalize("@scope/pkg-name"), "scopepkgname");
        assert_eq!(normalize("v2.0.1"), "v201");
    }

    #[test]
    fn normalize_cross_convention_equivalence() {
        let forms = [
            "AuthService",
            "authService",
            "auth_service",
            "auth-service",
            "AUTH_SERVICE",
            "Auth Service",
            "auth.service",
        ];
        for s in &forms {
            assert_eq!(normalize(s), "authservice");
        }
    }

    #[test]
    fn normalize2_strips_suffix_after_primary_norm() {
        let c = cfg(&["-service", "-api"]);
        assert_eq!(normalize2("auth-service", &c), Some("auth".to_string()));
        assert_eq!(normalize2("payments-api", &c), Some("payments".to_string()));
        assert_eq!(normalize2("frontend", &c), Some("frontend".to_string()));
        assert_eq!(normalize2("AUTH-SERVICE", &c), Some("auth".to_string()));
        assert_eq!(normalize2("auth_service", &c), Some("auth".to_string()));
    }

    #[test]
    fn normalize2_strips_at_most_one_suffix() {
        let c = cfg(&["-service", "-api"]);
        assert_eq!(normalize2("auth-service-api", &c), Some("authservice".to_string()));
    }

    #[test]
    fn normalize2_no_config_is_identity() {
        let c = cfg(&[]);
        assert_eq!(normalize2("Auth-Service", &c), Some("authservice".to_string()));
        assert_eq!(normalize2("", &c), None);
    }

    #[test]
    fn normalize2_whole_string_equals_suffix_leaves_it() {
        // Guard preserves at least one char: stripping "service" from "service"
        // would empty it, so the strip is skipped and the normalized form is kept.
        let c = cfg(&["service"]);
        assert_eq!(normalize2("Service", &c), Some("service".to_string()));
    }
}
