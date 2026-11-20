//! Actual consumer-path byte bounds, separate from selection estimates.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use assert_cmd::Command;
use serde_json::Value;

fn invoke(root: &Path, out: &Path, command: &str, compress: bool) -> Result<std::process::Output> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    cmd.current_dir(root)
        .args([command, "--budget", "2000", "--max-file-tokens", "200"])
        .arg("--force")
        .timeout(Duration::from_secs(30));
    if command == "context" {
        cmd.args(["--max-output-bytes", "32"]);
        cmd.args(["--mode", "bundle"]).arg("--bundle-dir").arg(out);
    } else {
        cmd.args(["--preset", "minimal"]).arg("--out-dir").arg(out);
    }
    if compress {
        cmd.arg("--compress");
    }
    Ok(cmd.output()?)
}

fn verify(out: &Path, command: &str) -> Result<Vec<u8>> {
    let name = if command == "context" {
        "bundle.txt"
    } else {
        "code.txt"
    };
    let bytes = fs::read(out.join(name))?;
    let manifest: Value = serde_json::from_slice(&fs::read(out.join("manifest.json"))?)?;
    let files = manifest
        .get("included_files")
        .and_then(Value::as_array)
        .context("missing inventory")?;
    ensure!(files.len() == 1, "wrong selected count");
    let file = files.first().context("empty inventory")?;
    ensure!(
        file.get("policy").and_then(Value::as_str) == Some("head_tail"),
        "fixture is not HeadTail"
    );
    ensure!(
        file.get("effective_tokens").and_then(Value::as_u64) == Some(200),
        "wrong charge"
    );
    // The header is exactly 20 bytes for long.rs, followed by one final blank
    // separator. The body's bounded marker/fragment-newline allowance is 64.
    ensure!(
        bytes.len() <= 800 + 20 + 1 + 64,
        "{} emitted {} bytes for an 800-byte retained-source allowance",
        command,
        bytes.len()
    );
    ensure!(
        std::str::from_utf8(&bytes)?.contains("omitted"),
        "missing explicit omission"
    );
    let artifacts = manifest
        .get("artifacts")
        .and_then(Value::as_array)
        .context("missing artifacts")?;
    let entry = artifacts
        .iter()
        .find(|a| a.get("path").and_then(Value::as_str) == Some(name))
        .context("missing payload artifact")?;
    ensure!(
        entry.get("bytes").and_then(Value::as_u64) == Some(bytes.len() as u64),
        "artifact byte count disagrees"
    );
    ensure!(
        entry.pointer("/hash/hash").and_then(Value::as_str)
            == Some(blake3::hash(&bytes).to_hex().as_str()),
        "artifact hash disagrees"
    );
    if command == "context" {
        ensure!(
            manifest.get("bundle_bytes").and_then(Value::as_u64) == Some(bytes.len() as u64),
            "bundle count disagrees"
        );
        let receipt: Value = serde_json::from_slice(&fs::read(out.join("receipt.json"))?)?;
        ensure!(
            receipt.get("files") == manifest.get("included_files"),
            "receipt inventory disagrees"
        );
        ensure!(
            receipt
                .pointer("/bundle_audit/output_bytes")
                .and_then(Value::as_u64)
                == Some(bytes.len() as u64),
            "receipt audit disagrees"
        );
    }
    Ok(bytes)
}

#[test]
fn context_pack_cli_giant_head_tail_bound_and_repair() -> Result<()> {
    for command in ["context", "handoff"] {
        for last in [false, true] {
            for compress in [false, true] {
                let root = tempfile::tempdir()?;
                let output = tempfile::tempdir()?;
                fs::create_dir(root.path().join(".git"))?;
                let giant = "//".to_string() + &"x".repeat(9_997) + "\n";
                let ordinary = "//".to_string() + &"y".repeat(37) + "\n";
                let content = if last {
                    ordinary.repeat(199) + &giant
                } else {
                    giant + &ordinary.repeat(199)
                };
                let path = root.path().join("long.rs");
                fs::write(&path, &content)?;
                let result = invoke(root.path(), output.path(), command, compress)?;
                ensure!(
                    result.status.success(),
                    "{} failed: {}",
                    command,
                    String::from_utf8_lossy(&result.stderr)
                );
                if command == "context" {
                    ensure!(
                        String::from_utf8_lossy(&result.stderr)
                            .contains("exceeds threshold (32 bytes)"),
                        "advisory output warning was lost or relabeled as a hard cap"
                    );
                }
                let stable = verify(output.path(), command)?;
                // Invalid UTF-8 in the omitted middle must still fail. #693's
                // retained partial-payload behavior remains distinct from the
                // completed-manifest contract checked here.
                let mut invalid = content.as_bytes().to_vec();
                *invalid.get_mut(9_000).context("short fixture")? = 0xff;
                fs::write(&path, invalid)?;
                let result = invoke(root.path(), output.path(), command, compress)?;
                ensure!(
                    !result.status.success(),
                    "omitted invalid UTF-8 silently succeeded"
                );
                ensure!(
                    !output.path().join("manifest.json").exists(),
                    "failure left completion manifest"
                );
                for _ in 0..2 {
                    fs::write(&path, &content)?;
                    let result = invoke(root.path(), output.path(), command, compress)?;
                    ensure!(result.status.success(), "repair failed");
                    ensure!(
                        verify(output.path(), command)? == stable,
                        "repair changed stable bytes"
                    );
                }
            }
        }
    }
    Ok(())
}
