use std::collections::HashMap;
use effect_proof::effects::count_lines::{CountLines, SimpleLineCounter};
use effect_proof::effects::read_bytes::{InMemReader, ReadBytes};
use effect_runtime::{EffectKind, RtCtx, RtCtxBuilder};

fn ctx_with_fake_fs() -> RtCtx {
    let mut fs = HashMap::new();
    fs.insert("a.txt".into(), b"hello\nworld\n".to_vec());
    fs.insert("b.rs".into(), b"fn main() {}\nfn other() {}\n".to_vec());

    RtCtxBuilder::new()
        .register::<ReadBytes, _>(InMemReader::new(fs))
        .register::<CountLines, _>(SimpleLineCounter)
        .build()
}

#[tokio::test]
async fn typed_roundtrip_read_bytes() {
    let ctx = ctx_with_fake_fs();

    // Typed response at call site. Zero type erasure visible.
    let bytes: Vec<u8> = ctx.put(ReadBytes { path: "a.txt".into() }).await;

    assert_eq!(bytes, b"hello\nworld\n");
}

#[tokio::test]
async fn two_effects_coexist_in_one_registry() {
    let ctx = ctx_with_fake_fs();

    let bytes = ctx.put(ReadBytes { path: "b.rs".into() }).await;
    let lines: usize = ctx.put(CountLines { content: bytes }).await;

    assert_eq!(lines, 2);
}

#[tokio::test]
async fn author_never_writes_type_erasure() {
    // Structural claim: this file compiles without importing or
    // mentioning the framework-private erasure machinery.
    // Run `plugins.sh audit` to confirm across effects + tests.
    let ctx = ctx_with_fake_fs();
    let _ = ctx.put(ReadBytes { path: "a.txt".into() }).await;
}

#[tokio::test]
#[should_panic(expected = "effect kind not registered")]
async fn unregistered_effect_panics_clearly() {
    struct Unregistered;
    impl EffectKind for Unregistered {
        type Response = ();
    }

    let ctx = RtCtxBuilder::new().build();
    let _ = ctx.put(Unregistered).await;
}
