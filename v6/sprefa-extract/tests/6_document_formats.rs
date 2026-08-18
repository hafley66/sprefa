//! WHICH DOCUMENT FORMATS THE EXTRACTOR ACTUALLY HANDLES, pinned.
//!
//! Each row names the FAMILY the format's facts must carry. json/yaml/toml ride
//! the `data` family; html/css ride the ast-grep cst fallback; md rides
//! tree-sitter-md; xml has no grammar in tree and produces nothing.
//!
//! Adding or losing a grammar flips a row here, which is the point: a dependency
//! bump that silently drops a format should not be silent.
// @comment-ok: the header is the coverage table's own contract, not narrative

use std::process::Command;

/// (extension, the family tag its facts must carry, why). An empty family is a
/// real absence and costs a new grammar dependency plus a `Source`, so it is a
/// build-vs-buy decision rather than a cleanup.
const FORMATS: &[(&str, &str, &str)] = &[
    ("html", "cst", "ast-grep-language ships the html grammar"),
    ("yaml", "data", "the data family, tree-sitter-yaml"),
    ("json", "data", "the data family, tree-sitter-json"),
    ("css", "cst", "ast-grep-language ships the css grammar"),
    ("md", "cst", "tree-sitter-md block and inline grammars"),
    ("toml", "data", "the data family, tree-sitter-toml-ng"),
    ("xml", "", "no xml grammar in ast-grep-language"),
];

/// Bodies that exercise each format's own syntax, so a covered row proves the
/// grammar parsed something rather than that the file was non-empty.
fn body(extension: &str) -> &'static str {
    match extension {
        "html" => "<html><body><h1>Title</h1><p class=\"note\">text</p></body></html>\n",
        "yaml" => "key: value\nlist:\n  - one\n  - two\n",
        "json" => "{\"name\": \"x\", \"items\": [1, 2]}\n",
        "css" => "body { color: red; }\n.note { margin: 0; }\n",
        "md" => "# Title\n\nSome *text* and [a link](http://example.com).\n",
        "toml" => "[package]\nname = \"x\"\nversion = \"1\"\n",
        "xml" => "<root><child id=\"1\">text</child></root>\n",
        other => panic!("no body for .{other}"),
    }
}

#[test]
fn document_format_coverage_is_what_the_cli_claims() {
    let dir = std::env::temp_dir().join(format!("sprefa-doc-formats-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    for (extension, family, why) in FORMATS {
        let covered = &!family.is_empty();
        let path = dir.join(format!("sample.{extension}"));
        std::fs::write(&path, body(extension)).unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_extract"))
            .arg(&path)
            .output()
            .expect("extract binary runs");
        // An unhandled extension is EXIT 0 WITH NO OUTPUT, never an error. That
        // is the documented contract and it is what lets a caller sweep a mixed
        // tree without filtering by extension first.
        assert!(
            output.status.success(),
            ".{extension} exited {}: an unhandled format must still exit 0",
            output.status
        );
        let facts = String::from_utf8(output.stdout).unwrap();
        let produced = facts.lines().count() > 0;
        assert_eq!(
            produced,
            *covered,
            ".{extension}: expected covered={covered} ({why}), got {} facts. \
             If a grammar was added or dropped, this row moves and the CLI's \
             LANGUAGE COVERAGE table moves with it.",
            facts.lines().count()
        );
        if *covered {
            assert!(
                facts.contains(&format!("\"family\":\"{family}\"")),
                ".{extension} is covered by the {family} family, so its facts must carry it"
            );
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// The CLI's own coverage text must name the formats the test above proves are
/// covered. A caller reads `--help`, not this file.
#[test]
fn the_cli_help_names_the_fallback_formats() {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--help")
        .output()
        .expect("extract binary runs");
    let help = String::from_utf8(output.stdout).unwrap();
    for (extension, family, _) in FORMATS {
        if !family.is_empty() {
            assert!(
                help.contains(extension),
                "--help does not mention .{extension}, which the extractor does handle"
            );
        }
    }
}
