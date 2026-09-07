//! Regression coverage for run-artifact resolution in the shared diff facade.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use tempfile::TempDir;
use tokmd_core::{diff_workflow, ffi::run_json, lang_workflow};
use tokmd_settings::{DiffSettings, LangSettings, ScanSettings};

const OLD_CODE: usize = 10;
const NEW_CODE: usize = 25;
const EXPECTED_DELTA: i64 = 15;

fn write_source_receipt(source: &Path, run_dir: &Path, body: &str, code: usize) -> Result<()> {
    fs::create_dir_all(source).context("create source fixture")?;
    fs::create_dir_all(run_dir).context("create run fixture")?;
    fs::write(source.join("lib.rs"), body).context("write Rust fixture")?;

    let scan = ScanSettings::for_paths(vec![source.to_string_lossy().into_owned()]);
    let mut receipt = lang_workflow(&scan, &LangSettings::default()).context("scan fixture")?;
    let rust_row = receipt
        .report
        .rows
        .iter_mut()
        .find(|row| row.lang == "Rust")
        .context("fixture scan produced no Rust row")?;
    rust_row.code = code;
    receipt.report.total.code = code;
    let json = serde_json::to_string_pretty(&receipt).context("serialize language receipt")?;
    fs::write(run_dir.join("lang.json"), json).context("write language receipt")?;
    Ok(())
}

fn fixture_runs() -> Result<TempDir> {
    let root = TempDir::new().context("create fixture root")?;
    write_source_receipt(
        &root.path().join("source-old"),
        &root.path().join("run-old"),
        "pub fn value() -> usize { 1 }\n",
        OLD_CODE,
    )?;
    write_source_receipt(
        &root.path().join("source-new"),
        &root.path().join("run-new"),
        "pub fn value() -> usize {\n    let first = 1;\n    let second = 2;\n    first + second\n}\n",
        NEW_CODE,
    )?;
    Ok(root)
}

fn assert_artifact_delta(diff: &tokmd_types::DiffReceipt) -> Result<()> {
    ensure!(diff.totals.delta_code == EXPECTED_DELTA);
    let rust = diff
        .diff_rows
        .iter()
        .find(|row| row.lang == "Rust")
        .context("diff produced no Rust row")?;
    ensure!(rust.old_code == OLD_CODE);
    ensure!(rust.new_code == NEW_CODE);
    ensure!(rust.delta_code == EXPECTED_DELTA);
    Ok(())
}

#[test]
fn run_directories_load_lang_receipts_instead_of_scanning_artifacts() -> Result<()> {
    let root = fixture_runs()?;
    fs::write(
        root.path().join("run-old/receipt.json"),
        r#"{"mode":"run"}"#,
    )
    .context("write old manifest")?;
    fs::write(
        root.path().join("run-new/receipt.json"),
        r#"{"mode":"run"}"#,
    )
    .context("write new manifest")?;
    let settings = DiffSettings {
        from: root.path().join("run-old").to_string_lossy().into_owned(),
        to: root.path().join("run-new").to_string_lossy().into_owned(),
    };

    let diff = diff_workflow(&settings).context("diff run directories")?;
    assert_artifact_delta(&diff)
}

