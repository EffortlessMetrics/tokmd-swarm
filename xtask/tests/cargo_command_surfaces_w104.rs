use anyhow::{Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const POLICY: &str = include_str!("../../policy/cargo-command-surfaces.toml");
const CARGO_CONFIG: &str = include_str!("../../.cargo/config.toml");

#[derive(Deserialize)]
struct CargoConfig {
    alias: BTreeMap<String, Alias>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Alias {
    Text(String),
    Args(Vec<String>),
}

impl Alias {
    fn tokens(&self) -> Vec<&str> {
        match self {
            Self::Text(text) => text.split_whitespace().collect(),
            Self::Args(args) => args.iter().map(String::as_str).collect(),
        }
    }
}

// This contract handles Cargo aliases, not shell programs. Unsupported shell
// aliases and cycles remain NotProven rather than being counted as locked.
fn expand_aliases<'a>(tokens: &[&'a str], config: &'a CargoConfig) -> Result<Vec<&'a str>, ()> {
    let mut expanded = tokens.to_vec();
    let mut seen = BTreeSet::new();
    loop {
        let index = command_index(&expanded)?;
        let name = *expanded.get(index).ok_or(())?;
        let Some(alias) = config.alias.get(name) else {
            return Ok(expanded);
        };
        let replacement = alias.tokens();
        if !seen.insert(name)
            || replacement
                .first()
                .is_none_or(|token| token.starts_with('!'))
        {
            return Err(());
        }
        expanded.splice(index..index + 1, replacement);
    }
}

fn alias_fixture() -> Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir()?;
    fs::create_dir(fixture.path().join("src"))?;
    fs::create_dir(fixture.path().join(".cargo"))?;
    fs::write(fixture.path().join(".cargo/config.toml"), CARGO_CONFIG)?;
    fs::write(
        fixture.path().join("src/main.rs"),
        "fn main() { for arg in std::env::args().skip(1) { println!(\"{arg}\"); } }\n",
    )?;
    write_alias_manifest(fixture.path(), "0.1.0")?;
    // Generate canonical accepted bytes in this disposable dependency-free
    // fixture before testing whether the development alias preserves them.
    let lock = run_fixture_alias(fixture.path(), &["generate-lockfile"])?;
    ensure!(
        lock.status.success(),
        "fixture lock: {}",
        String::from_utf8_lossy(&lock.stderr)
    );
    Ok(fixture)
}

fn write_alias_manifest(fixture: &Path, version: &str) -> Result<()> {
    fs::write(
        fixture.join("Cargo.toml"),
        format!("[package]\nname = \"xtask\"\nversion = \"{version}\"\nedition = \"2021\"\n"),
    )?;
    Ok(())
}

fn run_fixture_alias(fixture: &Path, args: &[&str]) -> Result<std::process::Output> {
    Ok(Command::new(env!("CARGO"))
        .current_dir(fixture)
        .env("CARGO_HOME", fixture.join("cargo-home"))
        .env("CARGO_TARGET_DIR", fixture.join("target"))
        .env("CARGO_NET_OFFLINE", "true")
        .args(args)
        .output()?)
}

#[test]
fn development_alias_preserves_valid_stale_and_missing_locks() -> Result<()> {
    let fixture = alias_fixture()?;
    let root = fixture.path();
    let lock = root.join("Cargo.lock");
    let bytes = fs::read(&lock)?;
    // Use the committed, nested formatting alias. The tiny stand-in only
    // echoes xtask arguments; dependency resolution is performed by real Cargo.
    let valid = run_fixture_alias(root, &["fmt-check"])?;
    ensure!(
        valid.status.success(),
        "valid lock: {}",
        String::from_utf8_lossy(&valid.stderr)
    );
    ensure!(
        String::from_utf8(valid.stdout)?
            .lines()
            .eq(["lint-fix", "--check", "--no-clippy"]),
        "format alias arguments changed"
    );
    ensure!(fs::read(&lock)? == bytes, "valid lock changed");

    write_alias_manifest(root, "0.2.0")?;
    let stale = run_fixture_alias(root, &["fmt-check"])?;
    ensure!(!stale.status.success(), "alias accepted stale lock");
    ensure!(
        String::from_utf8_lossy(&stale.stderr).contains("--locked"),
        "missing locked diagnostic"
    );
    ensure!(fs::read(&lock)? == bytes, "stale lock was rewritten");

    write_alias_manifest(root, "0.1.0")?;
    fs::remove_file(&lock)?;
    let missing = run_fixture_alias(root, &["fmt-check"])?;
    ensure!(!missing.status.success(), "alias accepted missing lock");
    ensure!(!lock.exists(), "alias created missing lock");
    Ok(())
}

