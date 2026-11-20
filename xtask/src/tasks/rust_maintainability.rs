mod compare;
mod measure;
#[cfg(test)]
mod tests;

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{path::Path, process::Command};
use tokmd_analysis::ast::DEFAULT_MAX_SYNTAX_BYTES;

const BASE: &str = "07d878beee199568dbfa7698686ca1999addb7c8";
const POLICY: &str = "tokmd.rust_callable_pilot.v1";
const PATHS: [(&str, &str); 2] = [
    (
        "crates/tokmd-analysis/src/ast/panic_seam.rs",
        "ast::panic_seam",
    ),
    ("crates/tokmd-analysis/src/ast/rust.rs", "ast::rust"),
];
const BASELINE: &str = include_str!("../../../policy/rust-callable-baseline.json");
const LIMITS: Metrics = Metrics {
    parameters: 6,
    nesting: Some(4),
};
const OMITTED: &str = "Only the two listed source paths and admitted named Rust declarations are measured. Other paths, attributed containers/items except doc attributes, anonymous blocks' local items, closures, macro expansions, external modules and compiler-resolved identities are outside this pilot. Nesting is unavailable for bodies containing macros, closures, opaque parser tokens or async/const blocks, and excludes separately owned local items. No NLOC, complexity score, boolean count, type complexity, debt ratio or maintainability index.";

#[derive(Serialize, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Identity {
    owners: Vec<String>,
    name: String,
    signature: String,
}

#[derive(Serialize, Debug, Eq, PartialEq)]
struct Metrics {
    parameters: usize,
    nesting: Option<usize>,
}

#[derive(Serialize, Debug)]
struct Symbol {
    id: Identity,
    lines: [usize; 2],
    bytes: [usize; 2],
    fingerprint: String,
    metrics: Metrics,
}

#[derive(Serialize, Debug)]
struct Source {
    path: &'static str,
    module: &'static str,
    blob: Option<String>,
    status: &'static str,
    symbols: Vec<Symbol>,
}

#[derive(Serialize)]
struct Snapshot {
    schema: &'static str,
    commit: String,
    package: &'static str,
    language: &'static str,
    analysis: &'static str,
    analyzer_lock_blake3: String,
    max_source_bytes: usize,
    thresholds: Metrics,
    outside_measurement: &'static str,
    files: Vec<Source>,
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .current_dir(root)
        .arg("--no-replace-objects")
        .args(args)
        .output()?;
    ensure!(
        out.status.success(),
        "git {} failed: {}",
        args[0],
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(out.stdout)
}

fn snapshot(root: &Path, commit: &str) -> Result<Snapshot> {
    ensure!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "expected a full lowercase Git commit SHA"
    );
    ensure!(
        git(root, &["cat-file", "-t", commit])? == b"commit\n",
        "expected a commit object"
    );
    let files = PATHS
        .iter()
        .map(|&(path, module)| source(root, commit, path, module))
        .collect::<Result<_>>()?;
    Ok(Snapshot {
        schema: POLICY,
        commit: commit.into(),
        package: "tokmd-analysis",
        language: "Rust",
        analysis: "syn / quote / proc-macro2, syntax-only v1",
        analyzer_lock_blake3: blake3::hash(include_bytes!("../../../Cargo.lock"))
            .to_hex()
            .to_string(),
        max_source_bytes: DEFAULT_MAX_SYNTAX_BYTES,
        thresholds: LIMITS,
        outside_measurement: OMITTED,
        files,
    })
}

fn source(root: &Path, commit: &str, path: &'static str, module: &'static str) -> Result<Source> {
    let listing = git(root, &["ls-tree", "-l", "--full-tree", commit, "--", path])?;
    let mut source = Source {
        path,
        module,
        blob: None,
        status: "absent",
        symbols: vec![],
    };
    if listing.is_empty() {
        return Ok(source);
    }
    let entry = std::str::from_utf8(&listing)?;
    let (header, _) = entry.split_once('\t').context("invalid ls-tree record")?;
    let fields = header.split_ascii_whitespace().collect::<Vec<_>>();
    let [mode, kind, oid, size] = fields.as_slice() else {
        anyhow::bail!("invalid ls-tree header");
    };
    if *kind != "blob" || !matches!(*mode, "100644" | "100755") {
        source.status = "unsupported_entry";
        return Ok(source);
    }
    source.blob = Some((*oid).into());
    if size.parse::<u64>()? > DEFAULT_MAX_SYNTAX_BYTES as u64 {
        source.status = "size_limit";
        return Ok(source);
    }
    let bytes = git(root, &["cat-file", "blob", oid])?;
    let Ok(text) = std::str::from_utf8(&bytes) else {
        source.status = "invalid_utf8";
        return Ok(source);
    };
    match syn::parse_file(text) {
        Ok(ast) if !measure::admitted(&ast.attrs) => source.status = "excluded_file_attributes",
        Ok(ast) => {
            source.status = "parsed";
            let prefix = usize::from(text.starts_with('\u{feff}')) * 3
                + ast.shebang.as_ref().map_or(0, String::len);
            source.symbols = measure::symbols(&ast, prefix);
        }
        Err(_) => source.status = "parse_failed",
    }
    Ok(source)
}

fn baseline_json(baseline: &Snapshot) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(baseline)?))
}

pub fn run(candidate: Option<&str>, emit_baseline: bool, json: bool) -> Result<()> {
    let current = std::env::current_dir()?;
    let root = git(&current, &["rev-parse", "--show-toplevel"])?;
    let root = Path::new(std::str::from_utf8(&root)?.trim_end_matches(['\r', '\n']));
    let base = snapshot(root, BASE)?;
    let bytes = baseline_json(&base)?;
    if emit_baseline {
        print!("{bytes}");
        return Ok(());
    }
    ensure!(
        bytes == BASELINE,
        "committed baseline differs from pinned source and metric definitions"
    );
    let candidate = snapshot(root, candidate.context("missing candidate")?)?;
    let report = compare::comparison(&base, &candidate);
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        report.print();
    }
    Ok(())
}
