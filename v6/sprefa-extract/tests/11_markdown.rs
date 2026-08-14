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
