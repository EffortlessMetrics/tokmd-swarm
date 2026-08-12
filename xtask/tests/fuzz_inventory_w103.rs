use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn workspace_root() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(PathBuf::from)
        .context("xtask should have a workspace parent")
}

fn manifest_targets(body: &str) -> Result<BTreeSet<String>, String> {
    let value: toml::Value = toml::from_str(body).map_err(|err| err.to_string())?;
    let bins = value
        .get("bin")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "fuzz manifest must declare [[bin]] targets".to_string())?;
    let mut targets = BTreeSet::new();

    for (index, bin) in bins.iter().enumerate() {
        let name = bin
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| format!("fuzz manifest bin {} is missing a string name", index + 1))?;
        if !name.starts_with("fuzz_") {
            return Err(format!(
                "fuzz manifest target {name:?} must start with fuzz_"
            ));
        }
        if !targets.insert(name.to_string()) {
            return Err(format!("duplicate fuzz manifest target {name:?}"));
        }
    }

    Ok(targets)
}

fn workflow_targets(body: &str) -> Result<BTreeSet<String>, String> {
    let mut targets = BTreeSet::new();

    for (line_index, raw_line) in body.lines().enumerate() {
        let line = raw_line.trim();
        let Some(name) = line.strip_prefix("- target:") else {
            continue;
        };
        let name = name.trim().trim_matches(['\'', '"']);
        if name.is_empty() || !name.starts_with("fuzz_") {
            return Err(format!(
                "fuzz workflow line {} has malformed target {name:?}",
                line_index + 1
            ));
        }
        if !targets.insert(name.to_string()) {
            return Err(format!("duplicate fuzz workflow target {name:?}"));
        }
    }

    if targets.is_empty() {
        return Err("fuzz workflow must schedule at least one target".to_string());
    }
    Ok(targets)
}

fn readme_targets(body: &str) -> Result<BTreeMap<String, bool>, String> {
    let mut targets = BTreeMap::new();

    for (line_index, line) in body.lines().enumerate() {
        if !line.starts_with("| `fuzz_") {
            continue;
        }
        let columns: Vec<_> = line.split('|').map(str::trim).collect();
        if columns.len() != 7 {
            return Err(format!(
                "fuzz README line {} must have five data columns",
                line_index + 1
            ));
        }
        let name = columns
            .get(1)
            .ok_or_else(|| format!("fuzz README line {} is missing a target", line_index + 1))?
            .trim_matches('`');
        let scheduled = match columns.get(3).copied() {
            Some("yes") => true,
            Some("no") => false,
            Some(value) => {
                return Err(format!(
                    "fuzz README line {} has invalid nightly value {value:?}; use yes or no",
                    line_index + 1
                ));
            }
            None => {
                return Err(format!(
                    "fuzz README line {} is missing a nightly value",
                    line_index + 1
                ));
            }
        };
        if targets.insert(name.to_string(), scheduled).is_some() {
            return Err(format!("duplicate fuzz README target {name:?}"));
        }
    }

    if targets.is_empty() {
        return Err("fuzz README must document at least one target".to_string());
    }
    Ok(targets)
}

#[test]
fn fuzz_manifest_readme_and_nightly_inventory_are_aligned() -> Result<()> {
    let root = workspace_root()?;
    let manifest_body = std::fs::read_to_string(root.join("fuzz/Cargo.toml"))
        .context("fuzz manifest should be readable")?;
    let workflow_body = std::fs::read_to_string(root.join(".github/workflows/fuzz.yml"))
        .context("fuzz workflow should be readable")?;
    let readme_body = std::fs::read_to_string(root.join("fuzz/README.md"))
        .context("fuzz README should be readable")?;

    let manifest = manifest_targets(&manifest_body).map_err(anyhow::Error::msg)?;
    let workflow = workflow_targets(&workflow_body).map_err(anyhow::Error::msg)?;
    let readme = readme_targets(&readme_body).map_err(anyhow::Error::msg)?;
    let documented: BTreeSet<_> = readme.keys().cloned().collect();
    let documented_as_scheduled: BTreeSet<_> = readme
        .iter()
        .filter_map(|(name, scheduled)| scheduled.then_some(name.clone()))
        .collect();

    ensure!(
        documented == manifest,
        "fuzz/README.md must document every fuzz/Cargo.toml target exactly once"
    );
    ensure!(
        workflow.is_subset(&manifest),
        "nightly workflow targets must be declared in fuzz/Cargo.toml: {:?}",
        workflow.difference(&manifest).collect::<Vec<_>>()
    );
    ensure!(
        documented_as_scheduled == workflow,
        "fuzz/README.md nightly status must match .github/workflows/fuzz.yml"
    );
    Ok(())
}

#[test]
fn manifest_parser_rejects_missing_and_duplicate_names() -> Result<()> {
    let missing = "[[bin]]\npath = 'fuzz_targets/missing.rs'\n";
    ensure!(manifest_targets(missing).is_err());

    let duplicate = "[[bin]]\nname = 'fuzz_one'\n[[bin]]\nname = 'fuzz_one'\n";
    ensure!(manifest_targets(duplicate).is_err());
    Ok(())
}

#[test]
fn workflow_parser_rejects_malformed_and_duplicate_targets() -> Result<()> {
    ensure!(workflow_targets("- target: not-a-fuzzer\n").is_err());
    ensure!(workflow_targets("- target: fuzz_one\n- target: fuzz_one\n").is_err());
    Ok(())
}

#[test]
fn readme_parser_rejects_invalid_status_and_duplicate_targets() -> Result<()> {
    let invalid = "| `fuzz_one` | `one` | sometimes | bytes | description |\n";
    ensure!(readme_targets(invalid).is_err());

    let duplicate = concat!(
        "| `fuzz_one` | `one` | yes | bytes | description |\n",
        "| `fuzz_one` | `one` | no | bytes | description |\n",
    );
    ensure!(readme_targets(duplicate).is_err());
    Ok(())
}
