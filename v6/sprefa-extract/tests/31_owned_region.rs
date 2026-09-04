use std::sync::Arc;

use sprefa_extract::{find_owned_region, propose_owned_region};

#[test]
fn generated_dl7_region_checks_stages_and_preserves_every_outside_byte() {
    let before = b"(: Authored (* (: id u64)))\n\n; sprefa:auto-begin rust-types\n(: Old (*))\n; sprefa:auto-end rust-types\n\n(: MoreAuthored (* (: name string)))\n";
    let generated =
        "(: Shape (+ (: Circle f64) (: Square unknown)))\n\n(: User (* (: id T) (: name Option)))";
    let proposal = propose_owned_region(before, "rust-types", generated).unwrap();
    assert!(proposal.changed());
    assert_eq!(
        &before[..proposal.region.start as usize],
        b"(: Authored (* (: id u64)))\n\n; sprefa:auto-begin rust-types\n"
    );
    assert_eq!(
        &before[proposal.region.end as usize..],
        b"; sprefa:auto-end rust-types\n\n(: MoreAuthored (* (: name string)))\n"
    );

    let target = std::env::temp_dir().join(format!(
        "sprefa_owned_region_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("generated.dl7"), before).unwrap();
    let mut root = soopy::SourceRoot::open_directory(&target).unwrap();
    let directory = root.directory().identity.clone();
    let root_id = soopy::SourceRootId::Directory {
        directory: directory.clone(),
    };
    let source = soopy::ActionSource::Directory {
        file: soopy::FileRef {
            directory,
            path: soopy::RootPath(Arc::from("generated.dl7")),
        },
    };
    let request = proposal.stage_request(
        root_id,
        source,
        soopy::ActionProducer::unordered("dl7-type-generator"),
    );
    let plan = soopy::plan_mutations(&mut root, &request).unwrap();
    let after = plan.files[0].bytes_after.as_ref().unwrap();
    assert_eq!(
        String::from_utf8(after.clone()).unwrap(),
        "(: Authored (* (: id u64)))\n\n; sprefa:auto-begin rust-types\n(: Shape (+ (: Circle f64) (: Square unknown)))\n\n(: User (* (: id T) (: name Option)))\n; sprefa:auto-end rust-types\n\n(: MoreAuthored (* (: name string)))\n"
    );

    let unchanged = propose_owned_region(after, "rust-types", generated).unwrap();
    assert!(!unchanged.changed());
    assert!(find_owned_region(after, "other").is_err());

    std::fs::write(target.join("generated.dl7"), b"changed after analysis\n").unwrap();
    let refusal = soopy::plan_mutations(&mut root, &request).unwrap_err();
    assert!(matches!(refusal, soopy::StageRefusal::Stale { .. }));

    #[cfg(feature = "cli")]
    {
        std::fs::write(target.join("generated.dl7"), before).unwrap();
        let generated_path = target.join("body.dl7");
        std::fs::write(&generated_path, generated).unwrap();
        let state = std::env::temp_dir().join(format!(
            "sprefa_owned_region_state_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&state).unwrap();
        let command = |apply: bool| {
            let mut run = std::process::Command::new(env!("CARGO_BIN_EXE_extract"));
            run.args([
                "region",
                target.join("generated.dl7").to_str().unwrap(),
                "rust-types",
                "--generated",
                generated_path.to_str().unwrap(),
                "--state",
                state.to_str().unwrap(),
            ]);
            if apply {
                run.arg("--apply");
            }
            run.output().unwrap()
        };
        let drift = command(false);
        assert_eq!(drift.status.code(), Some(1));
        assert!(String::from_utf8(drift.stdout)
            .unwrap()
            .contains(r#""status":"drift""#));
        let applied = command(true);
        assert!(applied.status.success());
        assert!(String::from_utf8(applied.stdout)
            .unwrap()
            .contains(r#""status":"applied""#));
        let current = command(false);
        assert!(current.status.success());
        assert!(String::from_utf8(current.stdout)
            .unwrap()
            .contains(r#""status":"current""#));
        let _ = std::fs::remove_dir_all(state);
    }

    let _ = std::fs::remove_dir_all(target);
}

#[test]
fn rust_comment_markers_select_the_same_owned_region_protocol() {
    let before = b"pub const BEFORE: u8 = 1;\n// sprefa:auto-begin wire\nold\n// sprefa:auto-end wire\npub const AFTER: u8 = 2;\n";
    let proposal = propose_owned_region(before, "wire", "generated").unwrap();
    assert_eq!(proposal.region.current, "old\n");
    assert_eq!(proposal.replacement, "generated\n");
    assert_eq!(
        &before[..proposal.region.start as usize],
        b"pub const BEFORE: u8 = 1;\n// sprefa:auto-begin wire\n"
    );
    assert_eq!(
        &before[proposal.region.end as usize..],
        b"// sprefa:auto-end wire\npub const AFTER: u8 = 2;\n"
    );
}