#[test]
fn unlocked_and_misplaced_alias_controls_really_move_the_lock() -> Result<()> {
    let fixture = alias_fixture()?;
    let root = fixture.path();
    let lock = root.join("Cargo.lock");
    let bytes = fs::read(&lock)?;
    fs::write(
        root.join(".cargo/config.toml"),
        CARGO_CONFIG.replace("run --locked -p xtask --", "run -p xtask --"),
    )?;
    write_alias_manifest(root, "0.2.0")?;
    let misplaced = run_fixture_alias(root, &["xtask", "--locked"])?;
    ensure!(
        misplaced.status.success(),
        "unlocked control did not run: {}",
        String::from_utf8_lossy(&misplaced.stderr)
    );
    ensure!(
        String::from_utf8(misplaced.stdout)?.trim() == "--locked",
        "flag was not forwarded to binary"
    );
    ensure!(
        fs::read(&lock)? != bytes,
        "unlocked control did not rewrite stale lock"
    );

    fs::write(&lock, &bytes)?;
    let global = run_fixture_alias(root, &["--locked", "xtask"])?;
    ensure!(
        !global.status.success(),
        "global locked control accepted stale lock"
    );
    ensure!(
        fs::read(&lock)? == bytes,
        "global locked control changed lock"
    );

    // The formatter itself does not resolve dependencies and accepts a stale
    // lock. Only the launcher of the convenience formatting alias needs locking.
    fs::write(root.join("src/main.rs"), "fn main() {}\n")?;
    let fmt = run_fixture_alias(root, &["fmt", "--", "--check"])?;
    ensure!(
        fmt.status.success(),
        "formatter control failed: {}",
        String::from_utf8_lossy(&fmt.stderr)
    );
    ensure!(fs::read(&lock)? == bytes, "formatter changed lock");
    Ok(())
}

#[test]
fn development_aliases_preserve_task_arguments() -> Result<()> {
    let fixture = alias_fixture()?;
    for (alias, expected) in [
        ("xtask", vec![]),
        ("fmt-check", vec!["lint-fix", "--check", "--no-clippy"]),
        ("fmt-fix", vec!["lint-fix", "--no-clippy"]),
        ("gate-check", vec!["gate", "--check"]),
        ("trim-target", vec!["trim-target"]),
        ("with-sccache", vec!["sccache", "--"]),
        ("sccache-check", vec!["sccache", "--check"]),
        ("sccache-stats", vec!["sccache", "--stats"]),
        ("sccache-stop", vec!["sccache", "--stop"]),
    ] {
        let output = run_fixture_alias(fixture.path(), &[alias])?;
        ensure!(
            output.status.success(),
            "{alias}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        ensure!(
            String::from_utf8(output.stdout)?.lines().eq(expected),
            "{alias}: changed task arguments"
        );
    }
    Ok(())
}

