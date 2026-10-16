#![cfg(feature = "analysis")]

//! Literal gate JSON and exit contracts through real subprocess pipes.

use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use tempfile::TempDir;

const PASSED: &str = r#"{
  "passed": true,
  "policy": {
    "passed": true,
    "rule_results": [],
    "errors": 0,
    "warnings": 0
  },
  "ratchet": null,
  "total_errors": 0,
  "total_warnings": 0
}
"#;

const FAILED: &str = r#"{
  "passed": false,
  "policy": {
    "passed": false,
    "rule_results": [
      {
        "name": "cap",
        "passed": false,
        "level": "error",
        "actual": 3,
        "expected": "/metric <= 1",
        "message": null
      }
    ],
    "errors": 1,
    "warnings": 0
  },
  "ratchet": null,
  "total_errors": 1,
  "total_warnings": 0
}
"#;

const FAILING_POLICY: &str = "[[rules]]\nname = \"cap\"\npointer = \"/metric\"\nop = \"lte\"\nvalue = 1\n";

struct Fixture(TempDir);

impl Fixture {
    fn new() -> Result<Self> {
        let fixture = Self(tempfile::tempdir()?);
        fixture.write("tokmd.toml", "")?;
        fixture.write("receipt.json", r#"{"metric":3}"#)?;
        fixture.write("policy.toml", "rules = []\n")?;
        Ok(fixture)
    }

    fn write(&self, name: &str, text: &str) -> Result<()> {
        Ok(std::fs::write(self.0.path().join(name), text)?)
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("tokmd"));
        cmd.current_dir(self.0.path())
            .env("TOKMD_CONFIG", self.0.path().join("tokmd.toml"))
            .env("XDG_CONFIG_HOME", self.0.path())
            .env("APPDATA", self.0.path())
            .env_remove("TOKMD_PROFILE")
            .env_remove("TOKMD_PROGRESS_EVENTS")
            .env_remove("RUST_BACKTRACE")
            .args([
                "--no-progress",
                "gate",
                "receipt.json",
                "--policy",
                "policy.toml",
                "--format",
                "json",
            ]);
        cmd
    }

    fn output(&self) -> Result<Output> {
        Ok(assert_cmd::Command::from_std(self.command())
            .timeout(Duration::from_secs(10))
            .output()?)
    }

    fn writable(&self, expected: &str, code: i32) -> Result<()> {
        let output = self.output()?;
        assert_eq!(output.status.code(), Some(code), "{output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        // Consume the entire stream; a JSON prefix followed by chatter fails.
        let _: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        Ok(())
    }
}

#[test]
fn passing_json_has_literal_bytes_and_exit_zero() -> Result<()> {
    Fixture::new()?.writable(PASSED, 0)
}

#[test]
fn failed_gate_keeps_complete_json_and_exit_one() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.write("policy.toml", FAILING_POLICY)?;
    fixture.writable(FAILED, 1)
}

fn closed_pipe_journey(failing: bool) -> Result<()> {
    let fixture = Fixture::new()?;
    if failing {
        fixture.write("policy.toml", FAILING_POLICY)?;
    }
    // Close before spawning to avoid races with pipe capacity or scheduling.
    let (reader, writer) = std::io::pipe()?;
    drop(reader);
    let stderr_path = fixture.0.path().join("stderr.txt");
    let mut command = fixture.command();
    command
        .stdout(writer)
        .stderr(Stdio::from(std::fs::File::create(&stderr_path)?));
    let mut child = command.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            bail!("gate did not exit before the closed-pipe deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stderr = std::fs::read_to_string(stderr_path)?;
    assert_eq!(status.code(), Some(1), "{status:?}: {stderr}");
    assert!(
        stderr.starts_with("Error: Failed to write gate JSON to stdout:"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");
    // Identical argv, now with a working consumer, preserves the gate verdict.
    if failing {
        fixture.writable(FAILED, 1)
    } else {
        fixture.writable(PASSED, 0)
    }
}

#[test]
fn passing_gate_closed_pipe_returns_io_error_and_can_retry() -> Result<()> {
    closed_pipe_journey(false)
}

#[test]
fn failing_gate_closed_pipe_reports_transport_error_and_can_retry() -> Result<()> {
    closed_pipe_journey(true)
}

fn assert_input_error(output: &Output, code: i32, diagnostic: &str) -> Result<()> {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = std::str::from_utf8(&output.stderr)?;
    assert!(stderr.starts_with(diagnostic), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
    Ok(())
}

#[test]
fn truncated_receipt_emits_no_json_and_retries_after_repair() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.write("receipt.json", r#"{"metric":"#)?;
    assert_input_error(
        &fixture.output()?,
        1,
        "Error: Failed to parse JSON from receipt.json:",
    )?;
    fixture.write("receipt.json", r#"{"metric":3}"#)?;
    fixture.writable(PASSED, 0)
}

#[test]
fn malformed_policy_emits_no_json_and_retries_after_repair() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.write("policy.toml", "rules = [")?;
    assert_input_error(
        &fixture.output()?,
        2,
        "Error: Invalid gate policy: Failed to load policy from policy.toml:",
    )?;
    fixture.write("policy.toml", "rules = []\n")?;
    fixture.writable(PASSED, 0)
}

#[test]
fn malformed_selected_config_emits_no_json_and_retries_after_repair() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.write("tokmd.toml", "broken = [")?;
    assert_input_error(
        &fixture.output()?,
        1,
        "Error: Failed to load TOML config from ",
    )?;
    fixture.write("tokmd.toml", "")?;
    fixture.writable(PASSED, 0)
}
