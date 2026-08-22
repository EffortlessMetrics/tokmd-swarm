use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const POLICY: &str = include_str!("../../policy/cargo-command-surfaces.toml");

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

fn governed_command<'a>(tokens: &'a [&'a str]) -> Result<Option<&'a str>, ()> {
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
        let command = token;
        return Ok(matches!(
            command,
            "build"
                | "check"
                | "test"
                | "clippy"
                | "run"
                | "install"
                | "update"
                | "generate-lockfile"
        )
        .then_some(command));
    }
    Err(())
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
    for line in text.lines() {
        if ambiguous_command_line(line) {
            return Verdict::NotProven;
        }
        let Some(tokens) = command_tokens(line) else {
            continue;
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
            .map_or(tokens.as_slice(), |separator| &tokens[..separator]);
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
    assert_eq!(live.len(), 2, "only #605 canonical surfaces are adopted");
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
        let verdict = scan_live(&text, lock_present);
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
