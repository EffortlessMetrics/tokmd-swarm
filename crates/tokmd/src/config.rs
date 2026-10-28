use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tokmd_settings::{Profile, TomlConfig, UserConfig, ViewProfile};

pub mod explain;
mod resolve;

pub use resolve::{
    resolve_export, resolve_export_with_config, resolve_lang, resolve_lang_with_config,
    resolve_module, resolve_module_with_config,
};

/// Configuration context combining TOML config, JSON config, and resolved profile.
///
/// # Example
///
/// ```rust
/// use tokmd::config::load_config;
///
/// let config = load_config();
/// // config.toml and config.json will be loaded from the environment if present
/// ```
#[derive(Debug, Default)]
pub struct ConfigContext {
    /// TOML configuration (tokmd.toml)
    pub toml: Option<TomlConfig>,
    /// Path where TOML config was found
    pub toml_path: Option<PathBuf>,
    /// Legacy JSON configuration (config.json)
    pub json: Option<UserConfig>,
}

impl ConfigContext {
    /// Get view profile from TOML config by name.
    pub fn get_toml_view(&self, name: &str) -> Option<&ViewProfile> {
        self.toml.as_ref().and_then(|t| t.view.get(name))
    }

    /// Get profile from JSON config by name.
    pub fn get_json_profile(&self, name: &str) -> Option<&Profile> {
        self.json.as_ref().and_then(|c| c.profiles.get(name))
    }
}

/// Load configuration on a best-effort basis for existing library callers.
/// The CLI uses `load_config_checked` so a selected invalid file is reported.
pub fn load_config() -> ConfigContext {
    let toml_result = discover_toml_config(false).ok().flatten();
    let json = load_json_config();

    ConfigContext {
        toml: toml_result.as_ref().map(|(config, _)| config.clone()),
        toml_path: toml_result.map(|(_, path)| path),
        json,
    }
}

/// Startup-only diagnostics, without changing the public best-effort context.
pub(crate) struct LoadedConfig {
    pub context: ConfigContext,
    pub legacy_failure: Option<anyhow::Error>,
}

/// TOML failures are fatal. Retain legacy failures for diagnostics and selected
/// profiles, while preserving successful unselected and matching-view commands.
pub(crate) fn load_config_checked() -> Result<LoadedConfig> {
    let toml_result = discover_toml_config(true)?;
    let (json, legacy_failure) = match load_json_config_checked() {
        Ok(json) => (json, None),
        Err(error) => (None, Some(error)),
    };
    Ok(LoadedConfig {
        context: ConfigContext {
            toml: toml_result.as_ref().map(|(config, _)| config.clone()),
            toml_path: toml_result.map(|(_, path)| path),
            json,
        },
        legacy_failure,
    })
}

fn sanitize_selector(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Discover TOML configuration following the precedence chain:
/// 1. TOKMD_CONFIG env var (explicit path)
/// 2. ./tokmd.toml (current directory)
/// 3. Parent directories up to root
/// 4. ~/.config/tokmd/tokmd.toml (user config)
fn discover_toml_config(strict: bool) -> Result<Option<(TomlConfig, PathBuf)>> {
    // 1. Check TOKMD_CONFIG environment variable
    if let Ok(config_path) = std::env::var("TOKMD_CONFIG")
        && let Some(config_path) = sanitize_selector(&config_path)
    {
        let path = PathBuf::from(&config_path);
        if let Some(result) = try_load_toml(&path, strict)? {
            return Ok(Some(result));
        }
        if strict {
            bail!("TOKMD_CONFIG points to a missing file: {}", path.display());
        }
    }

    // 2. Check current directory and walk up to root
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = Some(cwd.as_path());
        while let Some(d) = dir {
            let config_path = d.join("tokmd.toml");
            if let Some(result) = try_load_toml(&config_path, strict)? {
                return Ok(Some(result));
            }
            dir = d.parent();
        }
    }

    // 3. Check user config directory
    if let Some(config_dir) = dirs::config_dir() {
        let user_config_path = config_dir.join("tokmd").join("tokmd.toml");
        if let Some(result) = try_load_toml(&user_config_path, strict)? {
            return Ok(Some(result));
        }
    }

    Ok(None)
}

