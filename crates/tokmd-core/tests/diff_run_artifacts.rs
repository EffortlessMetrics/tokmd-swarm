//! Regression coverage for run-artifact resolution in the shared diff facade.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use tempfile::TempDir;
use tokmd_core::{diff_workflow, lang_workflow};
use tokmd_settings::{DiffSettings, LangSettings, ScanSettings};
use tokmd_types::LangReceipt;

fn write_source_receipt(source: &Path, run_dir: &Path, body: &str) -> Result<LangReceipt> {
    fs::create_dir_all(source).context("create source fixture")?;
    fs::create_dir_all(run_dir).context("create run fixture")?;
    fs::write(source.join("lib.rs"), body).context("write Rust fixture")?;

    let scan = ScanSettings::for_paths(vec![source.to_string_lossy().into_owned()]);
    let receipt = lang_workflow(&scan, &LangSettings::default()).context("scan fixture")?;
    let json = serde_json::to_string_pretty(&receipt).context("serialize language receipt")?;
    fs::write(run_dir.join("lang.json"), json).context("write language receipt")?;
    Ok(receipt)
}

fn fixture_runs() -> Result<(TempDir, LangReceipt, LangReceipt)> {
    let root = TempDir::new().context("create fixture root")?;
    let old = write_source_receipt(
        &root.path().join("source-old"),
        &root.path().join("run-old"),
        "pub fn value() -> usize { 1 }\n",
    )?;
    let new = write_source_receipt(
        &root.path().join("source-new"),
        &root.path().join("run-new"),
        "pub fn value() -> usize {\n    let first = 1;\n    let second = 2;\n    first + second\n}\n",
    )?;
    ensure!(
        old.report.total.code != new.report.total.code,
        "fixture language totals must differ"
    );
    Ok((root, old, new))
}

fn expected_delta(old: &LangReceipt, new: &LangReceipt) -> Result<i64> {
    let old_code = i64::try_from(old.report.total.code).context("old code total fits i64")?;
    let new_code = i64::try_from(new.report.total.code).context("new code total fits i64")?;
    Ok(new_code - old_code)
}

#[test]
fn run_directories_load_lang_receipts_instead_of_scanning_artifacts() -> Result<()> {
    let (root, old, new) = fixture_runs()?;
    let settings = DiffSettings {
        from: root.path().join("run-old").to_string_lossy().into_owned(),
        to: root.path().join("run-new").to_string_lossy().into_owned(),
    };

    let diff = diff_workflow(&settings).context("diff run directories")?;
    ensure!(diff.totals.delta_code == expected_delta(&old, &new)?);
    ensure!(
        diff.totals.delta_code != 0,
        "regression must not false-succeed"
    );
    Ok(())
}

#[test]
fn run_manifests_resolve_to_sibling_lang_receipts() -> Result<()> {
    let (root, old, new) = fixture_runs()?;
    let old_run = root.path().join("run-old");
    let new_run = root.path().join("run-new");
    fs::write(old_run.join("receipt.json"), r#"{"mode":"run"}"#).context("write old manifest")?;
    fs::write(new_run.join("receipt.json"), r#"{"mode":"run"}"#).context("write new manifest")?;
    let settings = DiffSettings {
        from: old_run.join("receipt.json").to_string_lossy().into_owned(),
        to: new_run.join("receipt.json").to_string_lossy().into_owned(),
    };

    let diff = diff_workflow(&settings).context("diff run manifests")?;
    ensure!(diff.totals.delta_code == expected_delta(&old, &new)?);
    ensure!(
        diff.totals.delta_code != 0,
        "regression must not false-succeed"
    );
    Ok(())
}

#[test]
fn direct_language_receipt_named_receipt_json_remains_supported() -> Result<()> {
    let (root, old, new) = fixture_runs()?;
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
    ensure!(diff.totals.delta_code == expected_delta(&old, &new)?);
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
