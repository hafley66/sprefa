//! WHICH DOCUMENT FORMATS THE EXTRACTOR ACTUALLY HANDLES, pinned.
//!
//! CORRECTION TO THE SPELUNK. `plans/2026-07-30-sprefa-extract-spelunk.md`
//! section 5 records "no Markdown, HTML, XML, TOML, or YAML Source" and prices
//! all five at "add or buy a grammar". Measured at HEAD, that is half wrong:
//! HTML and YAML (and JSON and CSS, which the doc does not mention) already
//! produce CST facts through the ast-grep fallback, because ast-grep-language
//! ships those grammars and `AstgrepSource` routes anything with a grammar.
//! Only Markdown, TOML and XML genuinely produce nothing.
//!
//! The gap was in the DOCUMENTATION, not the code: the CLI's language coverage
//! table said "python/c/... (any ast-grep grammar) cst only", which is true and
//! tells a caller nothing about whether their .yaml file works. The table now
//! names them and this test is what keeps the claim honest.
//!
//! Adding or losing a grammar flips a row here, which is the point: a dependency
//! bump that silently drops a format should not be silent.

use std::process::Command;

/// (extension, whether the extractor produces any fact for it today).
///
/// A `true` row is a format the ast-grep fallback covers as CST-only. A `false`
/// row is a real absence and costs a new grammar dependency plus a `Source`, so
/// it is a build-vs-buy decision rather than a cleanup.
const FORMATS: &[(&str, bool, &str)] = &[
    ("html", true, "ast-grep-language ships the html grammar"),
    ("yaml", true, "ast-grep-language ships the yaml grammar"),
    ("json", true, "ast-grep-language ships the json grammar"),
    ("css", true, "ast-grep-language ships the css grammar"),
    ("md", false, "no markdown grammar in ast-grep-language"),
    ("toml", false, "no toml grammar in ast-grep-language"),
    ("xml", false, "no xml grammar in ast-grep-language"),
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

    for (extension, covered, why) in FORMATS {
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
                facts.contains("\"family\":\"cst\""),
                ".{extension} is fallback-covered, so its facts must be cst family"
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
    for (extension, covered, _) in FORMATS {
        if *covered {
            assert!(
                help.contains(extension),
                "--help does not mention .{extension}, which the extractor does handle"
            );
        }
    }
}
