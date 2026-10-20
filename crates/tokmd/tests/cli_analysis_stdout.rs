#![cfg(feature = "analysis")]

use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"240\" height=\"32\" role=\"img\"><rect width=\"80\" height=\"32\" fill=\"#555\"/><rect x=\"80\" width=\"160\" height=\"32\" fill=\"#4c9aff\"/><text x=\"40\" y=\"20\" fill=\"#fff\" font-family=\"Verdana\" font-size=\"12\" text-anchor=\"middle\">tokens</text><text x=\"160\" y=\"20\" fill=\"#fff\" font-family=\"Verdana\" font-size=\"12\" text-anchor=\"middle\">10</text></svg>";

fn formats() -> Vec<&'static str> {
    let formats = vec![
        "md", "json", "jsonld", "xml", "svg", "mermaid", "tree", "html",
    ];
    #[cfg(feature = "fun")]
    let formats = [formats, vec!["obj", "midi"]].concat();
    formats
}

fn fixture() -> io::Result<TempDir> {
    let root = TempDir::new()?;
    std::fs::create_dir(root.path().join("source"))?;
    std::fs::write(root.path().join("source/main.rs"), "fn main() {}\n")?;
    std::fs::write(root.path().join("tokmd.toml"), "")?;
    Ok(root)
}

fn command(root: &TempDir, format: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    command
        .current_dir(root.path())
        .env("TOKMD_CONFIG", root.path().join("tokmd.toml"))
        .env("XDG_CONFIG_HOME", root.path())
        .env("APPDATA", root.path())
        .env("NO_COLOR", "1")
        .env_remove("TOKMD_PROFILE")
        .env_remove("TOKMD_PROGRESS_EVENTS")
        .env_remove("RUST_BACKTRACE")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("FORCE_COLOR")
        .args([
            "--no-progress",
            "analyze",
            "source",
            "--preset",
            "receipt",
            "--no-git",
            "--format",
            format,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn drain<R: Read>(stream: Option<R>) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if let Some(mut stream) = stream {
        stream.read_to_end(&mut bytes)?;
    }
    Ok(bytes)
}

fn finish(mut command: Command) -> io::Result<Output> {
    let mut child = command.spawn()?;
    // Drain while the child runs: HTML/JSON can exceed the pipe's capacity.
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout = std::thread::spawn(move || drain(stdout));
    let stderr = std::thread::spawn(move || drain(stderr));
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait()?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "analysis timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(Output {
        status,
        stdout: stdout
            .join()
            .map_err(|_| io::Error::other("stdout reader failed"))??,
        stderr: stderr
            .join()
            .map_err(|_| io::Error::other("stderr reader failed"))??,
    })
}

fn assert_success(output: &Output, format: &str) -> TestResult {
    assert_eq!(output.status.code(), Some(0), "{format}: {output:?}");
    assert!(output.stderr.is_empty(), "{format}: {output:?}");
    assert!(!output.stdout.is_empty(), "{format}");
    match format {
        "svg" => assert_eq!(output.stdout, SVG.as_bytes()),
        "midi" => {
            // One note, key 62, velocity 40, 180 ticks, tempo 120, then EOT.
            // This literal SMF oracle shares no code with the renderer.
            assert_eq!(output.stdout, b"MThd\0\0\0\x06\0\0\0\x01\x01\xe0MTrk\0\0\0\x14\0\xffQ\x03\x07\xa1\x20\0\x90\x3e\x28\x81\x34\x80\x3e\0\0\xff\x2f\0");
        }
        "json" => {
            let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(value["derived"]["totals"]["code"], 1);
            assert_eq!(value["derived"]["totals"]["files"], 1);
        }
        _ => {}
    }
    Ok(())
}

fn assert_failure(output: &Output, format: &str) {
    assert_eq!(output.status.code(), Some(1), "{format}: {output:?}");
    assert!(output.stdout.is_empty(), "{format}: {output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("analysis output to stdout"),
        "{format}: {stderr}"
    );
    assert!(!stderr.contains("panicked"), "{format}: {stderr}");
}

#[test]
fn successful_analysis_stdout_preserves_formats_and_file_bytes() -> TestResult {
    let root = fixture()?;
    for format in formats() {
        let output = finish(command(&root, format))?;
        assert_success(&output, format)?;
        if matches!(format, "svg" | "midi") {
            let directory = root.path().join(format!("out-{format}"));
            let mut to_file = command(&root, format);
            to_file.arg("--output-dir").arg(&directory);
            let file_output = finish(to_file)?;
            assert!(file_output.status.success(), "{file_output:?}");
            assert!(file_output.stdout.is_empty(), "{file_output:?}");
            assert!(file_output.stderr.is_empty(), "{file_output:?}");
            let extension = if format == "midi" { "mid" } else { "svg" };
            assert_eq!(
                std::fs::read(directory.join(format!("analysis.{extension}")))?,
                output.stdout
            );
        }
    }
    Ok(())
}

#[test]
fn closed_analysis_stdout_returns_error_and_same_argv_recovers() -> TestResult {
    let root = fixture()?;
    for format in formats() {
        let (reader, writer) = io::pipe()?;
        drop(reader); // Closed before spawn: independent of pipe capacity or scheduling.
        let mut failed = command(&root, format);
        failed.stdout(writer);
        assert_failure(&finish(failed)?, format);
        assert_success(&finish(command(&root, format))?, format)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn full_analysis_stdout_returns_error_and_same_argv_recovers() -> TestResult {
    let root = fixture()?;
    for format in formats() {
        let full = std::fs::OpenOptions::new().write(true).open("/dev/full")?;
        let mut failed = command(&root, format);
        failed.stdout(full);
        assert_failure(&finish(failed)?, format);
        assert_success(&finish(command(&root, format))?, format)?;
    }
    Ok(())
}
