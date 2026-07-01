//! CLI `tokmd module` single-root strip parity with `module_workflow`.
//!
//! `module_workflow` strips a lone scan root before module-key aggregation so
//! host paths align with archive/virtual relative paths. The CLI module command
//! must apply the same rule.

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;
use tokmd_core::{
    module_workflow,
    settings::{ModuleSettings, ScanOptions, ScanSettings},
};
use tokmd_types::ConfigMode;

fn fixture_dir() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).expect("mkdir src");
    std::fs::create_dir_all(root.join("docs")).expect("mkdir docs");
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n").expect("write lib.rs");
    std::fs::write(root.join("docs/README.md"), "# Title\n\n").expect("write README");
    dir
}

#[test]
fn cli_module_single_root_strip_matches_module_workflow() {
    let dir = fixture_dir();
    let root = dir.path().to_string_lossy().into_owned();

    let output = Command::new(env!("CARGO_BIN_EXE_tokmd"))
        .args([
            "--config",
            "none",
            "--no-ignore",
            "module",
            "--format",
            "json",
        ])
        .arg(&root)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cli: Value = serde_json::from_slice(&output).expect("valid module JSON");

    let scan = ScanSettings {
        paths: vec![root],
        options: ScanOptions {
            config: ConfigMode::None,
            no_ignore: true,
            ..Default::default()
        },
    };
    let workflow = module_workflow(&scan, &ModuleSettings::default()).expect("module_workflow");

    assert_eq!(
        cli["rows"],
        serde_json::to_value(&workflow.report.rows).expect("serialize rows")
    );
    assert_eq!(
        cli["total"],
        serde_json::to_value(&workflow.report.total).expect("serialize total")
    );
    assert!(
        cli["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .any(|row| row["module"] == "src"),
        "single-root strip should yield archive-relative module keys like \"src\""
    );
}
