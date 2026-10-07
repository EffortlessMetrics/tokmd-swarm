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
            && receipt.args.with_files
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
        std::io::Error::new(std::io::ErrorKind::Other, "network error"),
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
