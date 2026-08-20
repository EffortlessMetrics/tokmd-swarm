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
    let mut main_indent = None;
    let mut in_protection = false;
    let mut block = Vec::new();

    for line in normalized.lines() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        if trimmed == "branches:" {
            in_branches = true;
            continue;
        }
        if !in_branches {
            continue;
        }
        if trimmed.starts_with("- name: ") && !in_protection {
            in_main = trimmed == "- name: main";
            main_indent = in_main.then_some(indent);
            if !in_main {
                continue;
            }
            continue;
        }
        if in_main && !in_protection && trimmed == "protection:" {
            in_protection = true;
            block.push(trimmed.to_owned());
            continue;
        }
        if in_protection {
            if let Some(branch_indent) = main_indent
                && !trimmed.is_empty()
                && indent <= branch_indent
            {
                break;
            }
            block.push(line.to_owned());
        }
    }

    in_main.then_some(block.join("\n"))
}

fn main_protection_line_mutation(payload: &str, target: &str, replacement: &str) -> String {
    let normalized = payload.replace("\r\n", "\n");
    let mut in_branches = false;
    let mut in_main = false;
    let mut main_indent = None;
    let mut in_protection = false;
    let mut replaced = false;
    let mut output = Vec::new();

    for line in normalized.lines() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        if trimmed == "branches:" {
            in_branches = true;
        } else if in_branches && trimmed.starts_with("- name: ") && !in_protection {
            in_main = trimmed == "- name: main";
            main_indent = in_main.then_some(indent);
        } else if in_main && !in_protection && trimmed == "protection:" {
            in_protection = true;
        } else if in_protection
            && let Some(branch_indent) = main_indent
            && !trimmed.is_empty()
            && indent <= branch_indent
        {
            in_protection = false;
            in_main = false;
        }
        if in_protection && !replaced && trimmed == target {
            let prefix: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            output.push(format!("{prefix}{replacement}"));
            replaced = true;
        } else {
            output.push(line.to_owned());
        }
    }
    output.join("\n")
}

fn main_protection_insert_after(payload: &str, target: &str, inserted: &str) -> String {
    let target_prefix = main_protection_line_prefix(payload, target);
    let mut lines = Vec::new();
    let mut inserted_once = false;
    for line in payload.lines() {
        lines.push(line.to_owned());
        if !inserted_once
            && line.trim() == target
            && let Some(prefix) = target_prefix.as_deref()
        {
            lines.push(format!("{prefix}{inserted}"));
            inserted_once = true;
        }
    }
    lines.join("\n")
}

fn main_protection_line_prefix(payload: &str, target: &str) -> Option<String> {
    let mut in_branches = false;
    let mut in_main = false;
    let mut in_protection = false;
    let mut main_indent = None;
    for line in payload.lines() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        if trimmed == "branches:" {
            in_branches = true;
        } else if in_branches && trimmed.starts_with("- name: ") && !in_protection {
            in_main = trimmed == "- name: main";
            main_indent = in_main.then_some(indent);
        } else if in_main && !in_protection && trimmed == "protection:" {
            in_protection = true;
        } else if in_protection
            && let Some(branch_indent) = main_indent
            && !trimmed.is_empty()
            && indent <= branch_indent
        {
            in_protection = false;
            in_main = false;
        }
        if in_protection && trimmed == target {
            return Some(line.chars().take_while(|c| c.is_whitespace()).collect());
        }
    }
    None
}

