#![cfg(feature = "analysis")]

use std::io;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};
use tempfile::{TempDir, tempdir};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// Literal oracle: one Rust line, no renderer shared with production, no final LF.
const EXPECTED: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"24\" role=\"img\"><rect width=\"60\" height=\"24\" fill=\"#555\"/><rect x=\"60\" width=\"60\" height=\"24\" fill=\"#4c9aff\"/><text x=\"30\" y=\"16\" fill=\"#fff\" font-family=\"Verdana\" font-size=\"11\" text-anchor=\"middle\">lines</text><text x=\"90\" y=\"16\" fill=\"#fff\" font-family=\"Verdana\" font-size=\"11\" text-anchor=\"middle\">1</text></svg>";

fn fixture() -> io::Result<TempDir> {
    let dir = tempdir()?;
    std::fs::write(dir.path().join("main.rs"), "fn main() {}\n")?;
    Ok(dir)
}

fn command(dir: &TempDir) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    cmd.current_dir(dir.path())
        .args(["--no-progress", "badge", "--metric", "lines"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn finish(mut child: Child) -> io::Result<Output> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait()?;
            return Err(io::Error::new(io::ErrorKind::TimedOut, "badge timed out"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, EXPECTED.as_bytes());
    assert!(output.stderr.is_empty(), "{output:?}");
}

fn assert_output_failure(output: &Output) {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("badge SVG to stdout"), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn badge_stdout_preserves_exact_svg_and_file_output() -> TestResult {
    let dir = fixture()?;
    assert_success(&finish(command(&dir).spawn()?)?);

    let path = dir.path().join("badge.svg");
    let output = finish(command(&dir).arg("--out").arg(&path).spawn()?)?;
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_eq!(std::fs::read(path)?, EXPECTED.as_bytes());
    Ok(())
}

#[test]
fn badge_closed_stdout_returns_error_and_same_argv_recovers() -> TestResult {
    let dir = fixture()?;
    let (reader, writer) = io::pipe()?;
    drop(reader); // The reader is closed before spawn, so no consumer race exists.
    let output = finish(command(&dir).stdout(writer).spawn()?)?;
    assert_output_failure(&output);
    assert_success(&finish(command(&dir).spawn()?)?);
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn badge_full_stdout_returns_error_and_same_argv_recovers() -> TestResult {
    let dir = fixture()?;
    let full = std::fs::OpenOptions::new().write(true).open("/dev/full")?;
    let output = finish(command(&dir).stdout(full).spawn()?)?;
    assert_output_failure(&output);
    assert_success(&finish(command(&dir).spawn()?)?);
    Ok(())
}
