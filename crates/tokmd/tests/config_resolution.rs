use tokmd::cli::{CliLangArgs, TableFormat as CliTableFormat};
use tokmd::resolve_lang;
use tokmd_settings::Profile;
use tokmd_types::TableFormat;

#[test]
fn test_resolve_lang_no_args_no_profile() {
    let cli = CliLangArgs::default();
    let profile = None;

    let resolved = resolve_lang(&cli, profile);

    // Default fallback values
    assert_eq!(resolved.paths[0].to_string_lossy(), ".");
    assert_eq!(resolved.format, TableFormat::Md);
    assert_eq!(resolved.top, 0);
    assert!(!resolved.files);
}

#[test]
fn test_resolve_lang_cli_overrides_profile() {
    let cli = CliLangArgs {
        top: Some(50),
        format: Some(CliTableFormat::Json),
        ..Default::default()
    };

    let profile = Profile {
        top: Some(10),
        format: Some("csv".to_string()),
        ..Default::default()
    };

    let resolved = resolve_lang(&cli, Some(&profile));

    assert_eq!(resolved.top, 50);
    assert_eq!(resolved.format, TableFormat::Json);
}

#[test]
fn test_resolve_lang_profile_overrides_default() {
    let cli = CliLangArgs::default();

    let profile = Profile {
        top: Some(10),
        format: Some("tsv".to_string()),
        files: Some(true),
        ..Default::default()
    };

    let resolved = resolve_lang(&cli, Some(&profile));

    assert_eq!(resolved.top, 10);
    assert_eq!(resolved.format, TableFormat::Tsv);
    assert!(resolved.files);
}

#[test]
fn test_resolve_lang_partial_overrides() {
    let cli = CliLangArgs {
        files: true, // Override files only
        ..Default::default()
    };

    let profile = Profile {
        top: Some(10),                   // Profile sets top
        format: Some("tsv".to_string()), // Profile sets format
        ..Default::default()
    };

    let resolved = resolve_lang(&cli, Some(&profile));

    assert_eq!(resolved.top, 10); // From profile
    assert_eq!(resolved.format, TableFormat::Tsv); // From profile
    assert!(resolved.files); // From CLI
}

#[test]
fn test_resolve_export_cli_overrides_profile() {
    use tokmd::cli::{CliExportArgs, ExportFormat as CliExportFormat};
    use tokmd::resolve_export;
    use tokmd_types::ExportFormat;

    let cli = CliExportArgs {
        format: Some(CliExportFormat::Csv),
        min_code: Some(50),
        paths: None,
        output: None,
        module_roots: None,
        module_depth: None,
        children: None,
        max_rows: None,
        redact: None,
        meta: None,
        strip_prefix: None,
    };

    let profile = Profile {
        format: Some("json".to_string()),
        min_code: Some(10),
        ..Default::default()
    };

    let resolved = resolve_export(&cli, Some(&profile));

    assert_eq!(resolved.format, ExportFormat::Csv);
    assert_eq!(resolved.min_code, 50);
}

#[test]
fn test_resolve_module_profile_overrides_default() {
    use tokmd::cli::CliModuleArgs;
    use tokmd::resolve_module;

    let cli = CliModuleArgs {
        paths: None,
        format: None,
        top: None,
        module_roots: None,
        module_depth: None,
        children: None,
    };

    let profile = Profile {
        module_depth: Some(5),
        module_roots: Some(vec!["src".to_string()]),
        ..Default::default()
    };

    let resolved = resolve_module(&cli, Some(&profile));

    assert_eq!(resolved.module_depth, 5);
    assert_eq!(resolved.module_roots, vec!["src".to_string()]);
}

#[test]
fn test_resolve_module_cli_overrides_profile_scalars() {
    use tokmd::cli::CliModuleArgs;
    use tokmd::resolve_module;

    let cli = CliModuleArgs {
        paths: None,
        format: Some(CliTableFormat::Tsv),
        top: Some(100),
        module_roots: None,
        module_depth: None,
        children: None,
    };

    let profile = Profile {
        format: Some("json".to_string()),
        top: Some(20),
        ..Default::default()
    };

    let resolved = resolve_module(&cli, Some(&profile));

    assert_eq!(resolved.format, TableFormat::Tsv);
    assert_eq!(resolved.top, 100);
}

