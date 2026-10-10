//! Full-file packing regressions for the policy/packing measurement boundary.
//! Test names match the existing `cargo test -p tokmd context_pack` proof filter.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Result, ensure};
use assert_cmd::Command;
use tokmd_types::{ContextReceipt, InclusionPolicy};

fn run_context(root: &Path, strategy: &str, mode: &str) -> Result<Vec<u8>> {
    let output = Command::new(env!("CARGO_BIN_EXE_tokmd"))
        .current_dir(root)
        .args(["context", "--mode", mode, "--strategy", strategy])
        .args(["--budget", "10000", "--max-file-tokens", "1500"])
        .timeout(Duration::from_secs(30))
        .output()?;
    ensure!(
        output.status.success(),
        "context {strategy}/{mode} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
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

    // Full is deliberately omitted on the wire. Deserialize the public DTO
    // so its documented serde default is applied without accepting bad values.
    let receipt: ContextReceipt = serde_json::from_slice(&run_context(root, strategy, "json")?)?;
    ensure!(
        receipt.files.len() == 7 && receipt.file_count == 7,
        "expected seven fully charged files, got {} (reported {})",
        receipt.files.len(),
        receipt.file_count
    );
    ensure!(receipt.used_tokens == 9_800, "wrong total charge");
    let mut observed_bytes = 0_u64;
    for file in &receipt.files {
        ensure!(file.policy == InclusionPolicy::Full, "unexpected truncation");
        ensure!(file.tokens == 1_400, "stale full-file charge");
        ensure!(file.effective_tokens.is_none(), "full file has a partial charge");
        observed_bytes += fs::metadata(root.join(&file.path))?.len();
    }
    ensure!(
        observed_bytes == 39_200,
        "selected payload does not match charges"
    );

    let bundle = String::from_utf8(run_context(root, strategy, "bundle")?)?;
    ensure!(
        bundle.matches("// === ").count() == 7,
        "bundle selection disagrees"
    );
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
    ensure!(
        bundle.len() <= 40_000,
        "fixture bundle exceeds its byte-equivalent budget"
    );
    // This fixture has enough spare room for framing. This is not a general
    // hard output cap: long-line rendering and framing policy are separate work.
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
