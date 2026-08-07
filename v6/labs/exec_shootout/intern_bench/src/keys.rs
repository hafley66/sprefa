// Node ids become the (path, name) pair a real v6 module keys on: SYMBOLS_PER_FILE
// nodes share one path and FILES_PER_DIR paths share one directory prefix.
pub const SYMBOLS_PER_FILE: u32 = 50;
pub const FILES_PER_DIR: u32 = 40;

pub const PATH_PREFIX: &str = "src/engine/lower";
pub const NAME_PREFIX: &str = "resolveBindingStep";

pub fn node_path(node: u32) -> String {
    let file = node / SYMBOLS_PER_FILE;
    format!(
        "{PATH_PREFIX}/pass_{}/module_{}.ts",
        file / FILES_PER_DIR,
        file
    )
}

pub fn node_name(node: u32) -> String {
    format!("{NAME_PREFIX}_{}", node % SYMBOLS_PER_FILE)
}

// The checksum gate compares against the int-keyed run, so the two columns have
// to hand back the node id they were minted from.
pub fn node_from_columns(path: &str, name: &str) -> Option<u32> {
    let file: u32 = path
        .rsplit_once("/module_")?
        .1
        .strip_suffix(".ts")?
        .parse()
        .ok()?;
    let slot: u32 = name.rsplit_once('_')?.1.parse().ok()?;
    if slot >= SYMBOLS_PER_FILE {
        return None;
    }
    Some(file * SYMBOLS_PER_FILE + slot)
}

pub const TEXT_HEADER_TAG: &str = "text4";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_round_trip_to_the_node_id() {
        for node in [0u32, 1, 49, 50, 51, 1999, 2000, 123_456] {
            let path = node_path(node);
            let name = node_name(node);
            assert_eq!(node_from_columns(&path, &name), Some(node));
        }
    }

    #[test]
    fn paths_and_names_repeat_the_way_a_repo_does() {
        assert_eq!(node_path(0), node_path(49));
        assert_ne!(node_path(0), node_path(50));
        assert_eq!(node_name(0), node_name(50));
        assert_ne!(node_name(0), node_name(1));
    }

    #[test]
    fn the_path_column_carries_a_long_shared_prefix() {
        let left = node_path(0);
        let right = node_path(50);
        let shared = left
            .bytes()
            .zip(right.bytes())
            .take_while(|(one, two)| one == two)
            .count();
        assert!(shared >= 24, "shared prefix was only {shared} bytes");
    }
}
