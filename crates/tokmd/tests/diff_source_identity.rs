//! The source named by a diff must supply the measured language rows.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;

fn tokmd() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    command.args(["--no-progress", "--config", "none"]);
    command.timeout(Duration::from_secs(10));
    command
}

fn receipt(code: u64) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema_version": 2, "generated_at_ms": 0,
        "tool": {"name": "tokmd", "version": "test"},
        "mode": "lang", "status": "complete", "warnings": [],
        "scan": {
            "paths": ["fixture-source"], "excluded": [], "config": "none",
            "hidden": false, "no_ignore": false, "no_ignore_parent": false,
            "no_ignore_dot": false, "no_ignore_vcs": false,
            "treat_doc_strings_as_comments": false
        },
        "args": {"format": "json", "top": 0, "with_files": false, "children": "collapse"},
        "rows": [{"lang": "Rust", "code": code, "lines": code, "files": 1,
                  "bytes": code * 10, "tokens": code * 2, "avg_lines": code}],
        "total": {"code": code, "lines": code, "files": 1, "bytes": code * 10,
                  "tokens": code * 2, "avg_lines": code},
        "with_files": false, "children": "collapse", "top": 0
    }))
    .unwrap()
}

fn assert_delta(from: &Path, to: &Path) {
    let output = tokmd()
        .args(["diff", "--format", "json", "--from"])
        .arg(from)
        .arg("--to")
        .arg(to)
        .assert()
        .success()
        .get_output()
        .clone();
    let diff: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(diff["from_source"], from.to_string_lossy().as_ref());
    assert_eq!(diff["to_source"], to.to_string_lossy().as_ref());
    assert_eq!(diff["totals"]["delta_code"], 15);
    let rust = diff["diff_rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["lang"] == "Rust")
        .unwrap();
    assert_eq!(rust["old_code"], 10);
    assert_eq!(rust["new_code"], 25);
    assert_eq!(rust["delta_code"], 15);
}

#[test]
fn direct_receipt_json_without_sibling_uses_selected_file() {
    let root = tempdir().unwrap();
    let from = root.path().join("receipt.json");
    let to = root.path().join("new.json");
    fs::write(&from, receipt(10)).unwrap();
    fs::write(&to, receipt(25)).unwrap();
    assert_delta(&from, &to);
}

#[test]
fn direct_receipt_json_ignores_conflicting_or_malformed_sibling() {
    let root = tempdir().unwrap();
    let from = root.path().join("receipt.json");
    let to = root.path().join("new.json");
    let sibling = root.path().join("lang.json");
    let selected = receipt(10);
    fs::write(&from, &selected).unwrap();
    fs::write(&to, receipt(25)).unwrap();
    for unrelated in [receipt(900), b"not JSON".to_vec()] {
        fs::write(&sibling, &unrelated).unwrap();
        assert_delta(&from, &to);
        assert_eq!(fs::read(&from).unwrap(), selected);
        assert_eq!(fs::read(&sibling).unwrap(), unrelated);
    }
}

#[test]
fn moved_direct_receipt_json_preserves_metrics_and_selected_identity() {
    let root = tempdir().unwrap();
    let from = root.path().join("lang.json");
    let to = root.path().join("new.json");
    fs::write(&from, receipt(10)).unwrap();
    fs::write(&to, receipt(25)).unwrap();
    assert_delta(&from, &to);
    let moved = root.path().join("moved");
    fs::create_dir(&moved).unwrap();
    let moved_from = moved.join("receipt.json");
    fs::rename(from, &moved_from).unwrap();
    assert_delta(&moved_from, &to);
}

#[test]
fn missing_selected_receipt_does_not_use_existing_sibling() {
    let root = tempdir().unwrap();
    let from = root.path().join("receipt.json");
    let to = root.path().join("new.json");
    fs::write(root.path().join("lang.json"), receipt(10)).unwrap();
    fs::write(&to, receipt(25)).unwrap();
    let output = tokmd()
        .args(["diff", "--format", "json", "--from"])
        .arg(&from)
        .arg("--to")
        .arg(&to)
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("receipt.json"));
    assert!(error.contains("path does not exist"));
    // Identical argv recovers once the selected baseline actually exists.
    fs::write(&from, receipt(10)).unwrap();
    assert_delta(&from, &to);
}

#[test]
fn moved_run_manifest_still_uses_sibling_and_missing_sibling_fails() {
    let root = tempdir().unwrap();
    let run = root.path().join("run");
    fs::create_dir(&run).unwrap();
    fs::write(run.join("lang.json"), receipt(10)).unwrap();
    let manifest = br#"{"schema_version":2,"generated_at_ms":0,"lang_file":"lang.json","module_file":"module.json","export_file":"export.jsonl"}"#;
    fs::write(run.join("receipt.json"), manifest).unwrap();
    let to = root.path().join("new.json");
    fs::write(&to, receipt(25)).unwrap();
    assert_delta(&run.join("receipt.json"), &to);
    assert_delta(&run, &to);
    let moved = root.path().join("moved-run");
    fs::rename(run, &moved).unwrap();
    assert_delta(&moved.join("receipt.json"), &to);
    fs::remove_file(moved.join("lang.json")).unwrap();
    let output = tokmd()
        .args(["diff", "--format", "json", "--from"])
        .arg(moved.join("receipt.json"))
        .arg("--to")
        .arg(&to)
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("lang.json"));
    fs::write(moved.join("lang.json"), receipt(10)).unwrap();
    assert_delta(&moved.join("receipt.json"), &to);
    assert_eq!(fs::read(moved.join("receipt.json")).unwrap(), manifest);
}
