//! Full-file packing regressions for the policy/packing measurement boundary.
//! Test names match the existing `cargo test -p tokmd context_pack` proof filter.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use assert_cmd::Command;
use tokmd_types::{
    ContextBundleManifest, ContextReceipt, InclusionPolicy, TokenAudit, TokenEstimationMeta,
};

fn run_context(
    root: &Path,
    strategy: &str,
    mode: &str,
    bundle_dir: Option<&Path>,
) -> Result<Vec<u8>> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    command
        .current_dir(root)
        .args(["context", "--mode", mode, "--strategy", strategy])
        .args(["--budget", "10000", "--max-file-tokens", "1500"])
        .timeout(Duration::from_secs(30));
    if let Some(directory) = bundle_dir {
        command.arg("--bundle-dir").arg(directory);
    }
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "context {strategy}/{mode} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn check_estimate(estimate: Option<&TokenEstimationMeta>) -> Result<()> {
    let estimate = estimate.context("missing token estimation")?;
    ensure!(estimate.source_bytes == 39_200, "stale receipt source bytes");
    ensure!(estimate.tokens_est == 9_800, "stale receipt token estimate");
    Ok(())
}

fn check_audit(audit: Option<&TokenAudit>, output_bytes: usize) -> Result<()> {
    let audit = audit.context("missing bundle audit")?;
    let overhead = output_bytes
        .checked_sub(39_200)
        .context("bundle lost source content")?;
    ensure!(audit.output_bytes == output_bytes as u64, "wrong output bytes");
    ensure!(audit.overhead_bytes == overhead as u64, "wrong framing bytes");
    Ok(())
}

fn full_file_pack_uses_policy_estimate(strategy: &str, padding: &str) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    fs::create_dir(root.join(".git"))?;
    let line = format!("pub fn f() {{ let _ = \"{padding}\"; }}\n");
    ensure!(line.len() == 112, "fixture line must be 112 bytes");
    let source = line.repeat(50);
    ensure!(source.len() == 5_600, "fixture must be 5,600 bytes");
    for index in 0..20 {
        fs::write(root.join(format!("input_{index:02}.rs")), &source)?;
    }

    // Full is deliberately omitted on the wire. The public DTO applies the
    // documented serde default without accepting unknown or malformed policies.
    let json = run_context(root, strategy, "json", None)?;
    let receipt: ContextReceipt = serde_json::from_slice(&json)?;
    ensure!(
        receipt.files.len() == 7 && receipt.file_count == 7,
        "expected seven fully charged files, got {} (reported {})",
        receipt.files.len(),
        receipt.file_count
    );
    ensure!(receipt.used_tokens == 9_800, "wrong total charge");
    check_estimate(receipt.token_estimation.as_ref())?;
    let mut observed_bytes = 0_u64;
    for file in &receipt.files {
        ensure!(file.policy == InclusionPolicy::Full, "unexpected truncation");
        ensure!(file.tokens == 1_400, "stale full-file charge");
        ensure!(file.bytes == 5_600, "stale selected-file bytes");
        ensure!(file.effective_tokens.is_none(), "unexpected partial charge");
        observed_bytes += fs::metadata(root.join(&file.path))?.len();
    }
    ensure!(observed_bytes == 39_200, "selected payload disagrees with charge");

    let bundle = String::from_utf8(run_context(root, strategy, "bundle", None)?)?;
    ensure!(bundle.matches("// === ").count() == 7, "bundle selection disagrees");
    for file in &receipt.files {
        ensure!(
            bundle.contains(&format!("// === {} ===\n", file.path)),
            "selected file missing from bundle: {}",
            file.path
        );
    }
    ensure!(
        bundle.matches(source.as_str()).count() == 7,
        "bundle did not preserve all seven full source payloads"
    );
    ensure!(bundle.len() <= 40_000, "fixture exceeds its byte-equivalent budget");
    // This fixture has framing headroom. It is not a general output-cap proof.

    let output = tempfile::tempdir()?;
    run_context(root, strategy, "bundle", Some(output.path()))?;
    let saved_bundle = fs::read(output.path().join("bundle.txt"))?;
    ensure!(saved_bundle == bundle.as_bytes(), "directory bundle differs");
    let saved: ContextReceipt =
        serde_json::from_slice(&fs::read(output.path().join("receipt.json"))?)?;
    let manifest: ContextBundleManifest =
        serde_json::from_slice(&fs::read(output.path().join("manifest.json"))?)?;
    for rows in [&saved.files, &manifest.included_files] {
        ensure!(rows.len() == 7, "saved inventory count differs");
        for (actual, expected) in rows.iter().zip(&receipt.files) {
            ensure!(actual.path == expected.path, "saved selection differs");
            ensure!(actual.bytes == 5_600, "saved source bytes are stale");
            ensure!(actual.tokens == 1_400, "saved charge is stale");
            ensure!(actual.policy == InclusionPolicy::Full, "saved policy differs");
        }
    }
    ensure!(saved.file_count == 7 && manifest.file_count == 7, "wrong saved count");
    ensure!(
        saved.used_tokens == 9_800 && manifest.used_tokens == 9_800,
        "wrong saved total charge"
    );
    ensure!(manifest.bundle_bytes == saved_bundle.len(), "wrong manifest bytes");
    check_estimate(saved.token_estimation.as_ref())?;
    check_estimate(manifest.token_estimation.as_ref())?;
    check_audit(saved.bundle_audit.as_ref(), saved_bundle.len())?;
    check_audit(manifest.bundle_audit.as_ref(), saved_bundle.len())?;
    Ok(())
}

#[test]
fn context_pack_greedy_full_file_budget_uses_policy_estimate() -> Result<()> {
    full_file_pack_uses_policy_estimate("greedy", &"x".repeat(85))
}

#[test]
fn context_pack_spread_full_file_budget_uses_policy_estimate() -> Result<()> {
    full_file_pack_uses_policy_estimate("spread", &"x".repeat(85))
}

#[test]
fn context_pack_greedy_multibyte_budget_uses_policy_estimate() -> Result<()> {
    full_file_pack_uses_policy_estimate("greedy", &format!("{}x", "é".repeat(42)))
}

#[test]
fn context_pack_spread_multibyte_budget_uses_policy_estimate() -> Result<()> {
    full_file_pack_uses_policy_estimate("spread", &format!("{}x", "é".repeat(42)))
}
