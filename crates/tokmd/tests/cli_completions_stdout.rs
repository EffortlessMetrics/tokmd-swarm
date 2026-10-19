//! Completion scripts preserve their bytes and return ordinary output errors.

use anyhow::{Context, Result};
use clap::CommandFactory;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

const SHELLS: [(&str, clap_complete::Shell); 5] = [
    ("bash", clap_complete::Shell::Bash),
    ("elvish", clap_complete::Shell::Elvish),
    ("fish", clap_complete::Shell::Fish),
    ("powershell", clap_complete::Shell::PowerShell),
    ("zsh", clap_complete::Shell::Zsh),
];

fn root() -> Result<TempDir> {
    let root = TempDir::new()?;
    std::fs::write(root.path().join("tokmd.toml"), "")?;
    Ok(root)
}

fn command(root: &Path, shell: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    cmd.current_dir(root)
        .env("TOKMD_CONFIG", root.join("tokmd.toml"))
        .env("XDG_CONFIG_HOME", root)
        .env("APPDATA", root)
        .env_remove("TOKMD_PROFILE")
        .env_remove("TOKMD_PROGRESS_EVENTS")
        .env_remove("RUST_BACKTRACE")
        .args(["--no-progress", "completions", shell]);
    cmd
}

fn assert_successful_retry(
    root: &Path,
    shell: &str,
    generator: clap_complete::Shell,
) -> Result<()> {
    let output = command(root, shell).output().context("run completions")?;
    anyhow::ensure!(output.status.success(), "{shell}: {output:?}");
    anyhow::ensure!(output.stderr.is_empty(), "{shell}: {output:?}");
    let mut expected = Vec::new();
    let mut cmd = tokmd::cli::Cli::command();
    let name = cmd.get_name().to_owned();
    clap_complete::generate(generator, &mut cmd, name, &mut expected);
    anyhow::ensure!(!expected.is_empty(), "empty {shell} control");
    assert_eq!(output.stdout, expected, "{shell} script changed");
    Ok(())
}

fn assert_output_error(output: std::process::Output, shell: &str) -> Result<()> {
    assert_eq!(output.status.code(), Some(1), "{shell}: {output:?}");
    let stderr = String::from_utf8(output.stderr)?;
    anyhow::ensure!(
        stderr.starts_with("Error: Failed to write completions to stdout:"),
        "{shell}: {stderr}"
    );
    anyhow::ensure!(!stderr.contains("panicked"), "{shell}: {stderr}");
    Ok(())
}

#[test]
fn successful_scripts_match_generators() -> Result<()> {
    let root = root()?;
    for (shell, generator) in SHELLS {
        assert_successful_retry(root.path(), shell, generator)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn full_destination_returns_error_then_same_arguments_recover() -> Result<()> {
    let root = root()?;
    for (shell, generator) in SHELLS {
        let full = std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .context("open deterministic full destination")?;
        assert_output_error(command(root.path(), shell).stdout(full).output()?, shell)?;
        assert_successful_retry(root.path(), shell, generator)?;
    }
    Ok(())
}

#[test]
fn closed_pipe_returns_error_then_same_arguments_recover() -> Result<()> {
    let root = root()?;
    for (shell, generator) in SHELLS {
        // Close before spawning: this does not depend on pipe size or timing.
        let (reader, writer) = std::io::pipe()?;
        drop(reader);
        let output = command(root.path(), shell)
            .stdout(writer)
            .stderr(Stdio::piped())
            .output()?;
        assert_output_error(output, shell)?;
        assert_successful_retry(root.path(), shell, generator)?;
    }
    Ok(())
}
