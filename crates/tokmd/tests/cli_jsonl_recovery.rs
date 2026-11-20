#![cfg(feature = "analysis")]

//! JSONL parser failures identify the selected payload and physical record.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

struct Fixture {
    root: TempDir,
    payload: PathBuf,
    meta: Value,
    row: Value,
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        std::fs::write(root.path().join("tokmd.toml"), "")?;
        std::fs::create_dir(root.path().join("src"))?;
        std::fs::write(
            root.path().join("src/lib.rs"),
            "pub fn answer() -> u8 { 42 }\n",
        )?;
        let payload = root
            .path()
            .join("toml timeout rate_limit")
            .join("export's ü.jsonl");
        std::fs::create_dir(payload.parent().context("payload parent")?)?;
        let mut fixture = Self {
            root,
            payload,
            meta: Value::Null,
            row: Value::Null,
        };
        let output = fixture
            .command()
            .args(["export", "src", "--format", "jsonl"])
            .output()?;
        ensure!(output.status.success(), "export setup failed: {output:?}");
        ensure!(output.stderr.is_empty(), "export stderr: {output:?}");
        let records = String::from_utf8(output.stdout)?
            .lines()
            .map(serde_json::from_str)
            .collect::<serde_json::Result<Vec<Value>>>()?;
        ensure!(records.len() == 2, "expected metadata plus one source row");
        fixture.meta = records.first().context("export metadata")?.clone();
        fixture.row = records.get(1).context("export source row")?.clone();
        assert_eq!(fixture.meta.get("type"), Some(&Value::from("meta")));
        assert_eq!(fixture.row.get("code"), Some(&Value::from(1)));
        Ok(fixture)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tokmd"));
        command
            .current_dir(self.root.path())
            .env("TOKMD_CONFIG", self.root.path().join("tokmd.toml"))
            .env("XDG_CONFIG_HOME", self.root.path())
            .env("APPDATA", self.root.path())
            .env("RAYON_NUM_THREADS", "1")
            .env_remove("TOKMD_PROFILE")
            .env_remove("TOKMD_PROGRESS_EVENTS")
            .env_remove("RUST_BACKTRACE")
            .timeout(Duration::from_secs(10))
            .arg("--no-progress");
        command
    }

    fn analyze(&self, directory: bool) -> Command {
        let mut command = self.command();
        let input = if directory {
            self.payload.parent().unwrap_or(&self.payload)
        } else {
            &self.payload
        };
        command
            .arg("analyze")
            .arg(input)
            .args(["--preset", "receipt", "--format", "json"]);
        command
    }

    fn write_valid(&self) -> Result<()> {
        Ok(std::fs::write(
            &self.payload,
            format!("{}\r\n\r\n{}\r\n", self.meta, self.row),
        )?)
    }

    fn journey(&self, malformed: &str, cause: &str, directory: bool) -> Result<()> {
        // Physical line five follows a valid header, row, and empty CRLF lines.
        std::fs::write(
            &self.payload,
            format!("{}\r\n\r\n{}\r\n\r\n{malformed}\r\n", self.meta, self.row),
        )?;
        let output = self.analyze(directory).output()?;
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        let stderr = String::from_utf8(output.stderr)?;
        assert!(
            stderr.contains(&format!(
                "Failed to parse export rows from {}",
                self.payload.display()
            )),
            "missing selected payload: {stderr}"
        );
        assert!(stderr.contains("JSONL line 5"), "{stderr}");
        assert!(stderr.contains(cause), "lost native cause: {stderr}");
        assert!(
            stderr
                .contains("If it was hand-edited or truncated, regenerate the receipt and retry."),
            "missing recovery: {stderr}"
        );
        for unrelated in [
            "Retry-After",
            "network",
            "upstream service",
            "Check TOML syntax",
        ] {
            assert!(!stderr.contains(unrelated), "wrong advice: {stderr}");
        }

        self.write_valid()?;
        let output = self.analyze(directory).output()?;
        assert!(output.status.success(), "retry failed: {output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        // Whole-stream parsing rejects JSON followed by diagnostics or another document.
        let receipt: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(
            receipt.pointer("/derived/totals/files"),
            Some(&Value::from(1))
        );
        assert_eq!(
            receipt.pointer("/derived/totals/code"),
            Some(&Value::from(1))
        );
        assert_eq!(
            receipt.pointer("/source/export_schema_version"),
            Some(&Value::from(2))
        );
        assert_eq!(
            receipt.pointer("/source/children"),
            Some(&Value::from("separate"))
        );
        Ok(())
    }
}

#[test]
fn syntax_error_names_jsonl_source_and_physical_line_then_recovers() -> Result<()> {
    Fixture::new()?.journey("{broken", "key must be a string", false)
}

#[test]
fn typed_row_error_uses_receipt_recovery_despite_provider_words() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut row = fixture.row.clone();
    row.as_object_mut()
        .context("row object")?
        .insert("kind".into(), Value::from("timeout-rate_limit"));
    fixture.journey(&serde_json::to_string(&row)?, "unknown variant", false)
}

#[test]
fn metadata_error_names_jsonl_source_and_physical_line_then_recovers() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut meta = fixture.meta.clone();
    *meta
        .pointer_mut("/args/module_depth")
        .context("metadata depth")? = Value::from("timeout rate_limit");
    fixture.journey(&serde_json::to_string(&meta)?, "expected usize", false)
}

#[test]
fn directory_selection_names_actual_jsonl_payload_then_recovers() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.payload.set_file_name("export.jsonl");
    fixture.journey("{broken", "key must be a string", true)
}
