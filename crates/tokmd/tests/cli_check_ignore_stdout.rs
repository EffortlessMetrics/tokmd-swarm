//! Check-ignore reports must reach the consumer before returning a decision.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Result, bail, ensure};
use tempfile::TempDir;

struct Fixture(TempDir);

impl Fixture {
    fn new() -> Result<Self> {
        let fixture = Self(tempfile::tempdir()?);
        for (name, contents) in [
            ("tokmd.toml", ""),
            (".tokeignore", "ignored.rs\n"),
            ("ignored.rs", "fn ignored() {}\n"),
            ("keep.rs", "fn keep() {}\n"),
        ] {
            std::fs::write(fixture.0.path().join(name), contents)?;
        }
        Ok(fixture)
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(assert_cmd::cargo::cargo_bin!("tokmd"));
        command.current_dir(self.0.path());
        for (name, _) in std::env::vars_os() {
            let key = name.to_string_lossy();
            if key.starts_with("TOKMD_") || key.starts_with("GIT_") {
                command.env_remove(name);
            }
        }
        command
            .env("TOKMD_CONFIG", self.0.path().join("tokmd.toml"))
            .env("XDG_CONFIG_HOME", self.0.path())
            .env("APPDATA", self.0.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.0.path().join("tokmd.toml"))
            .env_remove("RUST_BACKTRACE")
            .args(["--no-progress", "check-ignore"])
            .args(args);
        command
    }

    fn writable(&self, args: &[&str], code: i32, stdout: &str) -> Result<()> {
        let output = assert_cmd::Command::from_std(self.command(args))
            .timeout(Duration::from_secs(10))
            .output()?;
        ensure!(output.status.code() == Some(code), "{output:?}");
        ensure!(output.stdout == stdout.as_bytes(), "{output:?}");
        ensure!(output.stderr.is_empty(), "{output:?}");
        Ok(())
    }

    fn failed_sink(&self, args: &[&str], sink: impl Into<Stdio>) -> Result<()> {
        let stderr_path = self.0.path().join("stderr.txt");
        let mut command = self.command(args);
        command
            .stdout(sink)
            .stderr(std::fs::File::create(&stderr_path)?);
        let mut child = command.spawn()?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill()?;
                child.wait()?;
                bail!("check-ignore did not exit before the failed-sink deadline");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let stderr = std::fs::read_to_string(stderr_path)?;
        ensure!(status.code() == Some(1), "{status:?}: {stderr}");
        ensure!(
            stderr.starts_with("Error: Failed to write check-ignore report to stdout:"),
            "{stderr}"
        );
        ensure!(!stderr.contains("panicked"), "{stderr}");
        Ok(())
    }
}

const CASES: &[(&[&str], i32, &str)] = &[
    (&["ignored.rs"], 0, "ignored.rs: ignored\n"),
    (
        &["-v", "ignored.rs"],
        0,
        "ignored.rs: ignored\n  .tokeignore: ignored.rs\n",
    ),
    (&["keep.rs"], 1, "keep.rs: not ignored\n"),
    (&["-v", "keep.rs"], 1, "keep.rs: not ignored\n"),
    (
        &["ignored.rs", "keep.rs"],
        1,
        "ignored.rs: ignored\nkeep.rs: not ignored\n",
    ),
];

#[test]
fn writable_reports_keep_literal_bytes_and_decision_exits() -> Result<()> {
    let fixture = Fixture::new()?;
    for (args, code, expected) in CASES {
        fixture.writable(args, *code, expected)?;
    }
    Ok(())
}

#[test]
fn closed_consumer_returns_io_error_then_identical_argv_recovers() -> Result<()> {
    let fixture = Fixture::new()?;
    for (args, code, expected) in CASES {
        // Close before spawn, independent of pipe capacity or scheduling.
        let (reader, writer) = std::io::pipe()?;
        drop(reader);
        fixture.failed_sink(args, writer)?;
        fixture.writable(args, *code, expected)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn full_sink_returns_io_error_then_identical_argv_recovers() -> Result<()> {
    let fixture = Fixture::new()?;
    for (args, code, expected) in CASES {
        fixture.failed_sink(args, std::fs::OpenOptions::new().write(true).open("/dev/full")?)?;
        fixture.writable(args, *code, expected)?;
    }
    Ok(())
}
