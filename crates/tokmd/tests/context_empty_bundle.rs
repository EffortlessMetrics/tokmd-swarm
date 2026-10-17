//! Empty stdout bundles remain valid payloads, with recovery guidance on stderr.

use std::path::PathBuf;
use std::process::{Command, Output};

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
        let output = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&self.root)
            .output()?;
        anyhow::ensure!(output.status.success(), "git init failed: {output:?}");
        Ok(())
    }

    fn context_command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tokmd"));
        command
            .current_dir(&self.root)
            .env("TOKMD_CONFIG", &self.config)
            .env_remove("TOKMD_PROFILE")
            .env_remove("TOKMD_PROGRESS_EVENTS")
            .args(["--no-progress", "context"])
            .args(args);
        command
    }

    fn context(&self, args: &[&str]) -> anyhow::Result<Output> {
        Ok(self.context_command(args).output()?)
    }

    fn write_source(&self) -> anyhow::Result<()> {
        std::fs::write(self.root.join("main.rs"), "fn main() {}\n")?;
        Ok(())
    }
}

fn assert_empty_bundle_guidance(fixture: &Fixture) -> anyhow::Result<()> {
    let output = fixture.context(&["--mode", "bundle"])?;
    anyhow::ensure!(output.status.success(), "empty bundle failed: {output:?}");
    anyhow::ensure!(
        output.stdout.is_empty(),
        "guidance contaminated the payload"
    );
    let stderr = std::str::from_utf8(&output.stderr)?;
    for expected in [
        "No files were selected for the context bundle.",
        "scan paths",
        "ignore rules",
        "`tokmd check-ignore <path>`",
        "`--mode json`",
    ] {
        anyhow::ensure!(
            stderr.contains(expected),
            "missing recovery guidance {expected:?}: {stderr:?}"
        );
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
        .context_command(&["--mode", "bundle"])
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
                .context_command(&["--mode", mode, "--budget", "1"])
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
    let output = fixture.context(&["--mode", "bundle"])?;
    anyhow::ensure!(output.status.success(), "bundle failed: {output:?}");
    anyhow::ensure!(output.stdout == b"// === main.rs ===\nfn main() {}\n\n");
    anyhow::ensure!(output.stderr.is_empty(), "unexpected guidance: {output:?}");
    Ok(())
}

#[test]
fn no_ignore_recovers_the_ignored_source_without_guidance() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture.init_git()?;
    fixture.write_source()?;
    std::fs::write(fixture.root.join(".gitignore"), "*\n")?;
    let output = fixture.context(&["--mode", "bundle", "--no-ignore"])?;
    anyhow::ensure!(output.status.success(), "recovery failed: {output:?}");
    anyhow::ensure!(output.stdout == b"// === main.rs ===\nfn main() {}\n\n");
    anyhow::ensure!(output.stderr.is_empty(), "unexpected guidance: {output:?}");
    Ok(())
}

#[test]
fn empty_receipt_modes_remain_silent_on_stderr() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    for mode in ["list", "json"] {
        let output = fixture.context(&["--mode", mode])?;
        anyhow::ensure!(output.status.success(), "{mode} failed: {output:?}");
        anyhow::ensure!(output.stderr.is_empty(), "unexpected guidance: {output:?}");
        if mode == "json" {
            let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            anyhow::ensure!(receipt["file_count"] == 0);
            anyhow::ensure!(receipt["files"] == serde_json::json!([]));
        } else {
            anyhow::ensure!(std::str::from_utf8(&output.stdout)?.contains("Files: 0"));
        }
    }
    Ok(())
}

#[test]
fn empty_bundle_preserves_machine_readable_progress_stderr() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let output = fixture
        .context_command(&["--mode", "bundle"])
        .env("TOKMD_PROGRESS_EVENTS", "1")
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "progress bundle failed: {output:?}"
    );
    anyhow::ensure!(output.stdout.is_empty());
    let stderr = std::str::from_utf8(&output.stderr)?;
    anyhow::ensure!(!stderr.is_empty(), "missing requested progress events");
    for line in stderr.lines() {
        let event: serde_json::Value = serde_json::from_str(line)?;
        anyhow::ensure!(event["event"] == "tokmd.progress");
        anyhow::ensure!(event["schema_version"] == 1);
        anyhow::ensure!(event["kind"] == "update" || event["kind"] == "finish");
    }
    Ok(())
}

#[test]
fn empty_file_bundle_keeps_the_existing_write_notice() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let output = fixture.context(&["--mode", "bundle", "--output", "context.txt"])?;
    anyhow::ensure!(output.status.success(), "file bundle failed: {output:?}");
    anyhow::ensure!(output.stdout.is_empty());
    anyhow::ensure!(std::fs::read(fixture.root.join("context.txt"))?.is_empty());
    anyhow::ensure!(std::str::from_utf8(&output.stderr)? == "Wrote context.txt\n");
    Ok(())
}

#[test]
fn empty_directory_bundle_does_not_get_stdout_guidance() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let output = fixture.context(&["--mode", "bundle", "--output-dir", "bundle"])?;
    anyhow::ensure!(
        output.status.success(),
        "directory bundle failed: {output:?}"
    );
    anyhow::ensure!(output.stdout.is_empty());
    anyhow::ensure!(fixture.root.join("bundle/manifest.json").is_file());
    anyhow::ensure!(!std::str::from_utf8(&output.stderr)?.contains("No files were selected"));
    Ok(())
}

#[test]
fn tiny_budget_omission_is_a_selected_file_not_an_empty_bundle() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    fixture.write_source()?;
    let output = fixture.context(&["--mode", "bundle", "--budget", "1"])?;
    anyhow::ensure!(output.status.success(), "tiny budget failed: {output:?}");
    let stdout = std::str::from_utf8(&output.stdout)?;
    anyhow::ensure!(stdout.contains("// === main.rs ==="));
    anyhow::ensure!(stdout.contains("bytes omitted"));
    anyhow::ensure!(
        std::str::from_utf8(&output.stderr)?
            == "Warning: context budget (1 tokens; per-file cap 0 tokens) leaves no usable file content. Review --budget and --max-file-pct.\n"
    );
    Ok(())
}
