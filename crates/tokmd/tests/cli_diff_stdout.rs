//! Diff transport failures must remain ordinary CLI errors.

use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use tempfile::TempDir;

const MARKDOWN: &str = "\
## Diff: old.json → new.json

### Summary

|Metric|From|To|Delta|Change|
|---|---:|---:|---:|---:|
|LOC|10|12|+2|+20.0%|
|Lines|10|12|+2|+20.0%|
|Files|1|1|0|+0.0%|
|Bytes|100|120|+20|+20.0%|
|Tokens|20|24|+4|+20.0%|

### Language Movement

|Type|Count|
|---|---:|
|Changed|1|
|Added|0|
|Removed|0|
|Modified|1|

### Language Breakdown

|Language|Old LOC|New LOC|Delta|
|---|---:|---:|---:|
|Rust|10|12|+2|
|**Total**|10|12|+2|
";

const COMPACT: &str = "\
## Diff: old.json → new.json

### Summary

|Metric|Value|
|---|---:|
|From LOC|10|
|To LOC|12|
|Delta LOC|+2|
|LOC Change|+20.0%|
|Delta Lines|+2|
|Delta Files|0|
|Delta Bytes|+20|
|Delta Tokens|+4|
|Languages changed|1|
|Languages added|0|
|Languages removed|0|
|Languages modified|1|
";

struct Fixture(TempDir);

impl Fixture {
    fn new() -> Result<Self> {
        let fixture = Self(tempfile::tempdir()?);
        std::fs::write(fixture.0.path().join("tokmd.toml"), "")?;
        for (name, code, bytes, tokens) in [("old.json", 10, 100, 20), ("new.json", 12, 120, 24)] {
            let total = json!({
                "code": code, "lines": code, "files": 1,
                "bytes": bytes, "tokens": tokens, "avg_lines": code
            });
            let mut row = total.clone();
            row.as_object_mut()
                .context("row object")?
                .insert("lang".into(), json!("Rust"));
            let receipt = json!({
                "schema_version": 2, "generated_at_ms": 0,
                "tool": {"name": "tokmd", "version": "fixture"},
                "mode": "lang", "status": "complete", "warnings": [],
                "scan": {
                    "paths": ["."], "excluded": [], "config": "none",
                    "hidden": false, "no_ignore": false, "no_ignore_parent": false,
                    "no_ignore_dot": false, "no_ignore_vcs": false,
                    "treat_doc_strings_as_comments": false
                },
                "args": {"format": "json", "top": 0, "with_files": false, "children": "collapse"},
                "rows": [row], "total": total, "with_files": false, "children": "collapse", "top": 0
            });
            std::fs::write(fixture.0.path().join(name), serde_json::to_vec(&receipt)?)?;
        }
        Ok(fixture)
    }

    fn run(&self, options: &[&str], closed_pipe: bool) -> Result<Output> {
        let stdout_path = self.0.path().join("stdout.txt");
        let stderr_path = self.0.path().join("stderr.txt");
        let stdout = if closed_pipe {
            // Close before spawn: independent of output size and scheduling.
            let (reader, writer) = std::io::pipe()?;
            drop(reader);
            Stdio::from(writer)
        } else {
            Stdio::from(std::fs::File::create(&stdout_path)?)
        };
        let mut child = Command::new(assert_cmd::cargo::cargo_bin!("tokmd"))
            .current_dir(self.0.path())
            .env("TOKMD_CONFIG", self.0.path().join("tokmd.toml"))
            .env("XDG_CONFIG_HOME", self.0.path())
            .env("APPDATA", self.0.path())
            .env_remove("TOKMD_PROFILE")
            .env_remove("TOKMD_PROGRESS_EVENTS")
            .env_remove("RUST_BACKTRACE")
            .args([
                "--no-progress",
                "--config",
                "none",
                "diff",
                "old.json",
                "new.json",
            ])
            .args(options)
            .stdout(stdout)
            .stderr(Stdio::from(std::fs::File::create(&stderr_path)?))
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill()?;
                child.wait()?;
                bail!("diff did not exit before the output deadline");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        Ok(Output {
            status,
            stdout: if closed_pipe {
                Vec::new()
            } else {
                std::fs::read(stdout_path)?
            },
            stderr: std::fs::read(stderr_path)?,
        })
    }

