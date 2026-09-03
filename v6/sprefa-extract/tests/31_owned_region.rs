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
    let _ = std::fs::remove_dir_all(target);
}
