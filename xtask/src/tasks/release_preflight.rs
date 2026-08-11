//! Rust-owned terminal aggregation for the release preflight workflow.
//!
//! The workflow owns command execution and receipt collection. This module owns
//! identity validation, required-command selection, fail-closed aggregation,
//! and the durable decision receipt.

use std::{collections::BTreeMap, fs};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::cli::ReleasePreflightArgs;

pub const INPUT_SCHEMA: &str = "tokmd.release_preflight_input.v2";
pub const RECEIPT_SCHEMA: &str = "tokmd.release_preflight.v2";
const SCHEMA_VERSION: u32 = 2;

const REQUIRED_COMMANDS: &[&str] = &[
    "affected_plan",
    "proof_plan",
    "fmt_check",
    "gate_check",
    "version_consistency",
    "publish_surface",
    "doc_artifacts",
    "docs_check",
    "proof_policy",
    "no_panic",
    "workspace_tests",
    "clippy",
    "cargo_deny",
    "publish_dry_run",
    "browser_tests",
    "browser_wasm_archive",
];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Passed,
    Failed,
    NotRun,
    Cancelled,
    Unavailable,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CommandResult {
    pub id: String,
    pub status: CommandStatus,
    pub duration_ms: Option<u64>,
    pub log: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct PreflightInput {
    schema: String,
    schema_version: u32,
    source_sha: String,
    affected_base_sha: String,
    expected_version: String,
    release_kind: String,
    commands: Vec<CommandResult>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReleasePreflightReceipt {
    pub schema: String,
    pub schema_version: u32,
    pub source_sha: String,
    pub affected_base_sha: String,
    pub expected_version: String,
    pub release_kind: String,
    pub overall: CommandStatus,
    pub required_commands: Vec<CommandResult>,
}

pub fn run(args: ReleasePreflightArgs) -> Result<()> {
    let input_text = fs::read_to_string(&args.input)
        .with_context(|| format!("read release preflight input {}", args.input.display()))?;
    let input: PreflightInput = serde_json::from_str(&input_text)
        .with_context(|| format!("parse release preflight input {}", args.input.display()))?;
    let receipt = aggregate(input)?;

    if let Some(parent) = args
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create release preflight output {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&receipt).context("serialize release preflight")?;
    fs::write(&args.output, format!("{json}\n"))
        .with_context(|| format!("write release preflight {}", args.output.display()))?;

    println!(
        "release-preflight: {} ({})",
        status_name(&receipt.overall),
        args.output.display()
    );
    if receipt.overall != CommandStatus::Passed {
        bail!("release preflight is {}", status_name(&receipt.overall));
    }
    Ok(())
}

fn aggregate(input: PreflightInput) -> Result<ReleasePreflightReceipt> {
    if input.schema != INPUT_SCHEMA || input.schema_version != SCHEMA_VERSION {
        bail!("release preflight input schema must be `{INPUT_SCHEMA}` version {SCHEMA_VERSION}");
    }
    validate_sha("source_sha", &input.source_sha)?;
    validate_sha("affected_base_sha", &input.affected_base_sha)?;
    if input.expected_version.trim().is_empty() {
        bail!("release preflight expected_version must not be empty");
    }
    if !matches!(input.release_kind.as_str(), "rc" | "stable") {
        bail!("release preflight release_kind must be `rc` or `stable`");
    }

    let mut by_id = BTreeMap::new();
    let mut empty_ids = Vec::new();
    let mut unknown_ids = Vec::new();
    let mut duplicate_ids = Vec::new();
    let mut missing_evidence = Vec::new();
    for mut command in input.commands {
        if command.id.trim().is_empty() {
            empty_ids.push(command.id);
            continue;
        }
        if !REQUIRED_COMMANDS.contains(&command.id.as_str()) {
            unknown_ids.push(command.id);
            continue;
        }
        if by_id.contains_key(&command.id) {
            duplicate_ids.push(command.id);
            continue;
        }
        let mut missing = Vec::new();
        if command.duration_ms.is_none() {
            missing.push("duration_ms");
        }
        if command
            .log
            .as_deref()
            .map(str::trim)
            .is_none_or(str::is_empty)
        {
            missing.push("log");
        }
        if !missing.is_empty() {
            missing_evidence.push(format!(
                "command `{}` is missing {}",
                command.id,
                missing.join(" and ")
            ));
        }
        command.log = command.log.map(|path| normalize_path(&path));
        by_id.insert(command.id.clone(), command);
    }
    empty_ids.sort();
    empty_ids.dedup();
    unknown_ids.sort();
    unknown_ids.dedup();
    duplicate_ids.sort();
    duplicate_ids.dedup();
    let mut validation_errors = Vec::new();
    if !empty_ids.is_empty() {
        validation_errors.push(format!(
            "empty command id(s): {}",
            empty_ids
                .iter()
                .map(|id| format!("`{id}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !unknown_ids.is_empty() {
        validation_errors.push(format!(
            "unknown command(s): {}",
            unknown_ids
                .iter()
                .map(|id| format!("`{id}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !duplicate_ids.is_empty() {
        validation_errors.push(format!(
            "duplicate command(s): {}",
            duplicate_ids
                .iter()
                .map(|id| format!("`{id}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    validation_errors.extend(missing_evidence);
    if !validation_errors.is_empty() {
        bail!(
            "release preflight input validation failed: {}",
            validation_errors.join("; ")
        );
    }

    let source_sha = input.source_sha.to_ascii_lowercase();
    let affected_base_sha = input.affected_base_sha.to_ascii_lowercase();
    let expected_version = input.expected_version.trim().to_owned();

    let mut required_commands = Vec::with_capacity(REQUIRED_COMMANDS.len());
    for id in REQUIRED_COMMANDS {
        let command = match by_id.remove(*id) {
            Some(command) => command,
            None => CommandResult {
                id: (*id).to_string(),
                status: CommandStatus::NotRun,
                duration_ms: None,
                log: None,
                detail: Some("required command result was not provided".to_string()),
            },
        };
        required_commands.push(command);
    }
    let overall = aggregate_status(&required_commands);

    Ok(ReleasePreflightReceipt {
        schema: RECEIPT_SCHEMA.to_string(),
        schema_version: SCHEMA_VERSION,
        source_sha,
        affected_base_sha,
        expected_version,
        release_kind: input.release_kind,
        overall,
        required_commands,
    })
}

fn aggregate_status(commands: &[CommandResult]) -> CommandStatus {
    if commands
        .iter()
        .all(|command| command.status == CommandStatus::Passed)
    {
        return CommandStatus::Passed;
    }
    if commands
        .iter()
        .any(|command| command.status == CommandStatus::Failed)
    {
        return CommandStatus::Failed;
    }
    if commands
        .iter()
        .any(|command| command.status == CommandStatus::Cancelled)
    {
        return CommandStatus::Cancelled;
    }
    if commands
        .iter()
        .any(|command| command.status == CommandStatus::Unavailable)
    {
        return CommandStatus::Unavailable;
    }
    CommandStatus::NotRun
}

fn normalize_path(path: &str) -> String {
    let mut components = Vec::new();
    for component in path.replace('\\', "/").split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.last().is_some_and(|last| *last != "..") {
                    components.pop();
                } else {
                    components.push(component);
                }
            }
            component => components.push(component),
        }
    }
    components.join("/")
}

fn status_name(status: &CommandStatus) -> &'static str {
    match status {
        CommandStatus::Passed => "passed",
        CommandStatus::Failed => "failed",
        CommandStatus::NotRun => "not_run",
        CommandStatus::Cancelled => "cancelled",
        CommandStatus::Unavailable => "unavailable",
    }
}

fn validate_sha(label: &str, value: &str) -> Result<()> {
    if (value.len() == 40 || value.len() == 64)
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Ok(());
    }
    bail!("release preflight {label} must be a 40- or 64-character hexadecimal SHA")
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Result, ensure};
    use serde_json::json;

    fn input(commands: serde_json::Value) -> Result<PreflightInput> {
        Ok(serde_json::from_value(json!({
            "schema": INPUT_SCHEMA,
            "schema_version": SCHEMA_VERSION,
            "source_sha": "0123456789abcdef0123456789abcdef01234567",
            "affected_base_sha": "89abcdef0123456789abcdef0123456789abcdef",
            "expected_version": "1.15.1",
            "release_kind": "stable",
            "commands": commands,
        }))?)
    }

    fn passed_commands() -> serde_json::Value {
        serde_json::Value::Array(
            REQUIRED_COMMANDS
                .iter()
                .map(|id| {
                    json!({
                        "id": id,
                        "status": "passed",
                        "duration_ms": 1,
                        "log": format!("logs\\{id}.log")
                    })
                })
                .collect(),
        )
    }

    #[test]
    fn complete_input_passes_in_stable_order() -> Result<()> {
        let receipt = aggregate(input(passed_commands())?)?;
        ensure!(receipt.overall == CommandStatus::Passed);
        ensure!(receipt.required_commands.len() == REQUIRED_COMMANDS.len());
        ensure!(receipt.required_commands[0].id == "affected_plan");
        Ok(())
    }

    #[test]
    fn missing_required_command_is_not_run_and_fails_closed() -> Result<()> {
        let mut commands = passed_commands()
            .as_array()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("fixture must be an array"))?;
        commands.retain(|command| {
            command.get("id").and_then(serde_json::Value::as_str) != Some("clippy")
        });
        let receipt = aggregate(input(serde_json::Value::Array(commands))?)?;
        let clippy = receipt
            .required_commands
            .iter()
            .find(|command| command.id == "clippy")
            .ok_or_else(|| anyhow::anyhow!("clippy entry missing"))?;
        ensure!(clippy.status == CommandStatus::NotRun);
        ensure!(receipt.overall == CommandStatus::NotRun);
        Ok(())
    }

    #[test]
    fn missing_evidence_is_rejected_and_paths_are_normalized() -> Result<()> {
        let mut commands = passed_commands()
            .as_array()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("fixture must be an array"))?;
        let first = commands
            .get_mut(0)
            .ok_or_else(|| anyhow::anyhow!("passed fixture is empty"))?;
        first
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("command fixture must be an object"))?
            .remove("duration_ms");
        let error = match aggregate(input(serde_json::Value::Array(commands))?) {
            Ok(_) => return Err(anyhow::anyhow!("missing evidence was accepted")),
            Err(error) => error,
        };
        ensure!(error.to_string().contains("duration_ms"));

        let receipt = aggregate(input(passed_commands())?)?;
        let first = receipt
            .required_commands
            .first()
            .ok_or_else(|| anyhow::anyhow!("receipt is empty"))?;
        ensure!(first.log.as_deref() == Some("logs/affected_plan.log"));
        Ok(())
    }

    #[test]
    fn log_paths_normalize_components_and_separators() -> Result<()> {
        let mut commands = passed_commands();
        commands
            .as_array_mut()
            .ok_or_else(|| anyhow::anyhow!("passed fixture is not an array"))?
            .first_mut()
            .ok_or_else(|| anyhow::anyhow!("passed fixture is empty"))?
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("command fixture must be an object"))?
            .insert(
                "log".to_owned(),
                json!(".\\logs//nested/../affected_plan.log"),
            );
        let receipt = aggregate(input(commands)?)?;
        let first = receipt
            .required_commands
            .first()
            .ok_or_else(|| anyhow::anyhow!("receipt is empty"))?;
        ensure!(first.log.as_deref() == Some("logs/affected_plan.log"));
        Ok(())
    }

    #[test]
    fn identity_fields_are_canonicalized() -> Result<()> {
        let mut value = serde_json::to_value(input(passed_commands())?)?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("preflight fixture must be an object"))?;
        object.insert(
            "source_sha".to_owned(),
            json!("ABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD"),
        );
        object.insert(
            "affected_base_sha".to_owned(),
            json!("ABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD"),
        );
        object.insert("expected_version".to_owned(), json!("  1.15.1  "));
        let receipt = aggregate(serde_json::from_value(value)?)?;
        ensure!(receipt.source_sha == "abcdefabcdefabcdefabcdefabcdefabcdefabcd");
        ensure!(receipt.affected_base_sha == "abcdefabcdefabcdefabcdefabcdefabcdefabcd");
        ensure!(receipt.expected_version == "1.15.1");
        ensure!(receipt.schema_version == SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn failure_precedes_unavailable_and_is_not_green() -> Result<()> {
        let mut commands = passed_commands()
            .as_array()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("fixture must be an array"))?;
        commands.push(json!({"id": "not-a-command", "status": "failed"}));
        commands.push(json!({"id": "also-not-a-command", "status": "failed"}));
        commands.push(json!({"id": "clippy", "status": "passed"}));
        commands.push(json!({"id": "", "status": "failed"}));
        let error = match aggregate(input(serde_json::Value::Array(commands))?) {
            Ok(_) => return Err(anyhow::anyhow!("unknown command was accepted")),
            Err(error) => error,
        };
        ensure!(error.to_string().contains("unknown command"));
        ensure!(error.to_string().contains("not-a-command"));
        ensure!(error.to_string().contains("also-not-a-command"));
        ensure!(error.to_string().contains("duplicate command"));
        ensure!(error.to_string().contains("empty command id"));

        let commands = REQUIRED_COMMANDS
            .iter()
            .map(|id| {
                json!({
                    "id": id,
                    "status": if *id == "gate_check" { "failed" } else { "unavailable" },
                    "duration_ms": 1,
                    "log": format!("logs\\{id}.log")
                })
            })
            .collect();
        let receipt = aggregate(input(serde_json::Value::Array(commands))?)?;
        ensure!(receipt.overall == CommandStatus::Failed);
        Ok(())
    }

    #[test]
    fn invalid_identity_is_rejected() -> Result<()> {
        let mut value = serde_json::to_value(input(passed_commands())?)?;
        value
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("preflight fixture must be an object"))?
            .insert("source_sha".to_owned(), json!("moving-main"));
        let invalid: PreflightInput = serde_json::from_value(value)?;
        let error = match aggregate(invalid) {
            Ok(_) => return Err(anyhow::anyhow!("moving source was accepted")),
            Err(error) => error,
        };
        ensure!(error.to_string().contains("source_sha"));
        Ok(())
    }

    #[test]
    fn cancelled_precedes_unavailable() -> Result<()> {
        let command = |status| CommandResult {
            id: "fixture".to_owned(),
            status,
            duration_ms: Some(1),
            log: Some("logs/fixture.log".to_owned()),
            detail: None,
        };
        ensure!(
            aggregate_status(&[
                command(CommandStatus::Cancelled),
                command(CommandStatus::Unavailable),
            ]) == CommandStatus::Cancelled
        );
        ensure!(
            aggregate_status(&[
                command(CommandStatus::Failed),
                command(CommandStatus::Cancelled),
            ]) == CommandStatus::Failed
        );
        Ok(())
    }

    #[test]
    fn invalid_release_kind_and_blank_version_are_rejected() -> Result<()> {
        let mut value = serde_json::to_value(input(passed_commands())?)?;
        {
            let object = value
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("preflight fixture must be an object"))?;
            object.insert("release_kind".to_owned(), json!("release"));
        }
        let invalid_kind: PreflightInput = serde_json::from_value(value.clone())?;
        let kind_error = match aggregate(invalid_kind) {
            Ok(_) => return Err(anyhow::anyhow!("invalid release kind was accepted")),
            Err(error) => error,
        };
        ensure!(kind_error.to_string().contains("release_kind"));

        {
            let object = value
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("preflight fixture must be an object"))?;
            object.insert("release_kind".to_owned(), json!("stable"));
            object.insert("expected_version".to_owned(), json!("   "));
        }
        let blank_version: PreflightInput = serde_json::from_value(value)?;
        let version_error = match aggregate(blank_version) {
            Ok(_) => return Err(anyhow::anyhow!("blank version was accepted")),
            Err(error) => error,
        };
        ensure!(version_error.to_string().contains("expected_version"));
        Ok(())
    }
}