#[test]
fn test_resolve_export_with_config() {
    use tokmd::cli::{CliExportArgs, ExportFormat as CliExportFormat};
    use tokmd::{ResolvedConfig, resolve_export_with_config};
    use tokmd_settings::{ExportConfig, TomlConfig};
    use tokmd_types::ExportFormat;

    let cli = CliExportArgs {
        format: Some(CliExportFormat::Csv),
        min_code: None,
        paths: None,
        output: None,
        module_roots: None,
        module_depth: None,
        children: None,
        max_rows: None,
        redact: None,
        meta: None,
        strip_prefix: None,
    };

    let toml = TomlConfig {
        export: ExportConfig {
            min_code: Some(25),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut config = ResolvedConfig::default();
    let toml_ref = &toml;
    config.toml = Some(toml_ref);

    let resolved = resolve_export_with_config(&cli, &config);

    assert_eq!(resolved.format, ExportFormat::Csv);
    assert_eq!(resolved.min_code, 25);
}

#[test]
fn test_resolve_export_profile_overrides_default_format() {
    use tokmd::cli::CliExportArgs;
    use tokmd::resolve_export;
    use tokmd_types::ExportFormat;

    let cli = CliExportArgs {
        paths: None,
        format: None,
        output: None,
        module_roots: None,
        module_depth: None,
        children: None,
        min_code: None,
        max_rows: None,
        redact: None,
        meta: None,
        strip_prefix: None,
    };

    let profile = Profile {
        format: Some("csv".to_string()),
        ..Default::default()
    };

    let resolved = resolve_export(&cli, Some(&profile));

    assert_eq!(resolved.format, ExportFormat::Csv);
}

#[test]
fn test_resolve_module_with_config() {
    use tokmd::cli::CliModuleArgs;
    use tokmd::{ResolvedConfig, resolve_module_with_config};
    use tokmd_settings::{ModuleConfig, TomlConfig};

    let cli = CliModuleArgs {
        paths: None,
        format: None,
        top: None,
        module_roots: None,
        module_depth: None,
        children: None,
    };

    let toml = TomlConfig {
        module: ModuleConfig {
            depth: Some(8),
            roots: Some(vec!["libs".to_string()]),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut config = ResolvedConfig::default();
    let toml_ref = &toml;
    config.toml = Some(toml_ref);

    let resolved = resolve_module_with_config(&cli, &config);

    assert_eq!(resolved.module_depth, 8);
    assert_eq!(resolved.module_roots, vec!["libs".to_string()]);
}

#[test]
fn test_resolve_export_no_args_no_profile() {
    use tokmd::cli::CliExportArgs;
    use tokmd::resolve_export;
    use tokmd_types::ExportFormat;

    let cli = CliExportArgs {
        paths: None,
        format: None,
        output: None,
        module_roots: None,
        module_depth: None,
        children: None,
        min_code: None,
        max_rows: None,
        redact: None,
        meta: None,
        strip_prefix: None,
    };
    let resolved = resolve_export(&cli, None);

    assert_eq!(resolved.paths[0].to_string_lossy(), ".");
    assert_eq!(resolved.format, ExportFormat::Jsonl);
    assert_eq!(
        resolved.module_roots,
        vec!["crates".to_string(), "packages".to_string()]
    );
    assert_eq!(resolved.module_depth, 2);
    assert_eq!(resolved.min_code, 0);
    assert_eq!(resolved.max_rows, 0);
    assert!(resolved.meta);
}

#[test]
fn test_resolve_module_no_args_no_profile() {
    use tokmd::cli::CliModuleArgs;
    use tokmd::resolve_module;

    let cli = CliModuleArgs {
        paths: None,
        format: None,
        top: None,
        module_roots: None,
        module_depth: None,
        children: None,
    };
    let resolved = resolve_module(&cli, None);

    assert_eq!(resolved.paths[0].to_string_lossy(), ".");
    assert_eq!(resolved.format, TableFormat::Md);
    assert_eq!(resolved.top, 0);
    assert_eq!(
        resolved.module_roots,
        vec!["crates".to_string(), "packages".to_string()]
    );
    assert_eq!(resolved.module_depth, 2);
}

#[test]
fn test_resolve_lang_with_config_precedence() {
    use tokmd::cli::{ChildrenMode as CliChildrenMode, CliLangArgs, TableFormat as CliTableFormat};
    use tokmd::{ResolvedConfig, resolve_lang_with_config};
    use tokmd_settings::ViewProfile;
    use tokmd_types::{ChildrenMode, TableFormat};

    let cli = CliLangArgs {
        format: Some(CliTableFormat::Json),
        top: None,
        files: false,
        paths: None,
        children: Some(CliChildrenMode::Separate),
    };

    let view = ViewProfile {
        top: Some(15),
        format: Some("tsv".to_string()),
        files: Some(true),
        children: Some("collapse".to_string()),
        ..Default::default()
    };

    let profile = Profile {
        top: Some(30),
        format: Some("json".to_string()),
        files: Some(false),
        children: Some("separate".to_string()),
        ..Default::default()
    };

    let config = ResolvedConfig {
        toml_view: Some(&view),
        json_profile: Some(&profile),
        toml: None,
        toml_path: None,
    };

    let resolved = resolve_lang_with_config(&cli, &config);

    // CLI format and children mode override TOML view and JSON profile.
    assert_eq!(resolved.format, TableFormat::Json);
    assert_eq!(resolved.children, ChildrenMode::Separate);

    // TOML view supplies defaults ahead of JSON profile when CLI omits a value.
    assert_eq!(resolved.top, 15);

    // TOML view files is true, CLI is false, OR logic results in true.
    assert!(resolved.files);

    let fallback_config = ResolvedConfig {
        toml_view: None,
        json_profile: Some(&profile),
        toml: None,
        toml_path: None,
    };
    let fallback_resolved = resolve_lang_with_config(&CliLangArgs::default(), &fallback_config);

    assert_eq!(fallback_resolved.format, TableFormat::Json);
    assert_eq!(fallback_resolved.top, 30);
    assert!(!fallback_resolved.files);
    assert_eq!(fallback_resolved.children, ChildrenMode::Separate);
}

// Exercise the real CLI on an owned ordinary directory, without
// requiring analysis, UI, Git, or any caller-wide environment changes.
fn first_use_command(
    repo: &std::path::Path,
    selected_config: &std::path::Path,
) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(env!("CARGO_BIN_EXE_tokmd"));
    command
        .current_dir(repo)
        .env("TOKMD_CONFIG", selected_config)
        .env_remove("TOKMD_PROFILE")
        .env_remove("TOKMD_PROGRESS_EVENTS")
        .args(["--no-progress", "--config", "none"]);
    command
}

fn first_use_lang_command(
    repo: &std::path::Path,
    selected_config: &std::path::Path,
    source: &std::path::Path,
    format: &str,
) -> assert_cmd::Command {
    let mut command = first_use_command(repo, selected_config);
    command
        .args(["lang", "--format", format, "--files"])
        .arg(source);
    command
}

fn first_use_json(output: &std::process::Output) -> anyhow::Result<serde_json::Value> {
    first_use_json_with_files(output, true)
}

fn first_use_json_with_files(
    output: &std::process::Output,
    with_files: bool,
) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(
        output.status.code() == Some(0) && output.stderr.is_empty(),
        "ordinary lang failed or emitted stderr: {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: tokmd_types::LangReceipt = serde_json::from_slice(&output.stdout)?;
    anyhow::ensure!(
        receipt.schema_version == tokmd_types::SCHEMA_VERSION
            && receipt.mode == "lang"
            && receipt.status == tokmd_types::ScanStatus::Complete
            && receipt.warnings.is_empty()
            && receipt.args.format == "json"
            && receipt.args.with_files == with_files
            && receipt.report.with_files == with_files
            && receipt.scan.config == tokmd_types::ConfigMode::None,
        "ordinary lang receipt has wrong machine metadata: {receipt:?}"
    );
    let row = receipt
        .report
        .rows
        .first()
        .ok_or_else(|| anyhow::anyhow!("ordinary lang must report the selected Rust source"))?;
    anyhow::ensure!(
        receipt.report.rows.len() == 1
            && row.lang == "Rust"
            && row.code == 2
            && row.lines == 2
            && row.files == 1
            && receipt.report.total.code == 2
            && receipt.report.total.lines == 2
            && receipt.report.total.files == 1,
        "ordinary lang did not inventory the two-line Rust fixture: {receipt:?}"
    );
    let mut semantic: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let object = semantic
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("ordinary lang receipt must be a JSON object"))?;
    anyhow::ensure!(
        object
            .remove("generated_at_ms")
            .is_some_and(|value| value.is_number()),
        "ordinary lang receipt must have a numeric generated_at_ms"
    );
    Ok(semantic)
}

fn first_use_failure(
    output: &std::process::Output,
    expected_hints: &[&str],
) -> anyhow::Result<String> {
    let stderr = std::str::from_utf8(&output.stderr)?;
    anyhow::ensure!(
        output.status.code() == Some(1) && output.stdout.is_empty(),
        "CLI failure must return code1 with empty stdout: {}: {stderr}",
        output.status
    );
    let hints = stderr
        .lines()
        .filter(|line| line.starts_with("- "))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        hints.as_slice() == expected_hints,
        "wrong entire ordinary lang recovery vector: {hints:?}"
    );
    Ok(stderr.to_string())
}

// Only Linux's pinned config-directory lookup honors XDG_CONFIG_HOME.
// Other platforms use constructed ConfigContext controls in config.rs.
#[cfg(target_os = "linux")]
fn profile_selection_command(
    repo: &std::path::Path,
    selected_config: &std::path::Path,
    user_config: &std::path::Path,
) -> assert_cmd::Command {
    let mut command = first_use_command(repo, selected_config);
    command
        .env("XDG_CONFIG_HOME", user_config)
        .timeout(std::time::Duration::from_secs(30));
    command
}

#[cfg(target_os = "linux")]
fn profile_selection_failure(
    output: &std::process::Output,
    name: &str,
    available: &str,
) -> anyhow::Result<()> {
    let stderr = std::str::from_utf8(&output.stderr)?;
    anyhow::ensure!(
        output.status.code() == Some(2) && output.stdout.is_empty(),
        "unmatched profile must fail before receipt output: {}: stdout={:?}, stderr={stderr}",
        output.status,
        String::from_utf8_lossy(&output.stdout)
    );
    let expected = format!(
        "Error: Unknown profile {name:?}. Available profiles: {available}. Use `tokmd --show-config` to inspect configuration sources, then select or define the profile.\n"
    );
    anyhow::ensure!(
        stderr == expected,
        "wrong entire unmatched-profile diagnostic: {stderr:?}"
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn legacy_failure_fixture(malformed: &str, diagnostic: bool) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let repo = dir.path().join("repo");
    let user_config = dir.path().join("rate_limit").join("timeout base ref");
    let legacy = user_config.join("tokmd").join("config.json");
    let selected_config = dir.path().join("selected.toml");
    std::fs::create_dir(&repo)?;
    std::fs::create_dir_all(
        legacy
            .parent()
            .ok_or_else(|| anyhow::anyhow!("legacy parent"))?,
    )?;
    std::fs::write(&selected_config, "")?;
    let source = repo.join("sample.rs");
    std::fs::write(
        &source,
        "pub fn first_use_one() {}\npub fn first_use_two() {}\n",
    )?;
    let repaired = r#"{"profiles":{"ci":{"format":"json","files":true}},"repos":{}}"#;
    std::fs::write(&legacy, repaired)?;
    let command = || profile_selection_command(&repo, &selected_config, &user_config);
    let run = || {
        command()
            .args(["--profile", "ci", "lang"])
            .arg(&source)
            .output()
    };
    let original = first_use_json(&run()?)?;
    std::fs::write(&legacy, malformed)?;
    if diagnostic {
        let output = command()
            .args(["--profile", "ci", "--show-config", "does-not-exist"])
            .output()?;
        anyhow::ensure!(
            output.status.code() == Some(0) && output.stderr.is_empty(),
            "legacy diagnostics must succeed without scanning: {output:?}"
        );
        let report = std::str::from_utf8(&output.stdout)?;
        anyhow::ensure!(
            report.contains(legacy.to_string_lossy().as_ref())
                && report.to_ascii_lowercase().contains("failed")
                && report.contains("line")
                && report.contains("column"),
            "diagnostics must identify failed legacy source and parser cause: {report}"
        );
    } else {
        let output = run()?;
        anyhow::ensure!(
            output.status.code() == Some(2) && output.stdout.is_empty(),
            "damaged selected legacy config must fail before machine output: {output:?}"
        );
        let stderr = std::str::from_utf8(&output.stderr)?;
        anyhow::ensure!(
            stderr.contains(legacy.to_string_lossy().as_ref())
                && stderr.contains("JSON")
                && stderr.contains("line")
                && stderr.contains("column")
                && stderr.to_ascii_lowercase().contains("repair"),
            "selected legacy failure must name its source, cause and repair: {stderr}"
        );
        anyhow::ensure!(
            !stderr.contains("tokmd JSON receipt")
                && !stderr.contains("git fetch")
                && !stderr.contains("Retry after a short delay")
                && !stderr.contains("rate limit"),
            "local config failure gained unrelated recovery advice: {stderr}"
        );
    }
    std::fs::write(&legacy, repaired)?;
    anyhow::ensure!(
        first_use_json(&run()?)? == original && first_use_json(&run()?)? == original,
        "legacy repair did not restore identical-argv semantic JSON"
    );
    Ok(())
}

#[test]
#[cfg(target_os = "linux")]
fn legacy_failure_selected_syntax_names_source_before_retry() -> anyhow::Result<()> {
    legacy_failure_fixture(r#"{"profiles":{"ci":"#, false)
}

#[test]
#[cfg(target_os = "linux")]
fn legacy_failure_selected_shape_names_source_before_retry() -> anyhow::Result<()> {
    legacy_failure_fixture(r#"{"profiles":[],"repos":{}}"#, false)
}

#[test]
#[cfg(target_os = "linux")]
fn legacy_failure_diagnostic_syntax_names_source_before_retry() -> anyhow::Result<()> {
    legacy_failure_fixture(r#"{"profiles":{"ci":"#, true)
}

#[test]
#[cfg(target_os = "linux")]
fn legacy_failure_diagnostic_shape_names_source_before_retry() -> anyhow::Result<()> {
    legacy_failure_fixture(r#"{"profiles":[],"repos":{}}"#, true)
}

#[test]
#[cfg(target_os = "linux")]
fn legacy_failure_preserves_success_and_toml_errors() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let repo = dir.path().join("repo");
    let user_config = dir.path().join("user-config");
    let legacy = user_config.join("tokmd/config.json");
    let selected = dir.path().join("selected.toml");
    std::fs::create_dir(&repo)?;
    std::fs::create_dir_all(
        legacy
            .parent()
            .ok_or_else(|| anyhow::anyhow!("legacy parent"))?,
    )?;
    let source = repo.join("sample.rs");
    std::fs::write(
        &source,
        "pub fn first_use_one() {}\npub fn first_use_two() {}\n",
    )?;
    std::fs::write(&selected, "[view.ci]\n")?;
    let command = || profile_selection_command(&repo, &selected, &user_config);
    let default = || {
        command()
            .args(["lang", "--format", "json"])
            .arg(&source)
            .output()
    };
    let matching = || {
        command()
            .args(["--profile", "ci", "lang", "--format", "json"])
            .arg(&source)
            .output()
    };
    let original = first_use_json_with_files(&default()?, false)?;
    anyhow::ensure!(
        first_use_json_with_files(&matching()?, false)? == original,
        "empty TOML view changed defaults"
    );
    std::fs::write(&legacy, r#"{"profiles":[]}"#)?;
    anyhow::ensure!(
        first_use_json_with_files(&default()?, false)? == original
            && first_use_json_with_files(&matching()?, false)? == original,
        "malformed legacy JSON changed successful defaults or empty TOML view"
    );
    for selector in ["--profile", "--view"] {
        let output = command()
            .args([selector, "unknown", "lang", "--format", "json"])
            .arg(&source)
            .output()?;
        anyhow::ensure!(
            output.status.code() == Some(2)
                && output.stdout.is_empty()
                && String::from_utf8_lossy(&output.stderr)
                    .contains(legacy.to_string_lossy().as_ref()),
            "selector alias lost legacy failure: {output:?}"
        );
    }
    let output = command()
        .env("TOKMD_PROFILE", "unknown")
        .args(["lang", "--format", "json"])
        .arg(&source)
        .output()?;
    anyhow::ensure!(
        output.status.code() == Some(2)
            && output.stdout.is_empty()
            && String::from_utf8_lossy(&output.stderr).contains(legacy.to_string_lossy().as_ref()),
        "environment selector lost legacy failure: {output:?}"
    );
    std::fs::write(
        &legacy,
        r#"{"profiles":{"ci":{"format":"md","files":true}},"repos":{}}"#,
    )?;
    std::fs::write(&selected, "[view.ci]\nformat = \"json\"\nfiles = false\n")?;
    let output = command()
        .args(["--profile", "ci", "lang"])
        .arg(&source)
        .output()?;
    anyhow::ensure!(
        first_use_json_with_files(&output, false)? == original,
        "TOML precedence changed after legacy repair"
    );
    std::fs::write(&legacy, "{")?;
    std::fs::write(&selected, "[view.ci\n")?;
    for diagnostic in [false, true] {
        let mut cmd = command();
        cmd.args(["--profile", "ci"]);
        if diagnostic {
            cmd.arg("--show-config");
        }
        let output = cmd
            .args(["lang", "--format", "json"])
            .arg(&source)
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::ensure!(
            output.status.code() == Some(1)
                && output.stdout.is_empty()
                && stderr.contains("TOML")
                && stderr.contains(selected.to_string_lossy().as_ref())
                && !stderr.contains(legacy.to_string_lossy().as_ref()),
            "TOML failure precedence changed: {output:?}"
        );
    }
    Ok(())
}

#[test]
#[cfg(target_os = "linux")]
fn profile_selection_unknown_cli_recovers_with_same_argv() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let repo = dir.path().join("repo");
    let user_config = dir.path().join("user-config");
    let selected_config = dir.path().join("selected.toml");
    std::fs::create_dir(&repo)?;
    std::fs::create_dir(&user_config)?;
    let source = repo.join("sample.rs");
    std::fs::write(
        &source,
        "pub fn first_use_one() {}\npub fn first_use_two() {}\n",
    )?;
    std::fs::write(
        &selected_config,
        "[view.ci]\nformat = \"json\"\nfiles = true\n",
    )?;
    let command = || profile_selection_command(&repo, &selected_config, &user_config);

    let original = first_use_json(
        &command()
            .args(["lang", "--format", "json", "--files"])
            .arg(&source)
            .output()?,
    )?;
    let selected = first_use_json(
        &command()
            .args(["--profile", "ci", "lang"])
            .arg(&source)
            .output()?,
    )?;
    anyhow::ensure!(selected == original, "valid profile changed the receipt");

    let retry = |explicit_json| {
        let mut selected = command();
        selected.args(["--profile", "c1", "lang"]).arg(&source);
        if explicit_json {
            selected.args(["--format", "json", "--files"]);
        }
        selected.output()
    };
    for explicit_json in [false, true] {
        profile_selection_failure(&retry(explicit_json)?, "c1", "\"ci\"")?;
    }
    std::fs::write(
        &selected_config,
        "[view.ci]\nformat = \"json\"\nfiles = true\n[view.c1]\nformat = \"json\"\nfiles = true\n",
    )?;
    for explicit_json in [false, true] {
        anyhow::ensure!(
            first_use_json(&retry(explicit_json)?)? == original
                && first_use_json(&retry(explicit_json)?)? == original,
            "same-argv profile repair did not restore repeatable JSON"
        );
    }
    Ok(())
}

#[test]
#[cfg(target_os = "linux")]
fn profile_selection_unknown_env_preserves_cli_override_and_diagnostics() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let selected_config = dir.path().join("selected.toml");
    let user_config = dir.path().join("user-config");
    std::fs::create_dir(&user_config)?;
    let source = dir.path().join("sample.rs");
    std::fs::write(
        &source,
        "pub fn first_use_one() {}\npub fn first_use_two() {}\n",
    )?;
    std::fs::write(&selected_config, "")?;
    let command = || {
        let mut command = profile_selection_command(dir.path(), &selected_config, &user_config);
        command.env("TOKMD_PROFILE", "missing");
        command
    };
    let run = || {
        command()
            .args(["lang", "--format", "json", "--files"])
            .arg(&source)
            .output()
    };
    profile_selection_failure(&run()?, "missing", "(none)")?;

    let diagnostic = command().arg("--show-config").arg("not-a-path").output()?;
    anyhow::ensure!(
        diagnostic.status.success() && diagnostic.stderr.is_empty(),
        "unmatched-profile diagnostics failed: {diagnostic:?}"
    );
    let report = std::str::from_utf8(&diagnostic.stdout)?;
    anyhow::ensure!(
        report.contains("missing (from TOKMD_PROFILE)")
            && report.contains("did not match any TOML view or JSON profile"),
        "diagnostics lost unmatched environment selection: {report}"
    );

    // A defined empty view is a match; CLI selection overrides the stale env.
    std::fs::write(&selected_config, "[view.ci]\n")?;
    first_use_json(
        &command()
            .args(["--view", "ci", "lang", "--format", "json", "--files"])
            .arg(&source)
            .output()?,
    )?;
    std::fs::write(&selected_config, "[view.missing]\n")?;
    first_use_json(&run()?)?;
    Ok(())
}

#[test]
#[cfg(target_os = "linux")]
fn profile_selection_legacy_match_sorted_names_and_local_guidance() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let selected_config = dir.path().join("selected.toml");
    let user_config = dir.path().join("user-config");
    std::fs::create_dir_all(user_config.join("tokmd"))?;
    let source = dir.path().join("sample.rs");
    std::fs::write(
        &source,
        "pub fn first_use_one() {}\npub fn first_use_two() {}\n",
    )?;
    std::fs::write(
        &selected_config,
        "[view.shared]\nformat = \"json\"\nfiles = true\n[view.zebra]\n",
    )?;
    std::fs::write(
        user_config.join("tokmd/config.json"),
        r#"{"profiles":{"alpha":{"format":"json","files":true},"shared":{"format":"md","files":false}},"repos":{}}"#,
    )?;
    let command = || profile_selection_command(dir.path(), &selected_config, &user_config);
    for name in ["alpha", "shared"] {
        first_use_json(
            &command()
                .args(["--profile", name, "lang"])
                .arg(&source)
                .output()?,
        )?;
    }
    let marker_name = "rate_limit/timeout base ref";
    profile_selection_failure(
        &command()
            .args(["--profile", marker_name, "lang", "--format", "json"])
            .arg(&source)
            .output()?,
        marker_name,
        "\"alpha\", \"shared\", \"zebra\"",
    )?;
    Ok(())
}

#[test]
fn ordinary_non_git_json_journey_recovers_config_and_source() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let repo = dir.path().join("ordinary-repo");
    std::fs::create_dir(&repo)?;
    let selected_config = dir.path().join("selected.toml");
    let source = repo.join("sample.rs");
    let content = "pub fn first_use_one() {}\npub fn first_use_two() {}\n";
    std::fs::write(&selected_config, "")?;
    std::fs::write(&source, content)?;
    anyhow::ensure!(
        source.is_absolute() && !repo.join(".git").exists(),
        "fixture must be an ordinary non-Git directory with an absolute source path"
    );

    let run = || first_use_lang_command(&repo, &selected_config, &source, "json").output();
    let original = first_use_json(&run()?)?;
    anyhow::ensure!(
        first_use_json(&run()?)? == original,
        "repeated ordinary lang receipt changed beyond its timestamp"
    );

    std::fs::write(&selected_config, "[scan\n")?;
    let malformed = first_use_failure(
        &run()?,
        &["- Check TOML syntax and key names in the file named above, then retry."],
    )?;
    anyhow::ensure!(
        malformed.starts_with(&format!(
            "Error: Failed to load TOML config from {}",
            selected_config.display()
        )),
        "malformed selected config lost its primary path: {malformed}"
    );
    std::fs::write(&selected_config, "")?;
    anyhow::ensure!(
        first_use_json(&run()?)? == original,
        "rewritten selected config did not restore the same-argv receipt"
    );

    anyhow::ensure!(
        std::fs::read_to_string(&source)? == content,
        "owned source fixture changed before removal"
    );
    std::fs::remove_file(&source)?;
    let missing = first_use_failure(
        &run()?,
        &[
            "- Verify the input path exists and is readable.",
            "- Use an absolute path to avoid working-directory confusion.",
        ],
    )?;
    anyhow::ensure!(
        missing.starts_with(&format!("Error: Path not found: {}", source.display())),
        "missing positional source lost its primary path: {missing}"
    );
    std::fs::write(&source, content)?;
    anyhow::ensure!(
        first_use_json(&run()?)? == original && first_use_json(&run()?)? == original,
        "recreated positional source did not restore repeated same-argv receipts"
    );
    Ok(())
}

#[test]
fn ordinary_lang_invalid_format_returns_argument_exit() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let selected_config = dir.path().join("selected.toml");
    std::fs::write(&selected_config, "")?;
    let output = first_use_lang_command(
        dir.path(),
        &selected_config,
        &dir.path().join("sample.rs"),
        "invalid_format",
    )
    .output()?;
    let stderr = std::str::from_utf8(&output.stderr)?;
    anyhow::ensure!(
        output.status.code() == Some(2)
            && output.stdout.is_empty()
            && stderr.contains("invalid value 'invalid_format'"),
        "invalid lang format must fail during argument parsing with code2: {}: {stderr}",
        output.status
    );
    Ok(())
}

#[test]
fn selected_profile_lang_format_recovers_by_override_or_config_repair() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let repo = dir.path().join("ordinary-repo");
    std::fs::create_dir(&repo)?;
    let config_dir = dir.path().join("rate_limit").join("timeout");
    std::fs::create_dir_all(&config_dir)?;
    let selected_config = config_dir.join("selected.toml");
    let source = repo.join("sample.rs");
    std::fs::write(
        &source,
        "pub fn first_use_one() {}\npub fn first_use_two() {}\n",
    )?;
    std::fs::write(&selected_config, "[view.ci]\nformat = \"jsno\"\n")?;

    let run = |override_format: Option<&str>| -> anyhow::Result<std::process::Output> {
        let mut command = first_use_command(&repo, &selected_config);
        command.args(["--profile", "ci", "lang"]);
        if let Some(format) = override_format {
            command.args(["--format", format]);
        }
        command.arg("--files").arg(&source);
        Ok(command.output()?)
    };

    let invalid = run(None)?;
    let stderr = std::str::from_utf8(&invalid.stderr)?;
    let selected_path = selected_config.display().to_string();
    let hints = stderr
        .lines()
        .filter(|line| line.starts_with("- "))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        invalid.status.code() == Some(2)
            && invalid.stdout.is_empty()
            && stderr.contains(selected_path.as_str())
            && stderr.contains("format")
            && stderr.contains("jsno")
            && stderr.contains("md, tsv, json")
            && stderr.contains("--format")
            && hints.is_empty(),
        "invalid selected lang format must fail with local repair guidance and no remote hints: {}: {stderr}; hints={hints:?}",
        invalid.status
    );

    let overridden = first_use_json(&run(Some("json"))?)?;
    std::fs::write(&selected_config, "[view.ci]\nformat = \"json\"\n")?;
    anyhow::ensure!(
        first_use_json(&run(None)?)? == overridden && first_use_json(&run(None)?)? == overridden,
        "profile repair did not restore repeatable same-argv JSON receipts"
    );
    Ok(())
}

