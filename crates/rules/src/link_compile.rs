use crate::types::{LinkPredicate, Side};

/// Compile a [`LinkPredicate`] tree into a SQL WHERE fragment.
///
/// The output is parenthesized so it can be dropped into `AND (...)` safely.
/// String literals in `KindEq` are single-quote escaped.
pub fn compile(pred: &LinkPredicate) -> String {
    match pred {
        LinkPredicate::KindEq { side, value } => {
            let prefix = match side {
                Side::Src => "src_m",
                Side::Tgt => "tgt_m",
            };
            let escaped = value.replace('\'', "''");
            format!("{prefix}.kind = '{escaped}'")
        }
        LinkPredicate::NormEq => "src_s.norm = tgt_s.norm".into(),
        LinkPredicate::Norm2Eq => "src_s.norm2 = tgt_s.norm2".into(),
        LinkPredicate::TargetFileEq => "src_r.target_file_id = tgt_r.file_id".into(),
        LinkPredicate::StringEq => "tgt_r.string_id = src_r.string_id".into(),
        LinkPredicate::SameRepo => "src_f.repo_id = COALESCE(tgt_f.repo_id, tgt_rr.repo_id)".into(),
        LinkPredicate::StemEq { side } => match side {
            Side::Src => "LOWER(src_f.stem) = tgt_s.norm".into(),
            Side::Tgt => "LOWER(tgt_f.stem) = src_s.norm".into(),
        },
        LinkPredicate::ExtEq { side } => match side {
            Side::Src => "LOWER(src_f.ext) = tgt_s.norm".into(),
            Side::Tgt => "LOWER(tgt_f.ext) = src_s.norm".into(),
        },
        LinkPredicate::DirEq { side } => match side {
            Side::Src => "LOWER(src_f.dir) = tgt_s.norm".into(),
            Side::Tgt => "LOWER(tgt_f.dir) = src_s.norm".into(),
        },
        LinkPredicate::And { all } => {
            let parts: Vec<String> = all.iter().map(compile).collect();
            format!("({})", parts.join(" AND "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_eq_src() {
        let pred = LinkPredicate::KindEq {
            side: Side::Src,
            value: "import_name".into(),
        };
        assert_eq!(compile(&pred), "src_m.kind = 'import_name'");
    }

    #[test]
    fn kind_eq_tgt() {
        let pred = LinkPredicate::KindEq {
            side: Side::Tgt,
            value: "export_name".into(),
        };
        assert_eq!(compile(&pred), "tgt_m.kind = 'export_name'");
    }

    #[test]
    fn kind_eq_escapes_quotes() {
        let pred = LinkPredicate::KindEq {
            side: Side::Src,
            value: "it's".into(),
        };
        assert_eq!(compile(&pred), "src_m.kind = 'it''s'");
    }

    #[test]
    fn simple_predicates() {
        assert_eq!(compile(&LinkPredicate::NormEq), "src_s.norm = tgt_s.norm");
        assert_eq!(compile(&LinkPredicate::Norm2Eq), "src_s.norm2 = tgt_s.norm2");
        assert_eq!(compile(&LinkPredicate::TargetFileEq), "src_r.target_file_id = tgt_r.file_id");
        assert_eq!(compile(&LinkPredicate::StringEq), "tgt_r.string_id = src_r.string_id");
        assert_eq!(compile(&LinkPredicate::SameRepo), "src_f.repo_id = COALESCE(tgt_f.repo_id, tgt_rr.repo_id)");
    }

    #[test]
    fn stem_eq() {
        assert_eq!(
            compile(&LinkPredicate::StemEq { side: Side::Src }),
            "LOWER(src_f.stem) = tgt_s.norm",
        );
        assert_eq!(
            compile(&LinkPredicate::StemEq { side: Side::Tgt }),
            "LOWER(tgt_f.stem) = src_s.norm",
        );
    }

    #[test]
    fn ext_eq() {
        assert_eq!(
            compile(&LinkPredicate::ExtEq { side: Side::Src }),
            "LOWER(src_f.ext) = tgt_s.norm",
        );
        assert_eq!(
            compile(&LinkPredicate::ExtEq { side: Side::Tgt }),
            "LOWER(tgt_f.ext) = src_s.norm",
        );
    }

    #[test]
    fn dir_eq() {
        assert_eq!(
            compile(&LinkPredicate::DirEq { side: Side::Src }),
            "LOWER(src_f.dir) = tgt_s.norm",
        );
        assert_eq!(
            compile(&LinkPredicate::DirEq { side: Side::Tgt }),
            "LOWER(tgt_f.dir) = src_s.norm",
        );
    }

    #[test]
    fn and_composition() {
        let pred = LinkPredicate::And {
            all: vec![
                LinkPredicate::KindEq { side: Side::Src, value: "dep_name".into() },
                LinkPredicate::KindEq { side: Side::Tgt, value: "package_name".into() },
                LinkPredicate::NormEq,
            ],
        };
        assert_eq!(
            compile(&pred),
            "(src_m.kind = 'dep_name' AND tgt_m.kind = 'package_name' AND src_s.norm = tgt_s.norm)"
        );
    }

    /// The import_binding predicate produces the same SQL as the raw version.
    #[test]
    fn import_binding_equivalence() {
        let pred = LinkPredicate::And {
            all: vec![
                LinkPredicate::KindEq { side: Side::Src, value: "import_name".into() },
                LinkPredicate::KindEq { side: Side::Tgt, value: "export_name".into() },
                LinkPredicate::TargetFileEq,
                LinkPredicate::StringEq,
            ],
        };
        let compiled = compile(&pred);
        assert_eq!(
            compiled,
            "(src_m.kind = 'import_name' AND tgt_m.kind = 'export_name' AND src_r.target_file_id = tgt_r.file_id AND tgt_r.string_id = src_r.string_id)"
        );
    }

    /// The image_source predicate produces the same SQL as the raw version.
    #[test]
    fn image_source_equivalence() {
        let pred = LinkPredicate::And {
            all: vec![
                LinkPredicate::KindEq { side: Side::Src, value: "image_repo".into() },
                LinkPredicate::KindEq { side: Side::Tgt, value: "package_name".into() },
                LinkPredicate::NormEq,
            ],
        };
        let compiled = compile(&pred);
        assert_eq!(
            compiled,
            "(src_m.kind = 'image_repo' AND tgt_m.kind = 'package_name' AND src_s.norm = tgt_s.norm)"
        );
    }

    #[test]
    fn deserialize_predicate() {
        let json = r#"{
            "op": "and",
            "all": [
                { "op": "kind_eq", "side": "src", "value": "import_name" },
                { "op": "kind_eq", "side": "tgt", "value": "export_name" },
                { "op": "target_file_eq" },
                { "op": "string_eq" }
            ]
        }"#;
        let pred: LinkPredicate = serde_json::from_str(json).unwrap();
        let compiled = compile(&pred);
        assert!(compiled.contains("src_m.kind = 'import_name'"));
        assert!(compiled.contains("tgt_r.string_id = src_r.string_id"));
    }
}
