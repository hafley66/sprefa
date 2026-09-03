use sprefa_extract::{dispatch, flatten_jsonl, FamilyMask};

const MARKDOWN: &[u8] = br#"# title

paragraph with *emphasis* and [a link](target.md).

- [ ] task

```rust
fn main() {}
```
"#;

#[test]
fn markdown_block_and_inline_grammars_share_one_cst() {
    let output = dispatch("sample.md", MARKDOWN, FamilyMask::ALL).expect("markdown source");
    assert!(output.cst.is_some());
    assert!(output.types.is_none());
    assert!(output.call.is_none());
    assert!(output.df.is_none());

    let facts = flatten_jsonl(&output).join("\n");
    for kind in [
        "atx_heading",
        "paragraph",
        "emphasis",
        "inline_link",
        "list_item",
        "fenced_code_block",
    ] {
        assert!(
            facts.contains(&format!("\"kind\":\"{kind}\"")),
            "missing markdown node kind {kind}:\n{facts}"
        );
    }
}

/// Types-only projects the sample's inline link and fence as `doc_node` rows:
/// the link carries its text and destination, the fence its language and the
/// content span between the fence lines (bytes 81..94 = `fn main() {}\n`).
#[test]
fn markdown_types_only_projects_link_and_fence_rows() {
    let types_only = FamilyMask {
        cst: false,
        types: true,
        call: false,
        df: false,
        data: false,
    };
    let output = dispatch("sample.md", MARKDOWN, types_only).expect("markdown source");
    let facts = flatten_jsonl(&output);
    let link = r#"{"record":"doc_node","family":"type","span":{"start":39,"end":58},"kind":"link","name":"a link","parent":"title","target":"target.md"}"#;
    let fence = r#"{"record":"doc_node","family":"type","span":{"start":73,"end":98},"kind":"code_block","name":"rust","parent":"title","body":{"start":81,"end":94}}"#;
    for expected in [link, fence] {
        assert!(
            facts.iter().any(|row| row == expected),
            "missing doc_node row {expected}:\n{}",
            facts.join("\n")
        );
    }
}