/// Try to load a TOML config file if it exists.
fn try_load_toml(path: &Path, strict: bool) -> Result<Option<(TomlConfig, PathBuf)>> {
    match TomlConfig::from_file(path) {
        Ok(config) => Ok(Some((config, path.to_path_buf()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) if strict => Err(error)
            .with_context(|| format!("Failed to load TOML config from {}", path.display())),
        Err(_) => Ok(None),
    }
}

/// Load legacy JSON configuration from user config directory.
fn load_json_config() -> Option<UserConfig> {
    load_json_config_checked().ok().flatten()
}

fn load_json_config_checked() -> Result<Option<UserConfig>> {
    let Some(config_dir) = dirs::config_dir() else {
        return Ok(None);
    };
    load_json_config_from(&config_dir.join("tokmd").join("config.json"))
}

fn load_json_config_from(path: &Path) -> Result<Option<UserConfig>> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "Failed to read legacy JSON configuration from {}",
                    path.display()
                )
            });
        }
    };
    serde_json::from_str(&content)
        .with_context(|| {
            format!(
                "Failed to parse legacy JSON configuration from {}",
                path.display()
            )
        })
        .map(Some)
}

/// Get the profile name from CLI arg, env var, or default.
///
/// # Examples
///
/// ```
/// use tokmd::config::get_profile_name;
///
/// let cli_profile = "  ci-profile  ".to_string();
/// assert_eq!(
///     get_profile_name(Some(&cli_profile)).as_deref(),
///     Some("ci-profile")
/// );
/// ```
pub fn get_profile_name(cli_profile: Option<&String>) -> Option<String> {
    // CLI argument takes precedence
    if let Some(name) = cli_profile
        && let Some(name) = sanitize_selector(name)
    {
        return Some(name);
    }

    // Then check TOKMD_PROFILE environment variable
    std::env::var("TOKMD_PROFILE")
        .ok()
        .and_then(|name| sanitize_selector(&name))
}

/// Resolve a JSON profile by name (legacy).
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
///
/// use tokmd::config::resolve_profile;
/// use tokmd_settings::{Profile, UserConfig};
///
/// let mut profiles = BTreeMap::new();
/// let profile = Profile {
///     top: Some(10),
///     ..Default::default()
/// };
/// profiles.insert("default".to_string(), profile);
///
/// let config = Some(UserConfig {
///     profiles,
///     repos: BTreeMap::new(),
/// });
///
/// let resolved = resolve_profile(&config, None).expect("default profile");
/// assert_eq!(resolved.top, Some(10));
/// ```
pub fn resolve_profile<'a>(
    config: &'a Option<UserConfig>,
    name: Option<&String>,
) -> Option<&'a Profile> {
    config.as_ref().and_then(|c| {
        let key = name.map(|s| s.as_str()).unwrap_or("default");
        c.profiles.get(key)
    })
}

/// Resolved configuration combining TOML and JSON sources.
///
/// This struct aggregates configurations from both `tokmd.toml` and the legacy
/// `config.json`, preferring TOML settings over JSON ones.
///
/// # Examples
///
/// ```
/// use tokmd::{ConfigContext, ResolvedConfig};
///
/// let ctx = ConfigContext::default();
/// let resolved = tokmd::resolve_config(&ctx, None);
///
/// assert_eq!(resolved.format(), None);
/// assert_eq!(resolved.top(), None);
/// ```
#[derive(Debug, Default)]
pub struct ResolvedConfig<'a> {
    /// TOML view profile (takes precedence).
    pub toml_view: Option<&'a ViewProfile>,
    /// JSON profile (fallback).
    pub json_profile: Option<&'a Profile>,
    /// TOML config sections.
    pub toml: Option<&'a TomlConfig>,
    /// Path of the discovered TOML config, for resolving config-relative inputs.
    pub toml_path: Option<&'a Path>,
}

