use std::path::{Path, PathBuf};

use crate::plan::PathRewriter;

const JS_EXTENSIONS: &[&str] = &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"];

/// Extension probing order when the import has no extension.
const PROBE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"];
const INDEX_FILES: &[&str] = &["index.ts", "index.tsx", "index.js", "index.jsx"];

pub struct JsPathRewriter;

impl PathRewriter for JsPathRewriter {
    fn extensions(&self) -> &[&str] {
        JS_EXTENSIONS
    }

    fn rewrite_import(
        &self,
        from_file: &str,
        _old_target: &str,
        new_target: &str,
        old_import_str: &str,
    ) -> Option<String> {
        // Only rewrite relative imports. Bare specifiers (react, lodash, etc.)
        // are package references -- a file move doesn't change them.
        if !old_import_str.starts_with('.') {
            return None;
        }

        let from_dir = Path::new(from_file).parent()?;
        let new_target = Path::new(new_target);

        let rel = relative_path(from_dir, new_target);
        let mut import_str = rel.to_string_lossy().to_string();

        // Match the extension convention of the original import.
        let old_has_ext = Path::new(old_import_str)
            .extension()
            .is_some();

        if !old_has_ext {
            // Strip extension if old import didn't have one.
            import_str = strip_importable_ext(&import_str);
            // Strip /index if this is a directory index file.
            import_str = strip_index_suffix(&import_str);
        }

        // Ensure ./ prefix for same-directory or child imports.
        if !import_str.starts_with('.') {
            import_str = format!("./{}", import_str);
        }

        // Normalize path separators to forward slash (for Windows).
        import_str = import_str.replace('\\', "/");

        Some(import_str)
    }
}

/// Compute a relative path from `from_dir` to `to_file`.
/// Returns a PathBuf like `../lib/utils.ts` or `./sibling.ts`.
fn relative_path(from: &Path, to: &Path) -> PathBuf {
    // Find common ancestor.
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();

    let common_len = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let ups = from_components.len() - common_len;
    let mut result = PathBuf::new();

    if ups == 0 {
        result.push(".");
    } else {
        for _ in 0..ups {
            result.push("..");
        }
    }

    for comp in &to_components[common_len..] {
        result.push(comp);
    }

    result
}

fn strip_importable_ext(path: &str) -> String {
    let p = Path::new(path);
    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
        if PROBE_EXTENSIONS.contains(&ext) {
            return p.with_extension("").to_string_lossy().to_string();
        }
    }
    path.to_string()
}

fn strip_index_suffix(path: &str) -> String {
    for idx in INDEX_FILES {
        let stem = Path::new(idx).file_stem().unwrap().to_str().unwrap(); // "index"
        let suffix = format!("/{}", stem);
        if path.ends_with(&suffix) {
            return path[..path.len() - suffix.len()].to_string();
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewrite(from: &str, old_target: &str, new_target: &str, import: &str) -> Option<String> {
        JsPathRewriter.rewrite_import(from, old_target, new_target, import)
    }

    #[test]
    fn same_directory_move() {
        // utils.ts moved from src/ to src/lib/
        let result = rewrite(
            "/repo/src/app.ts",
            "/repo/src/utils.ts",
            "/repo/src/lib/utils.ts",
            "./utils",
        );
        assert_eq!(result.as_deref(), Some("./lib/utils"));
    }

    #[test]
    fn parent_directory_move() {
        // utils.ts moved from src/lib/ to src/
        let result = rewrite(
            "/repo/src/lib/consumer.ts",
            "/repo/src/lib/utils.ts",
            "/repo/src/utils.ts",
            "./utils",
        );
        assert_eq!(result.as_deref(), Some("../utils"));
    }

    #[test]
    fn preserves_extension_when_original_had_one() {
        let result = rewrite(
            "/repo/src/app.ts",
            "/repo/src/utils.ts",
            "/repo/src/lib/utils.ts",
            "./utils.ts",
        );
        assert_eq!(result.as_deref(), Some("./lib/utils.ts"));
    }

    #[test]
    fn strips_index_when_original_was_directory_import() {
        let result = rewrite(
            "/repo/src/app.ts",
            "/repo/src/components/index.ts",
            "/repo/src/ui/index.ts",
            "./components",
        );
        assert_eq!(result.as_deref(), Some("./ui"));
    }

    #[test]
    fn bare_specifier_returns_none() {
        let result = rewrite(
            "/repo/src/app.ts",
            "/repo/node_modules/react/index.js",
            "/repo/node_modules/react/index.js",
            "react",
        );
        assert_eq!(result, None);
    }

    #[test]
    fn deeply_nested_relative_path() {
        let result = rewrite(
            "/repo/src/features/auth/login.ts",
            "/repo/src/utils/http.ts",
            "/repo/src/lib/http.ts",
            "../../utils/http",
        );
        assert_eq!(result.as_deref(), Some("../../lib/http"));
    }
}
