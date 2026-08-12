//! Diff workflow facade.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokmd_types::{DiffReceipt, LangReceipt, LangReport};

use crate::settings::{DiffSettings, LangSettings, ScanSettings};

use super::lang::lang_workflow;

/// Runs the diff workflow comparing two receipts or paths.
///
/// # Arguments
///
/// * `settings` - Diff settings (from, to references)
///
/// # Returns
///
/// A `DiffReceipt` showing changes between the two states.
///
/// # Example
///
/// ```rust
/// use tokmd_core::{diff_workflow, settings::DiffSettings};
///
/// let settings = DiffSettings {
///     from: ".".to_string(), // compare current dir to itself as a quick test
///     to: ".".to_string(),
///     ..Default::default()
/// };
///
/// let receipt = diff_workflow(&settings).expect("Diff failed");
/// assert!(receipt.totals.delta_code == 0); // delta is zero
/// ```
pub fn diff_workflow(settings: &DiffSettings) -> Result<DiffReceipt> {
    let from_report = load_lang_report(&settings.from)?;
    let to_report = load_lang_report(&settings.to)?;

    let rows = tokmd_format::compute_diff_rows(&from_report, &to_report);
    let totals = tokmd_format::compute_diff_totals(&rows);

    Ok(tokmd_format::create_diff_receipt(
        &settings.from,
        &settings.to,
        rows,
        totals,
    ))
}

/// Load a language report from a receipt file path or scan a directory.
fn load_lang_report(source: &str) -> Result<LangReport> {
    let path = Path::new(source);

    if path.exists() && path.is_file() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        if let Ok(receipt) = serde_json::from_str::<LangReceipt>(&content) {
            return Ok(receipt.report);
        }
    }

    if let Some(lang_path) = run_artifact_lang_path(path) {
        return load_lang_receipt(&lang_path);
    }

    let scan = ScanSettings::for_paths(vec![source.to_string()]);
    let lang = LangSettings::default();
    let receipt = lang_workflow(&scan, &lang)?;
    Ok(receipt.report)
}

/// Resolve repo-owned run artifacts without changing the existing ability to
/// diff arbitrary source directories and files through the binding facade.
fn run_artifact_lang_path(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        let lang_path = path.join("lang.json");
        if path.join("receipt.json").is_file() {
            return Some(lang_path);
        }
        return None;
    }

    (path.is_file() && path.file_name()? == "receipt.json")
        .then(|| path.parent().unwrap_or(path).join("lang.json"))
}

fn load_lang_receipt(path: &Path) -> Result<LangReport> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read run language receipt {}", path.display()))?;
    let receipt: LangReceipt = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse run language receipt {}", path.display()))?;
    Ok(receipt.report)
}