impl ResolvedConfig<'_> {
    /// Get format string, preferring TOML view, then JSON profile.
    pub fn format(&self) -> Option<&str> {
        self.toml_view
            .and_then(|v| v.format.as_deref())
            .or_else(|| self.json_profile.and_then(|p| p.format.as_deref()))
    }

    /// Get top value.
    pub fn top(&self) -> Option<usize> {
        self.toml_view
            .and_then(|v| v.top)
            .or_else(|| self.json_profile.and_then(|p| p.top))
    }

    /// Get files flag.
    pub fn files(&self) -> Option<bool> {
        self.toml_view
            .and_then(|v| v.files)
            .or_else(|| self.json_profile.and_then(|p| p.files))
    }

    /// Get module roots.
    pub fn module_roots(&self) -> Option<Vec<String>> {
        self.toml_view
            .and_then(|v| v.module_roots.clone())
            .or_else(|| self.toml.and_then(|t| t.module.roots.clone()))
            .or_else(|| self.json_profile.and_then(|p| p.module_roots.clone()))
    }

    /// Get module depth.
    pub fn module_depth(&self) -> Option<usize> {
        self.toml_view
            .and_then(|v| v.module_depth)
            .or_else(|| self.toml.and_then(|t| t.module.depth))
            .or_else(|| self.json_profile.and_then(|p| p.module_depth))
    }

    /// Get children mode string.
    pub fn children(&self) -> Option<&str> {
        self.toml_view
            .and_then(|v| v.children.as_deref())
            .or_else(|| self.toml.and_then(|t| t.module.children.as_deref()))
            .or_else(|| self.json_profile.and_then(|p| p.children.as_deref()))
    }

    /// Get min_code.
    pub fn min_code(&self) -> Option<usize> {
        self.toml_view
            .and_then(|v| v.min_code)
            .or_else(|| self.toml.and_then(|t| t.export.min_code))
            .or_else(|| self.json_profile.and_then(|p| p.min_code))
    }

    /// Get max_rows.
    pub fn max_rows(&self) -> Option<usize> {
        self.toml_view
            .and_then(|v| v.max_rows)
            .or_else(|| self.toml.and_then(|t| t.export.max_rows))
            .or_else(|| self.json_profile.and_then(|p| p.max_rows))
    }

    /// Get redact mode string.
    pub fn redact(&self) -> Option<&str> {
        self.toml_view
            .and_then(|v| v.redact.as_deref())
            .or_else(|| self.toml.and_then(|t| t.export.redact.as_deref()))
            .or_else(|| {
                self.json_profile
                    .and_then(|p| p.redact)
                    .map(|mode| match mode {
                        tokmd_types::RedactMode::None => "none",
                        tokmd_types::RedactMode::Paths => "paths",
                        tokmd_types::RedactMode::All => "all",
                    })
            })
    }

    /// Get meta flag.
    pub fn meta(&self) -> Option<bool> {
        self.toml_view
            .and_then(|v| v.meta)
            .or_else(|| self.json_profile.and_then(|p| p.meta))
    }
}

/// Resolve configuration from context and profile name.
///
/// # Examples
///
/// ```
/// use tokmd::{resolve_config, ConfigContext};
///
/// let ctx = ConfigContext::default();
/// let resolved = resolve_config(&ctx, Some("default"));
///
/// assert!(resolved.toml_view.is_none());
/// assert!(resolved.json_profile.is_none());
/// ```
pub fn resolve_config<'a>(
    ctx: &'a ConfigContext,
    profile_name: Option<&str>,
) -> ResolvedConfig<'a> {
    let toml_view = profile_name.and_then(|name| ctx.get_toml_view(name));
    let json_profile = profile_name.and_then(|name| ctx.get_json_profile(name));

    ResolvedConfig {
        toml_view,
        json_profile,
        toml: ctx.toml.as_ref(),
        toml_path: ctx.toml_path.as_deref(),
    }
}

/// A selected profile has no matching configuration and already names its repair.
#[derive(Debug)]
pub(crate) struct UnknownProfile(String);

impl std::fmt::Display for UnknownProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for UnknownProfile {}