#[test]
fn alias_scanner_checks_the_launcher_before_forwarded_arguments() -> Result<()> {
    let locked: CargoConfig = toml::from_str(
        "[alias]\nxtask = 'run --locked -p xtask --'\nfmt-check = 'xtask lint-fix --check --no-clippy'\n",
    )?;
    let unlocked: CargoConfig = toml::from_str(
        "[alias]\nxtask = 'run -p xtask --'\nfmt-check = 'xtask lint-fix --check --no-clippy'\n",
    )?;
    for alias in ["xtask", "fmt-check"] {
        let line = format!("cargo {alias}");
        ensure!(
            scan_live_with_aliases(&line, true, &locked) == Verdict::Pass,
            "locked {alias}"
        );
        ensure!(
            scan_live_with_aliases(&line, false, &locked) == Verdict::MissingLock,
            "missing lock {alias}"
        );
        ensure!(
            scan_live_with_aliases(&line, true, &unlocked) == Verdict::Violation,
            "unlocked {alias}"
        );
        ensure!(
            scan_live_with_aliases(&format!("{line} --locked"), true, &unlocked)
                == Verdict::Violation,
            "forwarded flag counted as Cargo option"
        );
        ensure!(
            scan_live_with_aliases(&format!("cargo --locked {alias}"), true, &unlocked)
                == Verdict::Pass,
            "global lock flag lost"
        );
    }
    for aliases in [
        "[alias]\nxtask = ['run', '--locked', '-p', 'xtask', '--']\n",
        "[alias]\nxtask = 'run --frozen -p xtask --'\n",
    ] {
        let config = toml::from_str(aliases)?;
        ensure!(
            scan_live_with_aliases("cargo xtask", true, &config) == Verdict::Pass,
            "supported locked alias rejected"
        );
    }
    for aliases in [
        "[alias]\nxtask = 'xtask'\n",
        "[alias]\nxtask = 'other'\nother = 'xtask'\n",
        "[alias]\nxtask = '!cargo run --locked'\n",
        "[alias]\nxtask = ''\n",
    ] {
        let config = toml::from_str(aliases)?;
        ensure!(
            scan_live_with_aliases("cargo xtask", true, &config) == Verdict::NotProven,
            "unsupported alias silently passed"
        );
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct Inventory {
    schema_version: u32,
    inventory: String,
    claim: String,
    non_goal: String,
    candidate_roots: Vec<String>,
    surface: Vec<Surface>,
}

#[derive(Debug, Deserialize)]
struct Surface {
    path: String,
    mode: Mode,
    reason: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Mode {
    Live,
    Historical,
    Deferred,
    Dynamic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    Pass,
    Violation,
    Historical,
    Deferred,
    NotProven,
    MissingLock,
    Unclassified,
}

fn workspace_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask workspace parent is missing".into())
}

fn normalize(path: &str) -> String {
    let mut normalized = path.replace('\\', "/");
    while let Some(rest) = normalized.strip_prefix("./") {
        normalized = rest.to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    while let Some(rest) = normalized.strip_prefix("./") {
        normalized = rest.to_string();
    }
    normalized.trim_end_matches('/').to_string()
}

fn is_path_under(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn surface_for<'a>(inventory: &'a Inventory, path: &str) -> Option<&'a Surface> {
    let path = normalize(path);
    inventory
        .surface
        .iter()
        .filter(|surface| is_path_under(&path, &normalize(&surface.path)))
        .max_by_key(|surface| normalize(&surface.path).len())
}

fn candidate_path(inventory: &Inventory, path: &str) -> bool {
    let path = normalize(path);
    inventory
        .candidate_roots
        .iter()
        .any(|root| is_path_under(&path, &normalize(root)))
}

fn classify(inventory: &Inventory, path: &str) -> Verdict {
    if !candidate_path(inventory, path) {
        return Verdict::Unclassified;
    }

    match surface_for(inventory, path).map(|surface| surface.mode) {
        Some(Mode::Live) => Verdict::Pass,
        Some(Mode::Historical) => Verdict::Historical,
        Some(Mode::Deferred) => Verdict::Deferred,
        Some(Mode::Dynamic) => Verdict::NotProven,
        None => Verdict::Unclassified,
    }
}

fn command_tokens(line: &str) -> Option<Vec<&str>> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    let cargo = tokens.iter().enumerate().find_map(|(index, token)| {
        let normalized = token.trim_matches(['`', '\'', '"']);
        let is_cargo = normalized == "cargo" || normalized.strip_suffix("/cargo").is_some();
        let prefix_is_invocation = tokens.iter().take(index).all(|prefix| {
            *prefix == "-"
                || *prefix == "*"
                || *prefix == ">"
                || *prefix == "rtk"
                || *prefix == "proxy"
                || *prefix == "&&"
                || *prefix == "||"
                || *prefix == "|"
                || *prefix == ";"
                || prefix.contains('=')
        });
        (is_cargo && prefix_is_invocation).then_some(index)
    })?;
    (cargo + 1 < tokens.len()).then(|| {
        tokens
            .get(cargo..)
            .map_or_else(Vec::new, |slice| slice.to_vec())
    })
}

fn command_index(tokens: &[&str]) -> Result<usize, ()> {
    let mut index = 1;
    while let Some(token) = tokens.get(index).copied() {
        if token.starts_with('+') {
            index += 1;
            continue;
        }
        if token == "--manifest-path" || token == "--config" || token == "--color" {
            index += 2;
            continue;
        }
        if matches!(token, "-C" | "-Z") {
            index += 2;
            continue;
        }
        if matches!(token, "-v" | "-vv" | "-q" | "-qq") || token.starts_with("-Z") {
            index += 1;
            continue;
        }
        if token.starts_with("--manifest-path=")
            || token.starts_with("--config=")
            || token.starts_with("--color=")
            || matches!(
                token,
                "--locked" | "--frozen" | "--offline" | "--quiet" | "--verbose"
            )
        {
            index += 1;
            continue;
        }
        if token.starts_with('-') {
            return Err(());
        }
        return Ok(index);
    }
    Err(())
}

fn governed_command<'a>(tokens: &[&'a str]) -> Result<Option<&'a str>, ()> {
    let command = *tokens.get(command_index(tokens)?).ok_or(())?;
    Ok(matches!(
        command,
        "build" | "check" | "test" | "clippy" | "run" | "install" | "update" | "generate-lockfile"
    )
    .then_some(command))
}