#[cfg(not(feature = "analysis"))]
#[test]
fn analysis_commands_without_feature_preserve_error() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let selected_config = dir.path().join("selected.toml");
    std::fs::write(&selected_config, "")?;
    let bundle = dir.path().join("absent-bundle");
    let output = first_use_command(dir.path(), &selected_config)
        .args(["render", "--from-packets"])
        .arg(&bundle)
        .args(["--preset", "bun-ub-handoff"])
        .output()?;
    let stderr = first_use_failure(&output, &[])?;
    anyhow::ensure!(
        stderr == "Error: analysis feature is not enabled\n" && !bundle.exists(),
        "disabled Render must retain the feature fallback without reading a bundle: {stderr}"
    );

    let source = dir.path().join("absent-source.rs");
    let output_dir = dir.path().join("absent-run-output");
    anyhow::ensure!(
        !source.exists() && !output_dir.exists(),
        "disabled Run input and output must begin absent"
    );
    let output = first_use_command(dir.path(), &selected_config)
        .args(["run", "--output-dir"])
        .arg(&output_dir)
        .arg(&source)
        .output()?;
    let stderr = first_use_failure(&output, &[])?;
    anyhow::ensure!(
        stderr == "Error: analysis feature is not enabled\n"
            && !source.exists()
            && !output_dir.exists(),
        "disabled Run must retain the feature fallback without creating artifacts: {stderr}"
    );
    Ok(())
}