/// Normal CLI commands must not silently discard a selected profile.
/// Keep public best-effort resolution and `--show-config` diagnostics separate.
pub(crate) fn check_selected_profile(
    ctx: &ConfigContext,
    name: Option<&str>,
    legacy_failure: Option<anyhow::Error>,
) -> Result<()> {
    let Some(name) = name else {
        return Ok(());
    };
    if ctx.get_toml_view(name).is_some() || ctx.get_json_profile(name).is_some() {
        return Ok(());
    }

    let names: std::collections::BTreeSet<&str> = ctx
        .toml
        .iter()
        .flat_map(|config| config.view.keys().map(String::as_str))
        .chain(
            ctx.json
                .iter()
                .flat_map(|config| config.profiles.keys().map(String::as_str)),
        )
        .collect();
    let available = if names.is_empty() {
        "(none)".to_string()
    } else {
        names
            .iter()
            .map(|name| format!("{name:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let error = UnknownProfile(format!(
        "Unknown profile {name:?}. Available profiles: {available}. Use `tokmd --show-config` to inspect configuration sources, then select or define the profile."
    ));
    match legacy_failure {
        Some(failure) => Err(failure.context(UnknownProfile(format!(
            "{error} Repair the legacy JSON configuration and retry."
        )))),
        None => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ConfigContext, Profile, TomlConfig, UnknownProfile, UserConfig, ViewProfile,
        check_selected_profile, get_profile_name, resolve_config, sanitize_selector,
    };

    #[test]
    fn legacy_failure_loader_distinguishes_absence_parse_and_read_errors() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.json");
        assert!(super::load_json_config_from(&path)?.is_none());
        for malformed in [r#"{"profiles":{"ci":"#, r#"{"profiles":[]}"#] {
            std::fs::write(&path, malformed)?;
            let error = super::load_json_config_from(&path).unwrap_err();
            assert!(error.downcast_ref::<serde_json::Error>().is_some());
            let message = format!("{error:#}");
            assert!(
                message.contains(path.to_string_lossy().as_ref())
                    && message.contains("line")
                    && message.contains("column"),
                "{message}"
            );
        }
        std::fs::write(&path, [0xff])?;
        let error = super::load_json_config_from(&path).unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::InvalidData
        );
        assert!(format!("{error:#}").contains(path.to_string_lossy().as_ref()));
        std::fs::remove_file(&path)?;
        std::fs::create_dir(&path)?;
        let error = super::load_json_config_from(&path).unwrap_err();
        assert!(error.downcast_ref::<std::io::Error>().is_some());
        assert!(format!("{error:#}").contains(path.to_string_lossy().as_ref()));
        std::fs::remove_dir(&path)?;
        std::fs::write(&path, r#"{"profiles":{"ci":{"format":"json"}},"repos":{}}"#)?;
        assert!(
            super::load_json_config_from(&path)?
                .unwrap()
                .profiles
                .contains_key("ci")
        );
        Ok(())
    }

    #[test]
    fn legacy_failure_selected_errors_preserve_local_typed_recovery() -> anyhow::Result<()> {
        let failure = || {
            anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            .context("Failed to read legacy JSON configuration from rate_limit/timeout base ref/config.json")
        };
        let name = "rate_limit/timeout base ref";
        let error = check_selected_profile(&ConfigContext::default(), Some(name), Some(failure()))
            .unwrap_err();
        assert!(error.downcast_ref::<UnknownProfile>().is_some());
        assert!(error.downcast_ref::<std::io::Error>().is_some());
        assert_eq!(crate::exit_code(&error), 2);
        let message = crate::format_error(&error);
        assert!(
            message.contains("Repair the legacy JSON configuration")
                && message.contains("config.json")
                && message.contains("permission denied"),
            "{message}"
        );
        assert!(
            !message.contains("tokmd JSON receipt")
                && !message.contains("git fetch")
                && !message.contains("Retry after a short delay")
                && !message.contains("rate limit"),
            "{message}"
        );
        check_selected_profile(&ConfigContext::default(), None, Some(failure()))?;
        let mut toml = TomlConfig::default();
        toml.view.insert(name.into(), ViewProfile::default());
        let ctx = ConfigContext {
            toml: Some(toml),
            ..Default::default()
        };
        check_selected_profile(&ctx, Some(name), Some(failure()))?;
        let mut json = UserConfig::default();
        json.profiles.insert(name.into(), Profile::default());
        let ctx = ConfigContext {
            json: Some(json),
            ..Default::default()
        };
        check_selected_profile(&ctx, Some(name), Some(failure()))?;
        Ok(())
    }

    #[test]
    fn profile_selection_constructed_matches_preserve_precedence() -> anyhow::Result<()> {
        check_selected_profile(&ConfigContext::default(), None, None)?;
        let mut toml = TomlConfig::default();
        toml.view.insert("empty".into(), ViewProfile::default());
        toml.view.insert(
            "shared".into(),
            ViewProfile {
                format: Some("json".into()),
                files: Some(true),
                ..Default::default()
            },
        );
        let mut json = UserConfig::default();
        json.profiles.insert("legacy".into(), Profile::default());
        json.profiles.insert(
            "shared".into(),
            Profile {
                format: Some("md".into()),
                files: Some(false),
                ..Default::default()
            },
        );
        let ctx = ConfigContext {
            toml: Some(toml),
            json: Some(json),
            ..Default::default()
        };
        for name in ["empty", "shared", "legacy"] {
            check_selected_profile(&ctx, Some(name), None)?;
        }
        let shared = resolve_config(&ctx, Some("shared"));
        anyhow::ensure!(
            shared.format() == Some("json") && shared.files() == Some(true),
            "TOML view must retain precedence over matching legacy JSON"
        );
        let unknown = resolve_config(&ctx, Some("unknown"));
        anyhow::ensure!(
            unknown.toml_view.is_none() && unknown.json_profile.is_none(),
            "public best-effort resolution must remain available"
        );
        Ok(())
    }

    #[test]
    fn profile_selection_constructed_errors_are_local_sorted_and_escaped() -> anyhow::Result<()> {
        let mut toml = TomlConfig::default();
        for name in ["zebra", "shared", "quote\"\nname"] {
            toml.view.insert(name.into(), ViewProfile::default());
        }
        let mut json = UserConfig::default();
        for name in ["shared", "alpha"] {
            json.profiles.insert(name.into(), Profile::default());
        }
        let ctx = ConfigContext {
            toml: Some(toml),
            json: Some(json),
            ..Default::default()
        };
        let name = "rate_limit/timeout base ref";
        for (context, available) in [
            (ctx, r#""alpha", "quote\"\nname", "shared", "zebra""#),
            (ConfigContext::default(), "(none)"),
        ] {
            let error = check_selected_profile(&context, Some(name), None)
                .err()
                .ok_or_else(|| anyhow::anyhow!("unmatched profile was accepted"))?;
            anyhow::ensure!(
                error.downcast_ref::<UnknownProfile>().is_some() && crate::exit_code(&error) == 2,
                "unmatched profile must retain its typed usage error"
            );
            let expected = format!(
                "Error: Unknown profile {name:?}. Available profiles: {available}. Use `tokmd --show-config` to inspect configuration sources, then select or define the profile."
            );
            anyhow::ensure!(
                crate::format_error(&error) == expected,
                "wrong complete local profile diagnostic: {}",
                crate::format_error(&error)
            );
        }
        Ok(())
    }

    #[test]
    fn sanitize_selector_rejects_empty_and_control_values() {
        assert_eq!(sanitize_selector("   "), None);
        assert_eq!(sanitize_selector("bad\nvalue"), None);
        assert_eq!(sanitize_selector("bad\0value"), None);
    }

    #[test]
    fn sanitize_selector_trims_safe_values() {
        assert_eq!(sanitize_selector("  default  ").as_deref(), Some("default"));
    }

    #[test]
    fn get_profile_name_sanitizes_cli_value() {
        let cli_value = "  secure-profile  ".to_string();
        assert_eq!(
            get_profile_name(Some(&cli_value)).as_deref(),
            Some("secure-profile")
        );
    }

    #[test]
    fn get_profile_name_rejects_control_char_cli_value() {
        let cli_value = "bad\nprofile".to_string();
        assert_eq!(get_profile_name(Some(&cli_value)), None);
    }
}