fn ambiguous_command_line(line: &str) -> bool {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    let Some(cargo) = tokens.iter().position(|token| {
        let token = token.trim_matches(['`', '\'', '"']);
        token == "cargo"
            || token.ends_with("/cargo")
            || token.ends_with("\\cargo")
            || token.ends_with("cargo.exe")
    }) else {
        return false;
    };
    let Some(arguments) = tokens.get(cargo + 1..) else {
        return false;
    };
    arguments.iter().any(|token| {
        token.starts_with('$')
            || token.starts_with('"')
            || token.starts_with('\'')
            || (*token == "--features"
                && arguments
                    .iter()
                    .position(|candidate| *candidate == "--features")
                    .and_then(|feature| arguments.get(feature + 1..))
                    .is_some_and(|after_features| {
                        after_features.iter().any(|candidate| {
                            matches!(
                                *candidate,
                                "build"
                                    | "check"
                                    | "test"
                                    | "clippy"
                                    | "install"
                                    | "update"
                                    | "generate-lockfile"
                            )
                        })
                    }))
    }) || tokens.get(cargo).is_some_and(|token| {
        let token = token.trim_matches(['`', '\'', '"']);
        token.ends_with("cargo.exe") || token.starts_with(".\\") || token.starts_with("/")
    })
}

fn dynamic_line(line: &str) -> bool {
    ["$(", "${", "{{", "format!", "Command::new", "env!("]
        .iter()
        .any(|marker| line.contains(marker))
}

fn scan_live(text: &str, lock_present: bool) -> Verdict {
    let Ok(config) = toml::from_str(CARGO_CONFIG) else {
        return Verdict::NotProven;
    };
    scan_live_with_aliases(text, lock_present, &config)
}

fn scan_live_with_aliases(text: &str, lock_present: bool, config: &CargoConfig) -> Verdict {
    for line in text.lines() {
        if ambiguous_command_line(line) {
            return Verdict::NotProven;
        }
        let Some(tokens) = command_tokens(line) else {
            continue;
        };
        let tokens = match expand_aliases(&tokens, config) {
            Ok(tokens) => tokens,
            Err(()) => return Verdict::NotProven,
        };
        let command = match governed_command(&tokens) {
            Ok(Some(command)) => command,
            Ok(None) => continue,
            Err(()) => return Verdict::NotProven,
        };
        if dynamic_line(line) {
            return Verdict::NotProven;
        }
        if !lock_present {
            return Verdict::MissingLock;
        }
        if matches!(command, "update" | "generate-lockfile") {
            return Verdict::Violation;
        }
        let cargo_arguments = tokens
            .iter()
            .position(|token| *token == "--")
            .map_or(tokens.as_slice(), |separator| {
                tokens.get(..separator).unwrap_or_default()
            });
        if !cargo_arguments
            .iter()
            .any(|token| matches!(*token, "--locked" | "--frozen"))
        {
            return Verdict::Violation;
        }
    }
    Verdict::Pass
}

fn evaluate(mode: Mode, text: &str, lock_present: bool) -> Verdict {
    match mode {
        Mode::Live => scan_live(text, lock_present),
        Mode::Historical => Verdict::Historical,
        Mode::Deferred => Verdict::Deferred,
        Mode::Dynamic => Verdict::NotProven,
    }
}

fn load_inventory() -> Result<Inventory, Box<dyn std::error::Error>> {
    toml::from_str(POLICY).map_err(Into::into)
}

fn candidate_files(
    root: &Path,
    inventory: &Inventory,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(tracked_files(root)?
        .into_iter()
        .filter(|path| candidate_path(inventory, path))
        .collect())
}

fn tracked_files(root: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["ls-files"])
        .output()?;
    if !output.status.success() {
        return Err(format!("git ls-files failed with {}", output.status).into());
    }
    let files = String::from_utf8(output.stdout)?
        .lines()
        .map(normalize)
        .filter(|path| !path.is_empty())
        .collect();
    Ok(files)
}

