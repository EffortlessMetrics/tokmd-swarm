//! Rust-owned terminal aggregation for the release preflight workflow.
//!
//! The workflow owns command execution and receipt collection. This module owns
//! identity validation, required-command selection, fail-closed aggregation,
//! and the durable decision receipt.

use std::{collections::BTreeMap, fs};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::cli::ReleasePreflightArgs;

pub const INPUT_SCHEMA: &str = "tokmd.release_preflight_input.v1";
pub const RECEIPT_SCHEMA: &str = "tokmd.release_preflight.v1";
const SCHEMA_VERSION: u32 = 1;

const REQUIRED_COMMANDS: &[&str] = &[
    "affected_plan",
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
    for command in input.commands {
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
    if !validation_errors.is_empty() {
        bail!(
            "release preflight input validation failed: {}",
            validation_errors.join("; ")
        );
    }

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
        source_sha: input.source_sha,
        affected_base_sha: input.affected_base_sha,
        expected_version: input.expected_version,
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
                .map(|id| json!({"id": id, "status": "passed"}))
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
        commands.retain(|command| command["id"] != "clippy");
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
                json!({"id": id, "status": if *id == "gate_check" { "failed" } else { "unavailable" }})
            })
            .collect();
        let receipt = aggregate(input(serde_json::Value::Array(commands))?)?;
        ensure!(receipt.overall == CommandStatus::Failed);
        Ok(())
    }

    #[test]
    fn invalid_identity_is_rejected() -> Result<()> {
        let mut value = serde_json::to_value(input(passed_commands())?)?;
        value["source_sha"] = json!("moving-main");
        let invalid: PreflightInput = serde_json::from_value(value)?;
        let error = match aggregate(invalid) {
            Ok(_) => return Err(anyhow::anyhow!("moving source was accepted")),
            Err(error) => error,
        };
        ensure!(error.to_string().contains("source_sha"));
        Ok(())
    }
}
