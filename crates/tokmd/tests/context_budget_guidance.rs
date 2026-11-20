use anyhow::Result;
use assert_cmd::Command;
use assert_cmd::assert::OutputAssertExt;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Output;
use std::time::Duration;
use tempfile::tempdir;

const WARNING: &str = "Warning: context budget (1 tokens; per-file cap 0 tokens) leaves no usable file content. Review --budget and --max-file-pct.\n";

fn source(dense: bool) -> String {
    if dense {
        format!(
            "pub fn budget_fixture() {{ let text = \"{}\"; }}\n",
            "x".repeat(400)
        )
    } else {
        "pub fn budget_fixture() {}\n".to_string()
    }
}

fn context(root: &Path, mode: &str, budget: &str, strategy: &str, events: bool) -> Result<Output> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    cmd.current_dir(root)
        .timeout(Duration::from_secs(15))
        .env_remove("TOKMD_PROGRESS_EVENTS")
        .args([
            "context",
            "--no-progress",
            "--no-git",
            "--mode",
            mode,
            "--budget",
            budget,
            "--strategy",
            strategy,
        ]);
    if events {
        // Presence, including an empty value, enables the machine progress stream.
        cmd.env("TOKMD_PROGRESS_EVENTS", "");
    }
    let output = cmd.output()?;
    assert!(output.status.success(), "{output:?}");
    Ok(output)
}

#[test]
fn tiny_budget_names_the_cause_and_larger_budget_recovers_the_same_file() -> Result<()> {
    for dense in [false, true] {
        let dir = tempdir()?;
        let content = source(dense);
        fs::write(dir.path().join("sample.rs"), &content)?;
        for strategy in ["greedy", "spread"] {
            for mode in ["bundle", "list", "json"] {
                let small = context(dir.path(), mode, "1", strategy, false)?;
                // A generic empty/ignore hint or the wrong budget must fail literally.
                small.clone().assert().stderr(predicate::str::diff(WARNING));
                if mode == "json" {
                    let receipt: serde_json::Value = serde_json::from_slice(&small.stdout)?;
                    assert_eq!(receipt["budget_tokens"], 1);
                    assert_eq!(receipt["used_tokens"], 0);
                    assert_eq!(receipt["file_count"], if dense { 0 } else { 1 });
                } else if mode == "bundle" && dense {
                    assert!(small.stdout.is_empty());
                }

                let recovered = context(dir.path(), mode, "1000", strategy, false)?;
                assert!(recovered.stderr.is_empty(), "{recovered:?}");
                if mode == "json" {
                    let receipt: serde_json::Value = serde_json::from_slice(&recovered.stdout)?;
                    assert_eq!(receipt["budget_tokens"], 1000);
                    assert_eq!(receipt["file_count"], 1);
                    assert_eq!(receipt["files"][0]["path"], "sample.rs");
                    assert!(
                        receipt["used_tokens"]
                            .as_u64()
                            .is_some_and(|tokens| tokens > 0)
                    );
                } else {
                    let text = String::from_utf8_lossy(&recovered.stdout);
                    assert!(text.contains("sample.rs"));
                    if mode == "bundle" {
                        assert!(text.contains(content.trim_end()));
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn empty_ignored_unborn_and_smart_excluded_inputs_do_not_claim_a_budget_cause() -> Result<()> {
    for kind in ["empty", "ignored", "unborn", "smart", "empty_file"] {
        let dir = tempdir()?;
        if matches!(kind, "ignored" | "unborn") {
            let status = std::process::Command::new("git")
                .args(["init", "--quiet"])
                .arg(dir.path())
                .status()?;
            assert!(status.success());
        }
        match kind {
            "ignored" => {
                fs::write(dir.path().join(".gitignore"), "*.rs\n")?;
                fs::write(dir.path().join("sample.rs"), source(true))?;
            }
            "smart" => fs::write(dir.path().join("Cargo.lock"), "version = 4\n")?,
            "empty_file" => fs::write(dir.path().join("sample.rs"), "")?,
            _ => {}
        }
        for mode in ["bundle", "json"] {
            let output = context(dir.path(), mode, "1", "greedy", false)?;
            assert!(
                !String::from_utf8_lossy(&output.stderr).contains("--budget"),
                "{kind}: {output:?}"
            );
            if mode == "bundle" && kind != "empty_file" {
                assert!(output.stdout.is_empty());
            } else if mode == "json" {
                let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)?;
                assert_eq!(
                    receipt["file_count"],
                    if kind == "empty_file" { 1 } else { 0 },
                    "{kind}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn explicit_hard_cap_is_not_mislabeled_as_a_small_budget() -> Result<()> {
    let dir = tempdir()?;
    fs::write(dir.path().join("sample.rs"), source(true))?;
    let output = Command::new(env!("CARGO_BIN_EXE_tokmd"))
        .current_dir(dir.path())
        .timeout(Duration::from_secs(15))
        .env_remove("TOKMD_PROGRESS_EVENTS")
        .args([
            "context",
            "--mode",
            "json",
            "--budget",
            "1000",
            "--max-file-tokens",
            "1",
        ])
        .output()?;
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("--budget"));
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(receipt["file_count"], 0);
    assert_eq!(receipt["excluded_by_policy"][0]["original_tokens"], 110);
    Ok(())
}

#[test]
fn budget_guidance_keeps_machine_progress_json_and_stdout_contracts() -> Result<()> {
    let dir = tempdir()?;
    fs::write(dir.path().join("sample.rs"), source(true))?;
    let output = context(dir.path(), "json", "1", "greedy", true)?;
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(receipt["file_count"], 0);
    let stderr = String::from_utf8(output.stderr)?;
    let events: Vec<serde_json::Value> = stderr
        .lines()
        .map(serde_json::from_str)
        .collect::<serde_json::Result<_>>()?;
    assert_eq!(events.len(), 5);
    for event in &events {
        assert_eq!(event["event"], "tokmd.progress");
        assert_eq!(event["schema_version"], 1);
    }
    assert_eq!(events[4]["kind"], "finish");
    assert_eq!(events[4]["message"], "done");
    Ok(())
}

#[test]
fn budget_guidance_does_not_count_as_saved_bundle_or_log_bytes() -> Result<()> {
    let dir = tempdir()?;
    fs::write(dir.path().join("sample.rs"), source(true))?;
    let output_path = dir.path().join("bundle.txt");
    let log_path = dir.path().join("context.jsonl");
    for budget in ["1", "1000"] {
        let output = Command::new(env!("CARGO_BIN_EXE_tokmd"))
            .current_dir(dir.path())
            .timeout(Duration::from_secs(15))
            .env_remove("TOKMD_PROGRESS_EVENTS")
            .args([
                "context", "--mode", "bundle", "--budget", budget, "--force", "--output",
            ])
            .arg("bundle.txt")
            .arg("--log")
            .arg("context.jsonl")
            .output()?;
        assert!(output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr)?;
        assert_eq!(stderr.matches(WARNING).count(), usize::from(budget == "1"));
        assert!(!stderr.contains("ignore"));
        let bytes = fs::read(&output_path)?;
        let log = fs::read_to_string(&log_path)?;
        let record: serde_json::Value = serde_json::from_str(
            log.lines()
                .last()
                .ok_or_else(|| anyhow::anyhow!("missing log record"))?,
        )?;
        assert_eq!(record["total_bytes"], bytes.len());
        assert_eq!(record["file_count"], usize::from(budget == "1000"));
        if budget == "1" {
            assert!(bytes.is_empty());
        } else {
            assert!(String::from_utf8(bytes)?.contains(source(true).trim_end()));
        }
    }
    Ok(())
}