#[test]
fn run_manifests_resolve_to_sibling_lang_receipts() -> Result<()> {
    let root = fixture_runs()?;
    let old_run = root.path().join("run-old");
    let new_run = root.path().join("run-new");
    fs::write(old_run.join("receipt.json"), r#"{"mode":"run"}"#).context("write old manifest")?;
    fs::write(new_run.join("receipt.json"), r#"{"mode":"run"}"#).context("write new manifest")?;
    let settings = DiffSettings {
        from: old_run.join("receipt.json").to_string_lossy().into_owned(),
        to: new_run.join("receipt.json").to_string_lossy().into_owned(),
    };

    let diff = diff_workflow(&settings).context("diff run manifests")?;
    assert_artifact_delta(&diff)
}

#[test]
fn direct_language_receipt_named_receipt_json_remains_supported() -> Result<()> {
    let root = fixture_runs()?;
    let old_direct = root.path().join("direct-old");
    let new_direct = root.path().join("direct-new");
    fs::create_dir_all(&old_direct).context("create old direct receipt directory")?;
    fs::create_dir_all(&new_direct).context("create new direct receipt directory")?;
    let old_path = old_direct.join("receipt.json");
    let new_path = new_direct.join("receipt.json");
    fs::copy(root.path().join("run-old/lang.json"), &old_path)
        .context("copy old direct receipt")?;
    fs::copy(root.path().join("run-new/lang.json"), &new_path)
        .context("copy new direct receipt")?;
    let settings = DiffSettings {
        from: old_path.to_string_lossy().into_owned(),
        to: new_path.to_string_lossy().into_owned(),
    };

    let diff = diff_workflow(&settings).context("diff direct receipts")?;
    assert_artifact_delta(&diff)
}

#[test]
fn source_directory_with_lang_json_but_no_manifest_is_scanned() -> Result<()> {
    let root = TempDir::new().context("create fixture root")?;
    let old = root.path().join("old");
    let new = root.path().join("new");
    fs::create_dir_all(&old).context("create old source directory")?;
    fs::create_dir_all(&new).context("create new source directory")?;
    fs::write(old.join("lib.rs"), "pub fn value() -> usize { 1 }\n")
        .context("write old source fixture")?;
    fs::write(
        new.join("lib.rs"),
        "pub fn value() -> usize {\n    let first = 1;\n    let second = 2;\n    first + second\n}\n",
    )
    .context("write new source fixture")?;
    fs::write(old.join("lang.json"), "not a receipt").context("write old colliding source file")?;
    fs::write(new.join("lang.json"), "not a receipt").context("write new colliding source file")?;
    let settings = DiffSettings {
        from: old.to_string_lossy().into_owned(),
        to: new.to_string_lossy().into_owned(),
    };

    let diff = diff_workflow(&settings).context("diff source directory")?;
    ensure!(diff.totals.delta_code != 0);
    ensure!(diff.diff_rows.iter().any(|row| row.lang == "Rust"));
    Ok(())
}

#[test]
fn missing_receipt_json_does_not_load_an_unrelated_sibling() -> Result<()> {
    let root = fixture_runs()?;
    let missing = root.path().join("missing/receipt.json");
    let sibling_dir = missing.parent().context("missing path has no parent")?;
    fs::create_dir_all(sibling_dir).context("create sibling fixture directory")?;
    fs::copy(
        root.path().join("run-old/lang.json"),
        sibling_dir.join("lang.json"),
    )
    .context("copy unrelated sibling language receipt")?;
    let settings = DiffSettings {
        from: missing.to_string_lossy().into_owned(),
        to: root
            .path()
            .join("run-old/lang.json")
            .to_string_lossy()
            .into_owned(),
    };

    ensure!(diff_workflow(&settings).is_err());
    Ok(())
}

#[test]
fn run_manifest_without_sibling_lang_receipt_fails_with_path_context() -> Result<()> {
    let root = TempDir::new().context("create fixture root")?;
    let run = root.path().join("run");
    fs::create_dir_all(&run).context("create run directory")?;
    fs::write(run.join("receipt.json"), r#"{"mode":"run"}"#).context("write manifest")?;
    let settings = DiffSettings {
        from: run.join("receipt.json").to_string_lossy().into_owned(),
        to: run.join("receipt.json").to_string_lossy().into_owned(),
    };

    let error = diff_workflow(&settings)
        .err()
        .context("missing sibling lang.json unexpectedly succeeded")?;
    let message = format!("{error:#}");
    ensure!(message.contains("Failed to read run language receipt"));
    ensure!(message.contains("lang.json"));
    Ok(())
}

#[test]
fn malformed_sibling_lang_receipt_fails_with_parse_context() -> Result<()> {
    let root = TempDir::new().context("create fixture root")?;
    let run = root.path().join("run");
    fs::create_dir_all(&run).context("create run directory")?;
    fs::write(run.join("receipt.json"), r#"{"mode":"run"}"#).context("write manifest")?;
    fs::write(run.join("lang.json"), "not json").context("write malformed receipt")?;
    let settings = DiffSettings {
        from: run.to_string_lossy().into_owned(),
        to: run.to_string_lossy().into_owned(),
    };

    let error = diff_workflow(&settings)
        .err()
        .context("malformed sibling lang.json unexpectedly succeeded")?;
    let message = format!("{error:#}");
    ensure!(message.contains("Failed to parse run language receipt"));
    ensure!(message.contains("lang.json"));
    Ok(())
}

#[test]
fn malformed_canonical_lang_receipt_fails_without_scan_fallback() -> Result<()> {
    let root = fixture_runs()?;
    let malformed = root.path().join("lang.json");
    fs::write(&malformed, r#"{"not":"a language receipt"}"#)
        .context("write malformed canonical receipt")?;
    let settings = DiffSettings {
        from: malformed.to_string_lossy().into_owned(),
        to: root
            .path()
            .join("run-old/lang.json")
            .to_string_lossy()
            .into_owned(),
    };

    let error = diff_workflow(&settings)
        .err()
        .context("malformed canonical receipt fell back to a source scan")?;
    let message = format!("{error:#}");
    ensure!(message.contains("Failed to parse language receipt"));
    ensure!(message.contains("lang.json"));
    ensure!(message.contains("missing field"));
    Ok(())
}

#[test]
fn generic_json_source_files_remain_eligible_for_source_path_handling() -> Result<()> {
    let root = TempDir::new().context("create fixture root")?;
    let old = root.path().join("config-old.json");
    let new = root.path().join("config-new.json");
    fs::write(&old, "{\"value\":1}\n").context("write old JSON source fixture")?;
    fs::write(&new, "{\n  \"value\": 1,\n  \"other\": 2\n}\n")
        .context("write new JSON source fixture")?;
    let settings = DiffSettings {
        from: old.to_string_lossy().into_owned(),
        to: new.to_string_lossy().into_owned(),
    };

    let diff = diff_workflow(&settings).context("diff generic JSON source files")?;
    let json = diff
        .diff_rows
        .iter()
        .find(|row| row.lang == "JSON")
        .context("diff produced no JSON row")?;
    ensure!(json.delta_code != 0);
    Ok(())
}

#[test]
fn ffi_preserves_json_source_compatibility_and_canonical_receipt_errors() -> Result<()> {
    let root = fixture_runs()?;
    let generic_json = root.path().join("LANG.JSON");
    fs::write(&generic_json, "{\n  \"value\": true\n}\n")
        .context("write generic JSON source fixture")?;
    let success_args = serde_json::json!({
        "from": generic_json,
        "to": generic_json,
    });
    let success: serde_json::Value =
        serde_json::from_str(&run_json("diff", &success_args.to_string()))
            .context("parse successful FFI envelope")?;
    ensure!(success.pointer("/ok") == Some(&serde_json::Value::Bool(true)));

    let malformed = root.path().join("lang.json");
    fs::write(&malformed, r#"{"not":"a language receipt"}"#)
        .context("write malformed canonical receipt")?;
    let error_args = serde_json::json!({
        "from": malformed,
        "to": root.path().join("run-old/lang.json"),
    });
    let error: serde_json::Value = serde_json::from_str(&run_json("diff", &error_args.to_string()))
        .context("parse failed FFI envelope")?;
    ensure!(error.pointer("/ok") == Some(&serde_json::Value::Bool(false)));
    let message = error
        .pointer("/error/message")
        .and_then(serde_json::Value::as_str)
        .context("FFI error has no message")?;
    ensure!(message.contains("lang.json"));
    ensure!(message.contains("missing field"));
    Ok(())
}
