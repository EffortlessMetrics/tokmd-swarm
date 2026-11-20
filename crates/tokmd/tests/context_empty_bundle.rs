//! Empty stdout bundles remain valid payloads, with recovery guidance on stderr.

use assert_cmd::Command;
use assert_cmd::assert::Assert;
use predicates::prelude::*;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    config: PathBuf,
}

impl Fixture {
    fn new() -> anyhow::Result<Self> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("repo");
        std::fs::create_dir(&root)?;
        // Keep the explicit config outside the inventory of an empty repository.
        let config = temp.path().join("tokmd.toml");
        std::fs::write(&config, "")?;
        Ok(Self {
            _temp: temp,
            root,
            config,
        })
    }

    fn init_git(&self) -> anyhow::Result<()> {
        let output = ProcessCommand::new("git")
            .args(["init", "--quiet"])
            .current_dir(&self.root)
            .output()?;
        anyhow::ensure!(output.status.success(), "git init failed: {output:?}");
        Ok(())
    }

    fn raw_context_command(&self, args: &[&str]) -> ProcessCommand {
        let mut command = ProcessCommand::new(env!("CARGO_BIN_EXE_tokmd"));
        command
            .current_dir(&self.root)
            .env("TOKMD_CONFIG", &self.config)
            .env_remove("TOKMD_PROFILE")
            .env_remove("TOKMD_PROGRESS_EVENTS")
            .args(["--no-progress", "context"])
            .args(args);
        command
    }

    fn context_command(&self, args: &[&str]) -> Command {
        Command::from_std(self.raw_context_command(args))
    }

    fn context(&self, args: &[&str]) -> Assert {
        self.context_command(args).assert().success()
    }

    fn write_source(&self) -> anyhow::Result<()> {
        std::fs::write(self.root.join("main.rs"), "fn main() {}\n")?;
        Ok(())
    }
}

fn assert_empty_bundle_guidance(fixture: &Fixture) -> anyhow::Result<()> {
    let mut assertion = fixture
        .context(&["--mode", "bundle"])
        .stdout(predicate::str::is_empty());
    for expected in [
        "No files were selected for the context bundle.",
        "scan paths",
        "ignore rules",
        "`tokmd check-ignore <path>`",
        "`--mode json`",
    ] {
        assertion = assertion.stderr(predicate::str::contains(expected));
    }
    Ok(())
}

#[test]
fn empty_stdout_bundle_has_guidance() -> anyhow::Result<()> {
    assert_empty_bundle_guidance(&Fixture::new()?)
}

#[test]
fn unborn_git_stdout_bundle_has_guidance() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture.init_git()?;
    assert_empty_bundle_guidance(&fixture)
}

#[test]
fn empty_stdout_bundle_keeps_success_with_a_closed_stderr_reader() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let (reader, writer) = std::io::pipe()?;
    drop(reader);
    let output = fixture
        .raw_context_command(&["--mode", "bundle"])
        .stderr(writer)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "optional guidance failed: {output:?}"
    );
    anyhow::ensure!(output.stdout.is_empty());
    Ok(())
}

#[test]
fn budget_guidance_keeps_success_with_a_closed_stderr_reader() -> anyhow::Result<()> {
    for dense in [false, true] {
        let fixture = Fixture::new()?;
        if dense {
            std::fs::write(
                fixture.root.join("main.rs"),
                format!(
                    "pub fn budget_fixture() {{ let text = \"{}\"; }}\n",
                    "x".repeat(400)
                ),
            )?;
        } else {
            fixture.write_source()?;
        }
        for mode in ["bundle", "json"] {
            let (reader, writer) = std::io::pipe()?;
            drop(reader);
            let output = fixture
                .raw_context_command(&["--mode", mode, "--budget", "1"])
                .stderr(writer)
                .output()?;
            anyhow::ensure!(
                output.status.success(),
                "optional budget guidance failed: {output:?}"
            );
            if mode == "json" {
                let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)?;
                anyhow::ensure!(receipt["file_count"] == usize::from(!dense));
                anyhow::ensure!(receipt["used_tokens"] == 0);
            } else if dense {
                anyhow::ensure!(output.stdout.is_empty());
            } else {
                anyhow::ensure!(std::str::from_utf8(&output.stdout)?.contains("bytes omitted"));
            }
        }
    }
    Ok(())
}

#[test]
fn ignored_only_stdout_bundle_has_guidance() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture.init_git()?;
    fixture.write_source()?;
    std::fs::write(fixture.root.join(".gitignore"), "*\n")?;
    assert_empty_bundle_guidance(&fixture)
}

#[test]
fn normal_stdout_bundle_keeps_its_payload_and_stderr() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture.write_source()?;
    fixture
        .context(&["--mode", "bundle"])
        .stdout(predicate::str::diff("// === main.rs ===\nfn main() {}\n\n"))
        .stderr(predicate::str::is_empty());
    Ok(())
}

#[test]
fn no_ignore_recovers_the_ignored_source_without_guidance() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture.init_git()?;
    fixture.write_source()?;
    std::fs::write(fixture.root.join(".gitignore"), "*\n")?;
    fixture
        .context(&["--mode", "bundle", "--no-ignore"])
        .stdout(predicate::str::diff("// === main.rs ===\nfn main() {}\n\n"))
        .stderr(predicate::str::is_empty());
    Ok(())
}

#[test]
fn empty_receipt_modes_remain_silent_on_stderr() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    for mode in ["list", "json"] {
        let assertion = fixture
            .context(&["--mode", mode])
            .stderr(predicate::str::is_empty());
        if mode == "json" {
            let receipt: serde_json::Value =
                serde_json::from_slice(&assertion.get_output().stdout)?;
            assert_eq!(receipt["file_count"], 0);
            assert_eq!(receipt["files"], serde_json::json!([]));
        } else {
            assertion.stdout(predicate::str::contains("Files: 0"));
        }
    }
    Ok(())
}

#[test]
fn empty_bundle_preserves_machine_readable_progress_stderr() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let assertion = fixture
        .context_command(&["--mode", "bundle"])
        .env("TOKMD_PROGRESS_EVENTS", "1")
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty().not());
    let stderr = std::str::from_utf8(&assertion.get_output().stderr)?;
    for line in stderr.lines() {
        let event: serde_json::Value = serde_json::from_str(line)?;
        assert_eq!(event["event"], "tokmd.progress");
        assert_eq!(event["schema_version"], 1);
        assert!(event["kind"] == "update" || event["kind"] == "finish");
    }
    Ok(())
}

#[test]
fn empty_file_bundle_keeps_the_existing_write_notice() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture
        .context(&["--mode", "bundle", "--output", "context.txt"])
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::diff("Wrote context.txt\n"));
    assert!(std::fs::read(fixture.root.join("context.txt"))?.is_empty());
    Ok(())
}

#[test]
fn empty_directory_bundle_does_not_get_stdout_guidance() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture
        .context(&["--mode", "bundle", "--output-dir", "bundle"])
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("No files were selected").not());
    assert!(fixture.root.join("bundle/manifest.json").is_file());
    Ok(())
}

#[test]
fn tiny_budget_omission_is_a_selected_file_not_an_empty_bundle() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture.write_source()?;
    fixture
        .context(&["--mode", "bundle", "--budget", "1"])
        .stdout(
            predicate::str::contains("// === main.rs ===")
                .and(predicate::str::contains("bytes omitted")),
        )
        .stderr(predicate::str::diff(
            "Warning: context budget (1 tokens; per-file cap 0 tokens) leaves no usable file content. Review --budget and --max-file-pct.\n",
        ));
    Ok(())
}