#[test]
fn policy_is_closed_and_live_surfaces_are_locked() -> Result<(), Box<dyn std::error::Error>> {
    let inventory = load_inventory()?;
    assert_eq!(inventory.schema_version, 1);
    assert_eq!(inventory.inventory, "closed-world");
    assert!(inventory.claim.contains("adoption"));
    assert!(inventory.non_goal.contains("depguard"));

    let mut roots = BTreeSet::new();
    for root in &inventory.candidate_roots {
        assert!(
            roots.insert(normalize(root)),
            "duplicate candidate root: {root}"
        );
        assert!(
            surface_for(&inventory, root).is_some(),
            "candidate root is not classified: {root}"
        );
    }

    let mut surface_paths = BTreeSet::new();
    for surface in &inventory.surface {
        assert!(
            surface_paths.insert(normalize(&surface.path)),
            "duplicate surface path: {}",
            surface.path
        );
    }

    let live = inventory
        .surface
        .iter()
        .filter(|surface| surface.mode == Mode::Live)
        .collect::<Vec<_>>();
    if live.len() != 3 {
        return Err("canonical guidance and Cargo aliases must be adopted".into());
    }
    assert!(live.iter().all(|surface| !surface.reason.is_empty()));

    let root = workspace_root()?;
    let lock_present = root.join("Cargo.lock").is_file();
    assert!(lock_present, "the checked workspace must have Cargo.lock");
    let mut unclassified = Vec::new();
    let mut violations = Vec::new();
    for path in tracked_files(&root)? {
        if surface_for(&inventory, &path).is_none() {
            unclassified.push(path);
        }
    }
    for path in candidate_files(&root, &inventory)? {
        let Some(surface) = surface_for(&inventory, &path) else {
            unclassified.push(path);
            continue;
        };
        if surface.mode != Mode::Live {
            continue;
        }
        let text = fs::read_to_string(root.join(&path))?;
        let verdict = if path == ".cargo/config.toml" {
            let config: CargoConfig = toml::from_str(&text)?;
            let invocations = config
                .alias
                .keys()
                .map(|name| format!("cargo {name}"))
                .collect::<Vec<_>>()
                .join("\n");
            scan_live_with_aliases(&invocations, lock_present, &config)
        } else {
            scan_live(&text, lock_present)
        };
        if verdict != Verdict::Pass {
            violations.push(path);
        }
    }
    assert!(
        unclassified.is_empty(),
        "unclassified candidate paths: {unclassified:?}"
    );
    assert!(
        violations.is_empty(),
        "unlocked live surfaces: {violations:?}"
    );

    assert_eq!(
        classify(&inventory, "new-command-surface.md"),
        Verdict::Unclassified
    );
    assert_eq!(
        classify(&inventory, "docs/new-command-surface.md"),
        Verdict::Deferred
    );
    assert_eq!(
        classify(&inventory, "xtask/src/new_command.rs"),
        Verdict::NotProven
    );
    Ok(())
}

#[test]
fn scanner_has_positive_negative_and_missing_lock_controls() {
    assert_eq!(
        scan_live("cargo build --locked\ncargo test --frozen", true),
        Verdict::Pass
    );
    assert_eq!(scan_live("cargo fmt --check", false), Verdict::Pass);
    assert_eq!(
        scan_live("cargo install --path crates/tokmd", true),
        Verdict::Violation
    );
    assert_eq!(scan_live("cargo update", true), Verdict::Violation);
    assert_eq!(
        scan_live("cargo test --locked", false),
        Verdict::MissingLock
    );
    assert_eq!(
        scan_live("cargo test --locked --manifest-path ${MANIFEST}", true),
        Verdict::NotProven
    );
    assert_eq!(scan_live("cargo -v build --locked", true), Verdict::Pass);
    assert_eq!(scan_live("cargo -q test", true), Verdict::Violation);
    assert_eq!(
        scan_live("cargo -x build --locked", true),
        Verdict::NotProven
    );
    assert_eq!(scan_live("cargo run -- --locked", true), Verdict::Violation);
    assert_eq!(
        scan_live(
            "cargo +stable --manifest-path Cargo.toml build --locked",
            true
        ),
        Verdict::Pass
    );
    assert_eq!(
        scan_live(
            "RUSTUP_TOOLCHAIN=stable cargo --manifest-path Cargo.toml test --locked",
            true
        ),
        Verdict::Pass
    );
    assert_eq!(
        scan_live("cargo test --features x --locked", true),
        Verdict::Pass
    );
    assert_eq!(
        scan_live("see cargo build in the guide", true),
        Verdict::Pass
    );
    for line in [
        "cargo $cmd",
        "cargo \"$cmd\"",
        "cargo --features x build",
        "cargo.exe build --locked",
        ".\\cargo.exe test --locked",
        "/usr/bin/cargo build --locked",
    ] {
        assert_eq!(scan_live(line, true), Verdict::NotProven, "{line}");
    }
}

