//! Exercise the compiled tool-schema producer with failed and repaired stdout.

use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tempfile::TempDir;

fn command(root: &TempDir, format: &str, pretty: bool) -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("tokmd"));
    cmd.current_dir(root.path())
        .env("TOKMD_CONFIG", root.path().join("tokmd.toml"))
        .env("XDG_CONFIG_HOME", root.path())
        .env("APPDATA", root.path())
        .env_remove("TOKMD_PROFILE")
        .env_remove("TOKMD_PROGRESS_EVENTS")
        .env_remove("RUST_BACKTRACE")
        .args(["--no-progress", "tools", "--format", format]);
    if pretty {
        cmd.arg("--pretty");
    }
    cmd
}

fn journey(format: &str, pretty: bool) -> Result<()> {
    let root = TempDir::new()?;
    std::fs::write(root.path().join("tokmd.toml"), "")?;

    // Close the reader before spawning, so failure does not depend on output
    // size, pipe capacity, or which process happens to run first.
    let (reader, writer) = std::io::pipe()?;
    drop(reader);
    let failed = command(&root, format, pretty)
        .stdout(writer)
        .stderr(Stdio::piped())
        .output()?;
    assert_eq!(failed.status.code(), Some(1), "{failed:?}");
    let stderr = String::from_utf8(failed.stderr)?;
    assert!(
        stderr.starts_with("Error: Failed to write tool schema to stdout:"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");

    // Retry identical argv with a working consumer. Expectations below are
    // literals, independent of the production schema renderer/serializer.
    let recovered = command(&root, format, pretty).output()?;
    assert_eq!(recovered.status.code(), Some(0), "{recovered:?}");
    assert!(recovered.stderr.is_empty(), "{recovered:?}");
    assert!(recovered.stdout.ends_with(b"}\n"));
    assert!(!recovered.stdout.ends_with(b"\n\n"));
    let text = std::str::from_utf8(&recovered.stdout)?;
    assert_eq!(text.lines().count() > 1, pretty);
    let value: Value = serde_json::from_slice(&recovered.stdout)?;
    let keys: Vec<&str> = value
        .as_object()
        .context("JSON object")?
        .keys()
        .map(String::as_str)
        .collect();
    let (entries, parameters) = match format {
        "openai" => {
            assert_eq!(keys, ["functions"]);
            (value.get("functions").context("functions")?, "parameters")
        }
        "anthropic" => {
            assert_eq!(keys, ["tools"]);
            (value.get("tools").context("tools")?, "input_schema")
        }
        "jsonschema" => {
            assert_eq!(
                keys,
                [
                    "$schema",
                    "description",
                    "name",
                    "schema_version",
                    "tools",
                    "version"
                ]
            );
            assert_eq!(value["$schema"], "https://json-schema.org/draft-07/schema#");
            assert_eq!(value["schema_version"], 1);
            assert_eq!(value["name"], "tokmd");
            (value.get("tools").context("tools")?, "parameters")
        }
        "clap" => {
            assert_eq!(
                keys,
                ["description", "name", "schema_version", "tools", "version"]
            );
            assert_eq!(value["schema_version"], 1);
            assert_eq!(value["name"], "tokmd");
            (value.get("tools").context("tools")?, "parameters")
        }
        _ => anyhow::bail!("test case has an unknown format"),
    };
    let lang = entries
        .as_array()
        .context("tool entries array")?
        .iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some("lang"))
        .context("lang tool")?;
    if format == "clap" {
        assert!(lang[parameters].is_array());
    } else {
        assert_eq!(lang[parameters]["type"], "object");
        assert_eq!(
            lang[parameters]["properties"]["format"]["enum"],
            json!(["md", "tsv", "json"])
        );
    }
    Ok(())
}

macro_rules! journeys {
    ($($name:ident: ($format:literal, $pretty:literal)),+ $(,)?) => {
        $(#[test]
        fn $name() -> Result<()> {
            journey($format, $pretty)
        })+
    };
}

journeys! {
    jsonschema_compact: ("jsonschema", false),
    jsonschema_pretty: ("jsonschema", true),
    openai_compact: ("openai", false),
    openai_pretty: ("openai", true),
    anthropic_compact: ("anthropic", false),
    anthropic_pretty: ("anthropic", true),
    clap_compact: ("clap", false),
    clap_pretty: ("clap", true),
}
