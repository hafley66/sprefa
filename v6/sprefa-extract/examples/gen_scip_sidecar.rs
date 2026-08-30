//! Stamp `<root>/.dl/.state/index.scip` with the freshness set of the file
//! list on argv, so a plain `--resolve` over the same list adopts it. Machine
//! receipt generator for the corpora, never run by tests.
use sprefa_extract::{content_id_of, record_index_set, IndexSet};

fn main() {
    let mut args = std::env::args().skip(1);
    let root = std::path::PathBuf::from(args.next().expect("project root"));
    let set = IndexSet::new(args.map(|path| {
        let bytes = std::fs::read(&path).expect("read file");
        (path, match content_id_of(&bytes) {
            sprefa_extract::ContentId::Blake3(bytes) => {
                bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
            }
            sprefa_extract::ContentId::GitBlob(oid) => oid.0.to_string(),
        })
    }));
    let index = root.join(".dl").join(".state").join("index.scip");
    record_index_set(&index, &set);
    println!("stamped {} ({} files, digest {})", index.display(), set.len(), set.digest());
}