#[test]
fn ordinary_export_missing_parent_reports_output_and_recovers() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let selected_config = dir.path().join("selected.toml");
    let source = dir.path().join("sample.rs");
    std::fs::write(&selected_config, "")?;
    std::fs::write(&source, "pub fn export_fixture() {}\n")?;

    // These names must never turn a local output failure into provider advice.
    let parent = dir.path().join("rate_limit").join("timeout");
    let output_path = parent.join("inventory.json");
    anyhow::ensure!(
        !parent.exists(),
        "missing output parent fixture already exists"
    );
    let run = || {
        let mut command = first_use_command(dir.path(), &selected_config);
        command
            .args(["export", "--format", "json", "--output"])
            .arg(&output_path)
            .arg(&source);
        command.output()
    };

    let failure = run()?;
    let stderr = std::str::from_utf8(&failure.stderr)?;
    anyhow::ensure!(
        failure.status.code() == Some(1) && failure.stdout.is_empty() && !output_path.exists(),
        "missing-parent export must fail without output: {}: {stderr}",
        failure.status
    );
    anyhow::ensure!(
        stderr.starts_with(&format!(
            "Error: Failed to create output file {}",
            output_path.display()
        )),
        "export error omitted the selected output path: {stderr}"
    );
    let hints = stderr
        .lines()
        .filter(|line| line.starts_with("- "))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        hints == ["- Create the parent directory for the output path named above, then retry."],
        "export output failure gave wrong recovery hints: {hints:?}"
    );

    std::fs::create_dir_all(&parent)?;
    let success = run()?;
    anyhow::ensure!(
        success.status.code() == Some(0) && success.stdout.is_empty() && success.stderr.is_empty(),
        "same-argv export retry failed or emitted console output: {}: {}",
        success.status,
        String::from_utf8_lossy(&success.stderr)
    );
    let receipt: tokmd_types::ExportReceipt =
        serde_json::from_slice(&std::fs::read(&output_path)?)?;
    anyhow::ensure!(
        receipt.mode == "export"
            && receipt.status == tokmd_types::ScanStatus::Complete
            && receipt.data.rows.len() == 1,
        "recovered export file has wrong receipt content: {receipt:?}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn ordinary_export_nonfile_output_paths_keep_local_recovery() -> anyhow::Result<()> {
    for parent_is_file in [false, true] {
        let dir = tempfile::tempdir()?;
        let selected_config = dir.path().join("selected.toml");
        let source = dir.path().join("sample.rs");
        std::fs::write(&selected_config, "")?;
        std::fs::write(&source, "pub fn export_fixture() {}\n")?;

        let rate_limit = dir.path().join("rate_limit");
        let parent = rate_limit.join("timeout");
        let output_path = parent.join("inventory.json");
        std::fs::create_dir(&rate_limit)?;
        if parent_is_file {
            std::fs::write(&parent, "occupied")?;
        } else {
            std::fs::create_dir(&parent)?;
            std::fs::create_dir(&output_path)?;
        }

        let run = || {
            let mut command = first_use_command(dir.path(), &selected_config);
            command
                .args(["export", "--format", "json", "--output"])
                .arg(&output_path)
                .arg(&source);
            command.output()
        };
        let failure = run()?;
        let stderr = std::str::from_utf8(&failure.stderr)?;
        anyhow::ensure!(
            failure.status.code() == Some(1) && failure.stdout.is_empty(),
            "non-file output must fail without stdout: {}: {stderr}",
            failure.status
        );
        anyhow::ensure!(
            stderr.starts_with(&format!(
                "Error: Failed to create output file {}",
                output_path.display()
            )),
            "non-file output error omitted selected path: {stderr}"
        );
        let expected_hint = if parent_is_file {
            "- Replace the non-directory output parent with a directory, then retry."
        } else {
            "- The output path is a directory. Select a file path, then retry."
        };
        let hints = stderr
            .lines()
            .filter(|line| line.starts_with("- "))
            .collect::<Vec<_>>();
        anyhow::ensure!(
            hints == [expected_hint],
            "non-file output received wrong recovery hints: {hints:?}: {stderr}"
        );

        if parent_is_file {
            anyhow::ensure!(
                std::fs::read_to_string(&parent)? == "occupied",
                "owned parent-file fixture changed"
            );
            std::fs::remove_file(&parent)?;
            std::fs::create_dir(&parent)?;
        } else {
            anyhow::ensure!(
                output_path.is_dir() && std::fs::read_dir(&output_path)?.next().is_none(),
                "owned output-directory fixture changed"
            );
            std::fs::remove_dir(&output_path)?;
        }
        let success = run()?;
        anyhow::ensure!(
            success.status.code() == Some(0)
                && success.stdout.is_empty()
                && success.stderr.is_empty(),
            "same-argv non-file repair failed: {}: {}",
            success.status,
            String::from_utf8_lossy(&success.stderr)
        );
        let receipt: tokmd_types::ExportReceipt =
            serde_json::from_slice(&std::fs::read(&output_path)?)?;
        anyhow::ensure!(
            receipt.mode == "export"
                && receipt.status == tokmd_types::ScanStatus::Complete
                && receipt.data.rows.len() == 1,
            "non-file repair did not write export JSON receipt: {receipt:?}"
        );
    }
    Ok(())
}

