//! W101 - Release validation source-closure guards.
//!
//! These tests protect files that hosted Nix validation needs inside filtered
//! check sources. They are intentionally text-level because `flake.nix` owns the
//! actual filter and hosted Nix remains the authoritative sandbox proof.

use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn flake_nix() -> String {
    std::fs::read_to_string(workspace_root().join("flake.nix")).expect("flake.nix must exist")
}

#[test]
fn nix_check_source_keeps_top_level_fixtures() {
    let flake = flake_nix();

    assert!(
        flake.contains(r#"pkgs.lib.hasInfix "/fixtures/" p"#),
        "mkCheckSrc must keep top-level fixtures used by compile-time include_str! tests"
    );
}

#[test]
fn syntax_native_boundary_fixture_exists_for_nix_checks() {
    let fixture = workspace_root()
        .join("fixtures")
        .join("syntax")
        .join("typescript")
        .join("native_boundary.ts");

    assert!(
        fixture.is_file(),
        "{} must exist for cli_syntax_integration include_str! and Nix checks",
        fixture.display()
    );
}