    fn writable(&self, options: &[&str], expected: Option<&str>) -> Result<()> {
        let output = self.run(options, false)?;
        ensure!(output.status.code() == Some(0), "{output:?}");
        ensure!(output.stderr.is_empty(), "{output:?}");
        if let Some(expected) = expected {
            ensure!(output.stdout == expected.as_bytes(), "{output:?}");
        } else {
            ensure!(
                output.stdout.ends_with(b"}\n"),
                "missing final JSON newline: {output:?}"
            );
            ensure!(
                output.stdout.split(|&b| b == b'\n').count() == 2,
                "JSON must occupy exactly one line: {output:?}"
            );
            let mut value: Value = serde_json::from_slice(&output.stdout)?;
            let object = value.as_object_mut().context("diff object")?;
            ensure!(
                object
                    .remove("generated_at_ms")
                    .and_then(|v| v.as_u64())
                    .is_some(),
                "timestamp missing"
            );
            ensure!(
                object.remove("tool")
                    == Some(json!({"name":"tokmd", "version":env!("CARGO_PKG_VERSION")})),
                "tool metadata mismatch"
            );
            ensure!(
                value
                    == json!({
                        "schema_version": 2, "mode": "diff", "from_source": "old.json", "to_source": "new.json",
                        "diff_rows": [{
                            "lang": "Rust", "old_code": 10, "new_code": 12, "delta_code": 2,
                            "old_lines": 10, "new_lines": 12, "delta_lines": 2,
                            "old_files": 1, "new_files": 1, "delta_files": 0,
                            "old_bytes": 100, "new_bytes": 120, "delta_bytes": 20,
                            "old_tokens": 20, "new_tokens": 24, "delta_tokens": 4
                        }],
                        "totals": {
                            "old_code": 10, "new_code": 12, "delta_code": 2,
                            "old_lines": 10, "new_lines": 12, "delta_lines": 2,
                            "old_files": 1, "new_files": 1, "delta_files": 0,
                            "old_bytes": 100, "new_bytes": 120, "delta_bytes": 20,
                            "old_tokens": 20, "new_tokens": 24, "delta_tokens": 4
                        }
                    }),
                "literal diff receipt mismatch: {value}"
            );
        }
        Ok(())
    }
}

fn journey(options: &[&str], expected: Option<&str>) -> Result<()> {
    let fixture = Fixture::new()?;
    let output = fixture.run(options, true)?;
    let stderr = std::str::from_utf8(&output.stderr)?;
    ensure!(output.status.code() == Some(1), "{output:?}");
    ensure!(
        stderr.starts_with("Error: Failed to write diff to stdout:"),
        "{stderr}"
    );
    ensure!(!stderr.contains("panicked"), "{stderr}");
    // Same argv and inputs, with a writable destination, must recover.
    fixture.writable(options, expected)
}

#[test]
fn markdown_keeps_literal_bytes() -> Result<()> {
    Fixture::new()?.writable(&["--format", "md", "--color", "never"], Some(MARKDOWN))
}

#[test]
fn compact_markdown_keeps_literal_bytes() -> Result<()> {
    Fixture::new()?.writable(
        &["--format", "md", "--compact", "--color", "never"],
        Some(COMPACT),
    )
}

#[test]
fn json_keeps_literal_receipt_and_one_newline() -> Result<()> {
    Fixture::new()?.writable(&["--format", "json"], None)
}

#[test]
fn markdown_closed_pipe_returns_error_and_retries() -> Result<()> {
    journey(&["--format", "md", "--color", "never"], Some(MARKDOWN))
}

#[test]
fn compact_markdown_closed_pipe_returns_error_and_retries() -> Result<()> {
    journey(
        &["--format", "md", "--compact", "--color", "never"],
        Some(COMPACT),
    )
}

#[test]
fn json_closed_pipe_returns_error_and_retries() -> Result<()> {
    journey(&["--format", "json"], None)
}