#[test]
fn typed_export_output_timeout_stays_transient_without_filename_advice() -> anyhow::Result<()> {
    let context = "Failed to create output file rate_limit/timeout/inventory.json";
    for (kind, expected) in [
        (
            std::io::ErrorKind::TimedOut,
            vec![
                "- This looks transient. Retry with backoff after network or service health recovers.",
                "- Check network, VPN, or proxy settings if retries keep failing.",
            ],
        ),
        (std::io::ErrorKind::StorageFull, vec![]),
    ] {
        let error =
            anyhow::Error::new(std::io::Error::new(kind, "operation impossible")).context(context);
        let rendered = tokmd::format_error(&error);
        let hints = rendered
            .lines()
            .filter(|line| line.starts_with("- "))
            .collect::<Vec<_>>();
        anyhow::ensure!(
            hints == expected,
            "wrong typed export output recovery for {kind:?}: {hints:?}"
        );
    }
    Ok(())
}

#[test]
fn typed_export_output_connection_causes_keep_transient_recovery() -> anyhow::Result<()> {
    let context = "Failed to create output file rate_limit/timeout/inventory.json";
    for native in [
        std::io::Error::from(std::io::ErrorKind::ConnectionReset),
        std::io::Error::from(std::io::ErrorKind::ConnectionRefused),
        std::io::Error::from(std::io::ErrorKind::BrokenPipe),
        std::io::Error::other("network error"),
    ] {
        let kind = native.kind();
        let rendered = tokmd::format_error(&anyhow::Error::new(native).context(context));
        let hints = rendered
            .lines()
            .filter(|line| line.starts_with("- "))
            .collect::<Vec<_>>();
        anyhow::ensure!(
            hints
                == [
                    "- This looks transient. Retry with backoff after network or service health recovers.",
                    "- Check network, VPN, or proxy settings if retries keep failing.",
                ],
            "genuine output-creation connection cause {kind:?} lost transient recovery: {hints:?}"
        );
    }
    Ok(())
}

