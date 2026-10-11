use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};

fn run(root: &Path, command: &str, format: &str, extra: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_tokmd"))
        .current_dir(root)
        .env_remove("TOKMD_CONFIG")
        .env_remove("TOKMD_PROFILE")
        .args([
            "--no-progress",
            "--config",
            "none",
            command,
            ".",
            "--format",
            format,
        ])
        .args(extra)
        .output()
}

fn assert_complete(
    output: Output,
    command: &str,
    format: &str,
    files: usize,
) -> anyhow::Result<()> {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let text = String::from_utf8(output.stdout)?;
    let lines: Vec<Value> = text
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let receipt = lines
        .first()
        .ok_or_else(|| anyhow::anyhow!("missing receipt"))?;
    assert_eq!(receipt["schema_version"], json!(2));
    assert_eq!(receipt["mode"], json!(command));
    assert_eq!(receipt["status"], json!("complete"));
    assert_eq!(receipt["warnings"], json!([]));
    if format == "jsonl" {
        assert_eq!(receipt["type"], json!("meta"));
        assert_eq!(lines.len(), files + 1);
        for row in lines.iter().skip(1) {
            assert_eq!(row["type"], json!("row"));
            assert_eq!(row["code"], json!(1));
        }
    } else {
        assert_eq!(lines.len(), 1);
        assert_eq!(
            receipt["rows"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("missing rows"))?
                .len(),
            files
        );
        if command != "export" {
            assert_eq!(receipt["total"]["files"], json!(files));
            assert_eq!(receipt["total"]["code"], json!(files));
        }
    }
    Ok(())
}

const FORMATS: &[(&str, &str)] = &[
    ("lang", "json"),
    ("module", "json"),
    ("export", "json"),
    ("export", "jsonl"),
];

#[test]
fn valid_empty_and_nonempty_machine_outputs_remain_complete() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    for &(command, format) in FORMATS {
        assert_complete(run(dir.path(), command, format, &[])?, command, format, 0)?;
    }
    let empty_rows = run(dir.path(), "export", "jsonl", &["--meta", "false"])?;
    assert_eq!(empty_rows.status.code(), Some(0));
    assert!(empty_rows.stdout.is_empty());
    assert!(empty_rows.stderr.is_empty());

    fs::write(dir.path().join("good.rs"), "fn main() {}\n")?;
    for &(command, format) in FORMATS {
        assert_complete(run(dir.path(), command, format, &[])?, command, format, 1)?;
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn unreadable_selected_source_cannot_serialize_complete_or_empty_success() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir()?;
    let blocked = dir.path().join("blocked.rs");
    fs::write(&blocked, "fn blocked() {}\n")?;
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000))?;
    // Privileged Unix users can bypass this fault. Keep the portable success
    // controls and adapter-level inaccurate-flag tests independent of it.
    if fs::File::open(&blocked).is_ok() {
        eprintln!("permission fault unavailable: current user can read mode-000 source");
        return Ok(());
    }

    for mixed in [false, true] {
        if mixed {
            fs::write(dir.path().join("good.rs"), "fn main() {}\n")?;
        }
        for &(command, format) in FORMATS {
            let output = run(dir.path(), command, format, &[])?;
            assert_eq!(
                output.status.code(),
                Some(1),
                "{command}/{format}: {output:?}"
            );
            assert!(
                output.stdout.is_empty(),
                "failed scan emitted a success receipt: {output:?}"
            );
            let stderr = String::from_utf8(output.stderr)?;
            assert!(stderr.contains("Incomplete scan"), "{stderr}");
            assert!(stderr.contains("Rust"), "{stderr}");
            assert!(stderr.contains("read or decoding errors"), "{stderr}");
        }
        for format in ["json", "jsonl"] {
            let destination = dir.path().join("retained-output");
            fs::write(&destination, b"retained artifact\n")?;
            let output = run(
                dir.path(),
                "export",
                format,
                &["--output", "retained-output"],
            )?;
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(output.stdout.is_empty());
            assert_eq!(fs::read(&destination)?, b"retained artifact\n");
        }
        let output = run(dir.path(), "export", "jsonl", &["--meta", "false"])?;
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stdout.is_empty());

        // Ignored input was not selected: its read permissions must not turn a
        // complete scan into an error. This also guards against pre-walking.
        for &(command, format) in FORMATS {
            assert_complete(
                run(dir.path(), command, format, &["--exclude", "blocked.rs"])?,
                command,
                format,
                usize::from(mixed),
            )?;
        }
    }

    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o600))?;
    let output = run(dir.path(), "export", "jsonl", &[])?;
    assert_complete(output, "export", "jsonl", 2)
}
