use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask workspace parent")
        .to_path_buf()
}

fn normalize(path: &str) -> String {
    path.trim_start_matches("./")
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
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
    let cargo = tokens
        .iter()
        .position(|token| *token == "cargo" || token.strip_suffix("/cargo").is_some())?;
    (cargo + 1 < tokens.len()).then(|| tokens[cargo..].to_vec())
}

fn governed_command<'a>(tokens: &'a [&'a str]) -> Option<&'a str> {
    let command = *tokens.get(1)?;
    matches!(
        command,
        "build" | "check" | "test" | "clippy" | "run" | "install" | "update" | "generate-lockfile"
    )
    .then_some(command)
}

fn dynamic_line(line: &str) -> bool {
    ["$(", "${", "{{", "format!", "Command::new", "env!("]
        .iter()
        .any(|marker| line.contains(marker))
}

fn scan_live(text: &str, lock_present: bool) -> Verdict {
    for line in text.lines() {
        let Some(tokens) = command_tokens(line) else {
            continue;
        };
        let Some(command) = governed_command(&tokens) else {
            continue;
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
        if !tokens
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

fn load_inventory() -> Inventory {
    toml::from_str(POLICY).expect("cargo command surface policy must parse")
}

fn candidate_files(root: &Path, inventory: &Inventory) -> Vec<String> {
    let mut files = BTreeSet::new();
    for candidate in &inventory.candidate_roots {
        let path = root.join(candidate);
        if path.is_file() {
            files.insert(normalize(candidate));
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        for entry in WalkDir::new(path).follow_links(false).into_iter().flatten() {
            if entry.file_type().is_file() {
                if let Ok(relative) = entry.path().strip_prefix(root) {
                    files.insert(normalize(&relative.to_string_lossy()));
                }
            }
        }
    }
    files.into_iter().collect()
}

#[test]
fn policy_is_closed_and_live_surfaces_are_locked() {
    let inventory = load_inventory();
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

    let live = inventory
        .surface
        .iter()
        .filter(|surface| surface.mode == Mode::Live)
        .collect::<Vec<_>>();
    assert_eq!(live.len(), 2, "only #605 canonical surfaces are adopted");
    assert!(live.iter().all(|surface| !surface.reason.is_empty()));

    let root = workspace_root();
    let lock_present = root.join("Cargo.lock").is_file();
    assert!(lock_present, "the checked workspace must have Cargo.lock");
    let mut unclassified = Vec::new();
    let mut violations = Vec::new();
    for path in candidate_files(&root, &inventory) {
        let Some(surface) = surface_for(&inventory, &path) else {
            unclassified.push(path);
            continue;
        };
        if surface.mode != Mode::Live {
            continue;
        }
        let text = fs::read_to_string(root.join(&path)).expect("live surface must be readable");
        if scan_live(&text, lock_present) != Verdict::Pass {
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
}

#[test]
fn scanner_preserves_historical_deferred_and_dynamic_boundaries() {
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

    let inventory = load_inventory();
    assert_eq!(
        classify(&inventory, "docs/examples/real-user-path-smoke-run.md"),
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
}