#[test]
fn ordinary_handoff_file_output_reports_directory_repair_and_recovers() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let selected_config = dir.path().join("selected.toml");
    let source = dir.path().join("sample.rs");
    std::fs::write(&selected_config, "")?;
    std::fs::write(&source, "pub fn handoff_fixture() {}\n")?;

    // A local pathname must not become provider advice.
    let parent = dir.path().join("rate_limit").join("timeout");
    std::fs::create_dir_all(&parent)?;
    let output_path = parent.join("handoff");
    std::fs::write(&output_path, "occupied")?;
    let run = |force: bool| {
        let mut command = first_use_command(dir.path(), &selected_config);
        command.args(["handoff", "--no-git", "--preset", "minimal", "--output-dir"]);
        command.arg(&output_path).arg(&source);
        if force {
            command.arg("--force");
        }
        command.output()
    };

    for force in [false, true] {
        let failure = run(force)?;
        let stderr = std::str::from_utf8(&failure.stderr)?;
        anyhow::ensure!(
            failure.status.code() == Some(1) && failure.stdout.is_empty(),
            "file-valued handoff output must fail without stdout: {}: {stderr}",
            failure.status
        );
        anyhow::ensure!(
            stderr.starts_with(&format!(
                "Error: Handoff output path is a file: {}",
                output_path.display()
            )),
            "handoff error omitted the file-valued output path: {stderr}"
        );
        let hints = stderr
            .lines()
            .filter(|line| line.starts_with("- "))
            .collect::<Vec<_>>();
        anyhow::ensure!(
            hints == ["- Select a directory path for handoff output, then retry."],
            "handoff file output gave wrong recovery hints: {hints:?}: {stderr}"
        );
        anyhow::ensure!(
            std::fs::read_to_string(&output_path)? == "occupied",
            "file-valued handoff output was changed"
        );
    }

    std::fs::remove_file(&output_path)?;
    std::fs::create_dir(&output_path)?;
    let success = run(false)?;
    anyhow::ensure!(
        success.status.code() == Some(0) && success.stdout.is_empty(),
        "same-argv handoff retry failed: {}: {}",
        success.status,
        String::from_utf8_lossy(&success.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_path.join("manifest.json"))?)?;
    anyhow::ensure!(
        manifest.get("mode").and_then(serde_json::Value::as_str) == Some("handoff")
            && output_path.join("work-order.md").is_file()
            && output_path.join("code.txt").is_file(),
        "recovered handoff output has missing or wrong artifacts: {manifest:?}"
    );
    Ok(())
}

