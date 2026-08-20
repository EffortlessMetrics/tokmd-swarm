use std::fs;
use std::path::PathBuf;

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
            path.display()
        )
    })
}

fn settings_contract_is_valid(payload: &str) -> bool {
    let required = [
        "    protection:",
        "      required_pull_request_reviews:",
        "        required_approving_review_count: 0",
        "      required_conversation_resolution: true",
        "      required_status_checks:",
        "        strict: true",
        "          - \"Tokmd Rust Result\"",
        "      enforce_admins: false",
        "      restrictions: null",
        "      allow_force_pushes: false",
        "      allow_deletions: false",
    ];

    required.iter().all(|line| payload.contains(line)) && !payload.contains("Codex Review Gate")
}

#[test]
fn settings_payload_has_complete_main_protection_contract() -> anyhow::Result<()> {
    let payload = settings_payload()?;

    if !settings_contract_is_valid(&payload) {
        anyhow::bail!("settings payload must declare the complete main protection contract");
    }
    Ok(())
}

#[test]
fn settings_contract_rejects_missing_required_fields_and_drift() -> anyhow::Result<()> {
    let payload = settings_payload()?;

    let missing_restrictions = payload.replace("      restrictions: null\n", "");
    if settings_contract_is_valid(&missing_restrictions) {
        anyhow::bail!("missing restrictions must be rejected");
    }

    let stale_context = payload.replace("Tokmd Rust Result", "Codex Review Gate");
    if settings_contract_is_valid(&stale_context) {
        anyhow::bail!("stale required context must be rejected");
    }

    let weakened_resolution = payload.replace(
        "required_conversation_resolution: true",
        "required_conversation_resolution: false",
    );
    if settings_contract_is_valid(&weakened_resolution) {
        anyhow::bail!("weakened conversation resolution must be rejected");
    }

    let weakened_strictness = payload.replace("        strict: true", "        strict: false");
    if settings_contract_is_valid(&weakened_strictness) {
        anyhow::bail!("weakened strictness must be rejected");
    }

    let changed_ruleset_shortcut =
        payload.replace("allow_force_pushes: false", "allow_force_pushes: true");
    if settings_contract_is_valid(&changed_ruleset_shortcut) {
        anyhow::bail!("force-push drift must be rejected");
    }
    Ok(())
}
