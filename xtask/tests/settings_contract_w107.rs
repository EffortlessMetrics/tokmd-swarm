use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> anyhow::Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("xtask manifest has no workspace parent"))
}

fn settings_payload() -> anyhow::Result<String> {
    let path = workspace_root()?.join(".github/settings.yml");
    fs::read_to_string(&path).map_err(|error| {
        anyhow::anyhow!(
            "read repository settings payload {}: {error}",
            normalize_path(&path)
        )
    })
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn main_protection_block(payload: &str) -> Option<String> {
    let normalized = payload.replace("\r\n", "\n");
    let mut in_branches = false;
    let mut in_main = false;
    let mut block = Vec::new();

    for line in normalized.lines() {
        if line == "branches:" {
            in_branches = true;
            continue;
        }
        if !in_branches {
            continue;
        }
        if line.starts_with("  - name: ") {
            if in_main && !block.is_empty() {
                return Some(block.join("\n"));
            }
            in_main = line == "  - name: main";
            continue;
        }
        if in_main {
            if line.starts_with("    protection:") || line.starts_with("      ") {
                block.push(line.to_string());
            } else if !line.is_empty() && !line.starts_with(' ') {
                break;
            }
        }
    }

    in_main.then_some(block.join("\n"))
}

#[derive(Default)]
struct ProtectionContract {
    approvals: Option<u32>,
    conversation_resolution: Option<bool>,
    strict: Option<bool>,
    contexts: Vec<String>,
    enforce_admins: Option<bool>,
    restrictions: Option<String>,
    allow_force_pushes: Option<bool>,
    allow_deletions: Option<bool>,
}

fn parse_main_protection(payload: &str) -> Option<ProtectionContract> {
    let block = main_protection_block(payload)?;
    let mut contract = ProtectionContract::default();
    let mut section = "";
    for raw_line in block.lines() {
        let line = raw_line.trim();
        if line == "required_pull_request_reviews:" {
            section = "reviews";
        } else if line == "required_status_checks:" {
            section = "status";
        } else if let Some(value) = line.strip_prefix("required_approving_review_count:") {
            contract.approvals = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("required_conversation_resolution:") {
            contract.conversation_resolution = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("strict:") {
            contract.strict = value.trim().parse().ok();
        } else if line.starts_with("- ") {
            if section == "status" {
                contract.contexts.push(
                    line[2..]
                        .trim()
                        .trim_matches('\"')
                        .trim_matches('\'')
                        .to_owned(),
                );
            }
        } else if let Some(value) = line.strip_prefix("enforce_admins:") {
            contract.enforce_admins = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("restrictions:") {
            contract.restrictions = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("allow_force_pushes:") {
            contract.allow_force_pushes = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("allow_deletions:") {
            contract.allow_deletions = value.trim().parse().ok();
        } else if !line.is_empty() && !line.starts_with('#') && section == "reviews" {
            section = "";
        }
    }
    Some(contract)
}

fn settings_contract_findings(payload: &str) -> Vec<&'static str> {
    let Some(contract) = parse_main_protection(payload) else {
        return vec!["branches[name=main].protection"];
    };
    let mut missing = Vec::new();
    if contract.approvals != Some(0) {
        missing.push("required_approving_review_count: 0");
    }
    if contract.conversation_resolution != Some(true) {
        missing.push("required_conversation_resolution: true");
    }
    if contract.strict != Some(true) {
        missing.push("required_status_checks.strict: true");
    }
    if contract.contexts != vec!["Tokmd Rust Result".to_owned()] {
        missing.push("required_status_checks.contexts: [Tokmd Rust Result]");
    }
    if contract.enforce_admins != Some(false) {
        missing.push("enforce_admins: false");
    }
    if contract.restrictions.as_deref() != Some("null") {
        missing.push("restrictions: null");
    }
    if contract.allow_force_pushes != Some(false) {
        missing.push("allow_force_pushes: false");
    }
    if contract.allow_deletions != Some(false) {
        missing.push("allow_deletions: false");
    }
    missing
}

fn settings_contract_is_valid(payload: &str) -> bool {
    settings_contract_findings(payload).is_empty()
}

#[test]
fn settings_payload_has_complete_main_protection_contract() -> anyhow::Result<()> {
    let payload = settings_payload()?;

    let missing = settings_contract_findings(&payload);
    if !missing.is_empty() {
        anyhow::bail!("settings payload contract violated; missing: {missing:?}");
    }
    Ok(())
}

#[test]
fn settings_contract_rejects_missing_required_fields_and_drift() -> anyhow::Result<()> {
    let payload = settings_payload()?;

    let payload = payload.replace("\r\n", "\n");
    let missing_restrictions = payload
        .lines()
        .filter(|line| line.trim() != "restrictions: null")
        .collect::<Vec<_>>()
        .join("\n");
    if missing_restrictions == payload {
        anyhow::bail!("missing-restrictions mutation did not change the payload");
    }
    if settings_contract_is_valid(&missing_restrictions) {
        anyhow::bail!("missing restrictions must be rejected");
    }

    let stale_context = payload.replace(
        "          - \"Tokmd Rust Result\"",
        "          - \"Codex Review Gate\"",
    );
    if stale_context == payload {
        anyhow::bail!("stale-context mutation did not change the payload");
    }
    if settings_contract_is_valid(&stale_context) {
        anyhow::bail!("stale required context must be rejected");
    }

    let weakened_resolution = payload.replace(
        "required_conversation_resolution: true",
        "required_conversation_resolution: false",
    );
    if weakened_resolution == payload {
        anyhow::bail!("weakened-resolution mutation did not change the payload");
    }
    if settings_contract_is_valid(&weakened_resolution) {
        anyhow::bail!("weakened conversation resolution must be rejected");
    }

    let weakened_strictness = payload.replace("        strict: true", "        strict: false");
    if weakened_strictness == payload {
        anyhow::bail!("weakened-strictness mutation did not change the payload");
    }
    if settings_contract_is_valid(&weakened_strictness) {
        anyhow::bail!("weakened strictness must be rejected");
    }

    let force_push_enabled =
        payload.replace("allow_force_pushes: false", "allow_force_pushes: true");
    if force_push_enabled == payload {
        anyhow::bail!("force-push mutation did not change the payload");
    }
    if settings_contract_is_valid(&force_push_enabled) {
        anyhow::bail!("force-push drift must be rejected");
    }

    for (name, mutation) in [
        (
            "missing approvals",
            payload.replace(
                "required_approving_review_count: 0",
                "required_approving_review_count: 1",
            ),
        ),
        (
            "admins enforced",
            payload.replace("enforce_admins: false", "enforce_admins: true"),
        ),
        (
            "extra required context",
            payload.replace(
                "          - \"Tokmd Rust Result\"",
                "          - \"Tokmd Rust Result\"\n          - \"Unexpected Context\"",
            ),
        ),
    ] {
        if mutation == payload {
            anyhow::bail!("{name} mutation did not change the payload");
        }
        if settings_contract_is_valid(&mutation) {
            anyhow::bail!("{name} drift must be rejected");
        }
    }

    let renamed_main = payload.replace("  - name: main", "  - name: backup");
    if settings_contract_is_valid(&renamed_main) {
        anyhow::bail!("protection on a non-main branch must be rejected");
    }
    Ok(())
}
