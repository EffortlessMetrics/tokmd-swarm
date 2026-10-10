//! Full-file packing regressions for the policy/packing measurement boundary.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use assert_cmd::Command;
use serde_json::Value;

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

    let receipt: Value = serde_json::from_slice(&run_context(root, strategy, "json")?)?;
    let files = receipt["files"].as_array().context("missing selected files")?;
    ensure!(
        files.len() == 7,
        "expected seven fully charged files, got {}",
        files.len()
    );
    ensure!(
        receipt["used_tokens"].as_u64() == Some(9_800),
        "wrong total charge"
    );
    let mut observed_bytes = 0_u64;
    for file in files {
        ensure!(
            file["policy"].as_str() == Some("full"),
            "unexpected truncation"
        );
        ensure!(
            file["tokens"].as_u64() == Some(1_400),
            "stale full-file charge"
        );
        ensure!(
            file["effective_tokens"].is_null(),
            "full file has a partial charge"
        );
        let path = file["path"].as_str().context("missing selected path")?;
        observed_bytes += fs::metadata(root.join(path))?.len();
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
    ensure!(bundle.len() >= 39_200, "selected content was lost");
    ensure!(
        bundle.len() <= 40_000,
        "fixture bundle exceeds its byte-equivalent budget"
    );
    // This fixture has enough spare room for framing. This is not a general
    // hard output cap: long-line rendering and framing policy are separate work.
    Ok(())
}

#[test]
fn greedy_full_file_budget_uses_policy_estimate() -> Result<()> {
    full_file_pack_uses_policy_estimate("greedy", &"x".repeat(85))
}

#[test]
fn spread_full_file_budget_uses_policy_estimate() -> Result<()> {
    full_file_pack_uses_policy_estimate("spread", &"x".repeat(85))
}

#[test]
fn greedy_multibyte_budget_uses_policy_estimate() -> Result<()> {
    full_file_pack_uses_policy_estimate("greedy", &format!("{}x", "é".repeat(42)))
}

#[test]
fn spread_multibyte_budget_uses_policy_estimate() -> Result<()> {
    full_file_pack_uses_policy_estimate("spread", &format!("{}x", "é".repeat(42)))
}
