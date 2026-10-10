//! Actual CLI exit/completion and stable path controls for required bundle reads.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Result, ensure};
use assert_cmd::Command;
use serde_json::Value;

fn run(
    root: &Path,
    command: &str,
    nested: bool,
    absolute: bool,
    cap: &str,
    compress: bool,
) -> Result<std::process::Output> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    cmd.current_dir(if nested {
        root.join("src")
    } else {
        root.to_path_buf()
    })
    .timeout(Duration::from_secs(30))
    .args([
        "--no-progress",
        command,
        if absolute {
            root.to_str()
                .ok_or_else(|| anyhow::anyhow!("fixture path is not UTF-8"))?
        } else if nested {
            ".."
        } else {
            "."
        },
        "--budget",
        "1000",
        "--max-file-pct",
        "1.0",
        "--max-file-tokens",
        cap,
        "--no-git",
        "--force",
        "--output-dir",
    ])
    .arg(
        root.parent()
            .ok_or_else(|| anyhow::anyhow!("missing fixture parent"))?
            .join("out"),
    );
    if compress {
        cmd.arg("--compress");
    }
    Ok(cmd.output()?)
}

#[test]
fn context_pack_cli_required_reads_preserve_failure_exit_and_completion() -> Result<()> {
    for command in ["context", "handoff"] {
        for nested in [false, true] {
            for absolute in [false, true] {
                for cap in ["1000", "4"] {
                    for compress in [false, true] {
                        let temp = tempfile::tempdir()?;
                        let root_buf = temp.path().join("repo");
                        let root = root_buf.as_path();
                        let out = temp.path().join("out");
                        fs::create_dir_all(root.join("src"))?;
                        let input = root.join("src/input.rs");
                        let valid = "fn f() {}\n\n".repeat(20);
                        fs::write(&input, &valid)?;
                        let stable = run(root, command, nested, absolute, cap, compress)?;
                        ensure!(
                            stable.status.success(),
                            "stable {command} failed: {}",
                            String::from_utf8_lossy(&stable.stderr)
                        );
                        let payload_path = out.clone().join(if command == "context" {
                            "bundle.txt"
                        } else {
                            "code.txt"
                        });
                        let stable_bytes = fs::read(&payload_path)?;
                        let manifest: Value =
                            serde_json::from_slice(&fs::read(out.join("manifest.json"))?)?;
                        let included = manifest
                            .get("included_files")
                            .and_then(Value::as_array)
                            .ok_or_else(|| anyhow::anyhow!("missing inventory"))?;
                        ensure!(included.len() == 1, "unexpected inventory count");
                        let path = included
                            .first()
                            .and_then(|row| row.get("path"))
                            .and_then(Value::as_str)
                            .ok_or_else(|| anyhow::anyhow!("missing selected path"))?;
                        ensure!(
                            !path.contains('\\') && !path.starts_with('/'),
                            "path normalization changed: {path}"
                        );
                        ensure!(
                            String::from_utf8_lossy(&stable_bytes)
                                .contains(&format!("// === {path} ===\n")),
                            "inventory/header mismatch"
                        );
                        // Invalid UTF-8 is a deterministic read fault in text policies;
                        // tokei still selects the source. No permissions or race timing.
                        let mut invalid = valid.as_bytes().to_vec();
                        invalid.push(0xff);
                        fs::write(&input, &invalid)?;
                        let failed = run(root, command, nested, absolute, cap, compress)?;
                        if command == "context" && cap == "1000" && !compress {
                            ensure!(
                                failed.status.success(),
                                "raw Full context lost its byte-stream contract: {}",
                                String::from_utf8_lossy(&failed.stderr)
                            );
                            ensure!(
                                fs::read(&payload_path)?.contains(&0xff),
                                "raw Full context dropped source byte"
                            );
                        } else {
                            ensure!(
                                failed.status.code() == Some(1),
                                "required read returned wrong exit: {:?}; {}",
                                failed.status.code(),
                                String::from_utf8_lossy(&failed.stderr)
                            );
                            let diagnostic = String::from_utf8(failed.stderr)?;
                            ensure!(
                                diagnostic.contains("input.rs")
                                    && diagnostic.contains("Failed to read"),
                                "missing selected path/read diagnostic: {diagnostic}"
                            );
                            ensure!(
                                !out.join("manifest.json").exists(),
                                "read failure retained completion manifest"
                            );
                            if command == "context" {
                                ensure!(
                                    !out.join("receipt.json").exists(),
                                    "read failure retained receipt"
                                );
                            }
                        }
                        fs::write(&input, &valid)?;
                        let retry = run(root, command, nested, absolute, cap, compress)?;
                        ensure!(
                            retry.status.success(),
                            "repair failed: {}",
                            String::from_utf8_lossy(&retry.stderr)
                        );
                        ensure!(
                            fs::read(&payload_path)? == stable_bytes,
                            "retry changed successful bytes"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
