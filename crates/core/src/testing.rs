//! Test utilities shared across the workspace.
//!
//! Available to all crates via `project_core::testing`. Not gated behind
//! `#[cfg(test)]` so integration tests and downstream dev-dependencies
//! can use it.

use std::path::{Path, PathBuf};

/// Returns the path to a fixture file relative to the calling crate's
/// `tests/fixtures/` directory, or `None` if the file does not exist.
///
/// Tests should early-return when `None` is returned:
///
/// ```ignore
/// let path = fixture_or_skip!("some_input.bin");
/// // ... test continues only if fixture exists
/// ```
///
/// The skipped test still reports as passing. Do not `#[ignore]`
/// and do not panic.
pub fn fixture_or_skip(manifest_dir: &str, name: &str) -> Option<PathBuf> {
    let path = Path::new(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join(name);
    if path.exists() {
        Some(path)
    } else {
        eprintln!("SKIP: fixture {name} not present at {}", path.display());
        None
    }
}

/// Convenience macro that calls [`fixture_or_skip`] with the current
/// crate's `CARGO_MANIFEST_DIR` and returns from the test if the
/// fixture is absent.
///
/// ```ignore
/// #[test]
/// fn test_parse_real_data() {
///     let path = fixture_or_skip!("sample.bin");
///     // ... test continues only if fixture exists
/// }
/// ```
#[macro_export]
macro_rules! fixture_or_skip {
    ($name:expr) => {
        match $crate::testing::fixture_or_skip(env!("CARGO_MANIFEST_DIR"), $name) {
            Some(path) => path,
            None => return,
        }
    };
}