#[test]
fn scanner_preserves_historical_deferred_and_dynamic_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        evaluate(Mode::Historical, "cargo install tokmd", false),
        Verdict::Historical
    );
    assert_eq!(
        evaluate(Mode::Deferred, "cargo install tokmd", false),
        Verdict::Deferred
    );
    assert_eq!(
        evaluate(Mode::Dynamic, "Command::new(\"cargo\")", true),
        Verdict::NotProven
    );

    let inventory = load_inventory()?;
    assert_eq!(
        classify(&inventory, "docs/examples/real-user-path-smoke-run.md"),
        Verdict::Historical
    );
    assert_eq!(
        classify(&inventory, ".jules/goals/active.toml"),
        Verdict::Historical
    );
    assert_eq!(
        classify(
            &inventory,
            ".factory/security/reports/security-report-2026-08-17.md"
        ),
        Verdict::Historical
    );
    assert_eq!(
        classify(&inventory, "docs/reference-cli.md"),
        Verdict::Deferred
    );
    assert_eq!(
        classify(&inventory, "xtask/src/tasks/docs.rs"),
        Verdict::NotProven
    );
    Ok(())
}

fn temp_fixture_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    for attempt in 0..100u32 {
        let candidate = base.join(format!("tokmd-cargo-command-surfaces-{pid}-{attempt}"));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err("could not allocate an isolated Cargo fixture directory".into())
}

fn fixture_cargo_check(
    fixture: &Path,
) -> Result<std::process::ExitStatus, Box<dyn std::error::Error>> {
    let manifest = fixture.join("Cargo.toml");
    let target = fixture.join("target");
    let manifest_arg = manifest.to_string_lossy().to_string();
    Ok(Command::new("cargo")
        .current_dir(fixture)
        .env("CARGO_TARGET_DIR", target)
        .args([
            "check",
            "--locked",
            "--offline",
            "--manifest-path",
            &manifest_arg,
        ])
        .status()?)
}

#[test]
fn locked_fixture_rejects_lock_movement_without_mutating_lock()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = temp_fixture_dir()?;
    let outcome = (|| -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir(fixture.join("src"))?;
        fs::write(
            fixture.join("Cargo.toml"),
            "[package]\nname = \"cargo-command-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )?;
        fs::write(fixture.join("src/main.rs"), "fn main() {}\n")?;
        fs::write(
            fixture.join("Cargo.lock"),
            "# This file is automatically @generated by Cargo.\nversion = 3\n\n[[package]]\nname = \"cargo-command-fixture\"\nversion = \"0.1.0\"\n",
        )?;

        assert!(fixture_cargo_check(&fixture)?.success());
        let lock = fixture.join("Cargo.lock");
        let locked_bytes = fs::read(&lock)?;

        fs::write(
            fixture.join("Cargo.toml"),
            "[package]\nname = \"cargo-command-fixture\"\nversion = \"0.2.0\"\nedition = \"2021\"\n",
        )?;
        assert!(!fixture_cargo_check(&fixture)?.success());
        assert_eq!(fs::read(&lock)?, locked_bytes);

        fs::write(
            fixture.join("Cargo.toml"),
            "[package]\nname = \"cargo-command-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )?;
        fs::remove_file(&lock)?;
        assert!(!fixture_cargo_check(&fixture)?.success());
        assert!(!lock.exists());
        Ok(())
    })();
    let cleanup = fs::remove_dir_all(&fixture);
    outcome.and(cleanup.map_err(Into::into))
}

#[test]
fn normalize_matches_canonical_path_boundaries() {
    let cases = [
        (r".\docs\examples\file.md", "docs/examples/file.md"),
        ("././docs/examples/file.md", "docs/examples/file.md"),
        ("/docs/examples/file.md/", "docs/examples/file.md"),
        ("docs/examples/", "docs/examples"),
    ];
    for (input, expected) in cases {
        assert_eq!(
            normalize(input),
            expected,
            "normalization mismatch: {input}"
        );
    }
}