#[test]
fn export_redaction_uses_legacy_profile_unless_overridden() {
    use tokmd::cli::{CliExportArgs, RedactMode as CliRedactMode};
    use tokmd::{ResolvedConfig, resolve_export_with_config};
    use tokmd_settings::{ExportConfig, TomlConfig, ViewProfile};
    use tokmd_types::RedactMode;

    let profile = Profile {
        redact: Some(RedactMode::Paths),
        ..Default::default()
    };
    let mut config = ResolvedConfig {
        json_profile: Some(&profile),
        ..Default::default()
    };
    let mut cli = CliExportArgs {
        paths: None,
        format: None,
        output: None,
        module_roots: None,
        module_depth: None,
        children: None,
        min_code: None,
        max_rows: None,
        redact: None,
        meta: None,
        strip_prefix: None,
    };
    assert_eq!(
        resolve_export_with_config(&cli, &config).redact,
        RedactMode::Paths
    );

    let toml = TomlConfig {
        export: ExportConfig {
            redact: Some("none".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    config.toml = Some(&toml);
    assert_eq!(
        resolve_export_with_config(&cli, &config).redact,
        RedactMode::None
    );
    let view = ViewProfile {
        redact: Some("paths".into()),
        ..Default::default()
    };
    config.toml_view = Some(&view);
    assert_eq!(
        resolve_export_with_config(&cli, &config).redact,
        RedactMode::Paths
    );
    cli.redact = Some(CliRedactMode::None);
    assert_eq!(
        resolve_export_with_config(&cli, &config).redact,
        RedactMode::None
    );
}

#[cfg(target_os = "linux")]
#[test]
fn legacy_profile_redaction_reaches_export_and_config_report() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let repo = dir.path().join("repo");
    let user_config = dir.path().join("user-config");
    let legacy = user_config.join("tokmd/config.json");
    let toml = dir.path().join("selected.toml");
    std::fs::create_dir(&repo)?;
    std::fs::create_dir_all(
        legacy
            .parent()
            .ok_or_else(|| anyhow::anyhow!("legacy parent"))?,
    )?;
    std::fs::write(&toml, "")?;
    std::fs::write(repo.join("secret.rs"), "fn example() {}\n")?;
    std::fs::write(
        &legacy,
        r#"{"profiles":{"ci":{"redact":"paths"}},"repos":{}}"#,
    )?;
    let output = profile_selection_command(&repo, &toml, &user_config)
        .args(["--profile", "ci", "export", "--format", "json"])
        .output()?;
    anyhow::ensure!(output.status.success(), "export failed: {output:?}");
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(receipt["args"]["redact"], "paths");
    assert_eq!(
        receipt["rows"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing rows"))?
            .len(),
        1
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("secret.rs"));
    let report = profile_selection_command(&repo, &toml, &user_config)
        .args(["--profile", "ci", "--show-config"])
        .output()?;
    anyhow::ensure!(report.status.success(), "report failed: {report:?}");
    assert!(String::from_utf8_lossy(&report.stdout).contains("redact:         paths"));
    Ok(())
}
