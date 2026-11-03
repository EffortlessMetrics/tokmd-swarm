use std::path::PathBuf;

use tokmd_model::normalize_path;

#[test]
fn normalize_path_prefix_partial_match() {
    let path = PathBuf::from("project_extra/file.rs");
    let prefix = PathBuf::from("project");

    assert_eq!(
        normalize_path(&path, Some(&prefix)),
        "project_extra/file.rs"
    );
}

#[test]
fn normalize_path_prefix_exact_match_keeps_path() {
    let path = PathBuf::from("project");
    let prefix = PathBuf::from("project");

    assert_eq!(normalize_path(&path, Some(&prefix)), "project");
}

#[test]
fn normalize_path_prefix_without_separator_strips_child() {
    let path = PathBuf::from("project/src/lib.rs");
    let prefix = PathBuf::from("project");

    assert_eq!(normalize_path(&path, Some(&prefix)), "src/lib.rs");
}

#[test]
fn normalize_path_prefix_dot_backslash() {
    // Counterexample: prefix `.\project` must normalize to `project`
    // before stripping, so the child path is not retained whole.
    let path = PathBuf::from("project/src/lib.rs");
    let prefix = PathBuf::from(r".\project");

    assert_eq!(normalize_path(&path, Some(&prefix)), "src/lib.rs");
}

#[test]
fn normalize_path_repeated_leading_dot_slash_with_prefix() {
    // Counterexample: repeated leading `./` on the path must be removed
    // before stripping, so `project` is not retained.
    let path = PathBuf::from("././project/src/lib.rs");
    let prefix = PathBuf::from("project");

    assert_eq!(normalize_path(&path, Some(&prefix)), "src/lib.rs");
}

#[test]
fn normalize_path_repeated_leading_dot_slash_without_prefix() {
    let path = PathBuf::from("././src/lib.rs");

    assert_eq!(normalize_path(&path, None), "src/lib.rs");
}

#[test]
fn normalize_path_prefix_repeated_dot_segments() {
    let path = PathBuf::from("project/src/lib.rs");
    let prefix = PathBuf::from("././project");

    assert_eq!(normalize_path(&path, Some(&prefix)), "src/lib.rs");
}

#[test]
fn normalize_path_prefix_mixed_slashes() {
    let path = PathBuf::from("my/prefix/dir/file.rs");
    let prefix = PathBuf::from("my\\prefix/");

    assert_eq!(normalize_path(&path, Some(&prefix)), "dir/file.rs");
}