#[derive(Default)]
struct ProtectionContract {
    approvals: Option<u32>,
    dismiss_stale_reviews: Option<bool>,
    require_code_owner_reviews: Option<bool>,
    conversation_resolution: Option<bool>,
    strict: Option<bool>,
    contexts: Vec<String>,
    enforce_admins: Option<bool>,
    restrictions: Option<String>,
    allow_force_pushes: Option<bool>,
    allow_deletions: Option<bool>,
    required_linear_history: Option<bool>,
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
        } else if let Some(value) = line.strip_prefix("dismiss_stale_reviews:") {
            contract.dismiss_stale_reviews = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("require_code_owner_reviews:") {
            contract.require_code_owner_reviews = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("required_conversation_resolution:") {
            contract.conversation_resolution = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("strict:") {
            contract.strict = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("contexts:") {
            if section == "status" {
                let values = value.trim().trim_start_matches('[').trim_end_matches(']');
                contract.contexts.extend(
                    values
                        .split(',')
                        .filter(|item| !item.trim().is_empty())
                        .map(|item| item.trim().trim_matches('"').trim_matches('\'').to_owned()),
                );
                section = "status_contexts";
            }
        } else if let Some(value) = line.strip_prefix("- ") {
            if section == "status_contexts" || section == "status" {
                contract.contexts.push(
                    value
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
        } else if let Some(value) = line.strip_prefix("required_linear_history:") {
            contract.required_linear_history = value.trim().parse().ok();
        } else if !line.is_empty()
            && !line.starts_with('#')
            && (section == "reviews" || section == "status_contexts")
        {
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
    if contract.dismiss_stale_reviews != Some(false) {
        missing.push("dismiss_stale_reviews: false");
    }
    if contract.require_code_owner_reviews != Some(false) {
        missing.push("require_code_owner_reviews: false");
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
    if contract.required_linear_history != Some(false) {
        missing.push("required_linear_history: false");
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

    let stale_context = main_protection_line_mutation(
        &payload,
        "- \"Tokmd Rust Result\"",
        "- \"Codex Review Gate\"",
    );
    if stale_context == payload {
        anyhow::bail!("stale-context mutation did not change the payload");
    }
    if settings_contract_is_valid(&stale_context) {
        anyhow::bail!("stale required context must be rejected");
    }

    let weakened_resolution = main_protection_line_mutation(
        &payload,
        "required_conversation_resolution: true",
        "required_conversation_resolution: false",
    );
    if weakened_resolution == payload {
        anyhow::bail!("weakened-resolution mutation did not change the payload");
    }
    if settings_contract_is_valid(&weakened_resolution) {
        anyhow::bail!("weakened conversation resolution must be rejected");
    }

    let weakened_strictness =
        main_protection_line_mutation(&payload, "strict: true", "strict: false");
    if weakened_strictness == payload {
        anyhow::bail!("weakened-strictness mutation did not change the payload");
    }
    if settings_contract_is_valid(&weakened_strictness) {
        anyhow::bail!("weakened strictness must be rejected");
    }

    let force_push_enabled = main_protection_line_mutation(
        &payload,
        "allow_force_pushes: false",
        "allow_force_pushes: true",
    );
    if force_push_enabled == payload {
        anyhow::bail!("force-push mutation did not change the payload");
    }
    if settings_contract_is_valid(&force_push_enabled) {
        anyhow::bail!("force-push drift must be rejected");
    }

    for (name, mutation) in [
        (
            "missing approvals",
            main_protection_line_mutation(
                &payload,
                "required_approving_review_count: 0",
                "required_approving_review_count: 1",
            ),
        ),
        (
            "admins enforced",
            main_protection_line_mutation(
                &payload,
                "enforce_admins: false",
                "enforce_admins: true",
            ),
        ),
        (
            "stale reviews dismissed",
            main_protection_line_mutation(
                &payload,
                "dismiss_stale_reviews: false",
                "dismiss_stale_reviews: true",
            ),
        ),
        (
            "code owner reviews required",
            main_protection_line_mutation(
                &payload,
                "require_code_owner_reviews: false",
                "require_code_owner_reviews: true",
            ),
        ),
        (
            "linear history required",
            main_protection_line_mutation(
                &payload,
                "required_linear_history: false",
                "required_linear_history: true",
            ),
        ),
        (
            "extra required context",
            main_protection_insert_after(
                &payload,
                "- \"Tokmd Rust Result\"",
                "- \"Unexpected Context\"",
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
    if renamed_main == payload {
        anyhow::bail!("renamed_main mutation did not change the payload");
    }
    if settings_contract_is_valid(&renamed_main) {
        anyhow::bail!("protection on a non-main branch must be rejected");
    }
    Ok(())
}
