use super::{AstShadowCompareArgs, read_manifest_paths, run_with_root};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tokmd_analysis::ast::{
    RustLandmarkKind, SyntaxParseOptions, SyntaxParseStatus, parse_rust_landmarks,
    parse_syntax_receipt,
};

const CLEAN: &str = "fixtures/ast-shadow/rust/qualified-symbols.rs";
const DEGRADED: &str = "fixtures/ast-shadow/rust/parse-degraded.rs";
const MANIFEST: &str = "policy/ast-shadow-corpus.toml";
const CLEAN_BLAKE3: &str = "5654f86dca89894326c6d10346c1051df54e78e2b65a5ffbe518656930823186";
const DEGRADED_BLAKE3: &str = "b29a6285c211318be1d92d217cc04d429bd1b9f31cb4a113a0a37834e09f718d";

/// Literal Rust function oracle v1. Identity is lexical, local to the fixture.
/// Byte offsets are zero-based and end-exclusive. Lines and byte columns are
/// one-based, with an exclusive end column.
#[derive(Clone, Copy)]
struct ExpectedFunction {
    identity: &'static str,
    bytes: [usize; 2],
    span: [usize; 4],
}

const FUNCTIONS: [ExpectedFunction; 7] = [
    ExpectedFunction {
        identity: "scope::left::repeat",
        bytes: [23, 37],
        span: [1, 24, 1, 38],
    },
    ExpectedFunction {
        identity: "scope::right::repeat",
        bytes: [52, 66],
        span: [1, 53, 1, 67],
    },
    ExpectedFunction {
        identity: "<impl First>::repeat",
        bytes: [119, 138],
        span: [7, 5, 7, 24],
    },
    ExpectedFunction {
        identity: "<impl Second>::repeat",
        bytes: [160, 179],
        span: [11, 5, 11, 24],
    },
    ExpectedFunction {
        identity: "outer",
        bytes: [183, 282],
        span: [14, 1, 20, 2],
    },
    ExpectedFunction {
        identity: "outer::nested",
        bytes: [200, 214],
        span: [15, 5, 15, 19],
    },
    ExpectedFunction {
        identity: "outer::éclair",
        bytes: [231, 246],
        span: [16, 17, 16, 32],
    },
];

impl ExpectedFunction {
    fn name(self) -> &'static str {
        self.identity.rsplit("::").next().unwrap_or(self.identity)
    }

    fn line_key(self) -> (&'static str, usize, usize) {
        (self.name(), self.span[0], self.span[2])
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn check_fixture_bytes(path: &str, source: &str, expected_blake3: &str) -> Result<()> {
    let observed = blake3::hash(source.as_bytes());
    ensure!(
        observed.to_hex().as_str() == expected_blake3,
        "{path} source digest mismatch: {observed}; review the literal oracle"
    );
    Ok(())
}

fn check_functions(source: &str, expected: &[ExpectedFunction], crlf: bool) -> Result<()> {
    let shadow = parse_rust_landmarks(source)?;
    let receipt = parse_syntax_receipt(CLEAN, source, SyntaxParseOptions::default());
    anyhow::ensure!(!shadow.has_error && receipt.status == SyntaxParseStatus::Complete);

    let landmarks: Vec<_> = shadow
        .landmarks
        .iter()
        .filter(|fact| fact.kind == RustLandmarkKind::Function)
        .collect();
    let symbols: Vec<_> = receipt
        .facts
        .symbols
        .iter()
        .filter(|fact| fact.kind == "function")
        .collect();
    anyhow::ensure!(
        landmarks.len() == expected.len() && symbols.len() == expected.len(),
        "function counts differ: shadow {}, syntax {}, oracle {}",
        landmarks.len(),
        symbols.len(),
        expected.len()
    );

    for (fact, truth) in landmarks.iter().zip(expected) {
        let [start_line, _, end_line, _] = truth.span;
        let [mut start_byte, mut end_byte] = truth.bytes;
        if crlf {
            start_byte += start_line - 1;
            end_byte += end_line - 1;
        }
        anyhow::ensure!(
            (
                fact.name.as_str(),
                fact.start_byte,
                fact.end_byte,
                fact.start_line,
                fact.end_line
            ) == (truth.name(), start_byte, end_byte, start_line, end_line),
            "shadow name or range mismatch for {}: {fact:?}",
            truth.identity
        );
        anyhow::ensure!(
            symbols.iter().any(|symbol| {
                let span = symbol.span;
                symbol.name == truth.name()
                    && [
                        span.start_line,
                        span.start_column,
                        span.end_line,
                        span.end_column,
                    ] == truth.span
            }),
            "syntax name or range mismatch for {}",
            truth.identity
        );
    }

    Ok(())
}

#[test]
fn rust_function_oracle_anchors_ranges_and_detects_mutations() -> Result<()> {
    let source = fs::read_to_string(repo_root().join(CLEAN))?;
    check_fixture_bytes(CLEAN, &source, CLEAN_BLAKE3)?;
    check_functions(&source, &FUNCTIONS, false)?;
    check_functions(&source.replace('\n', "\r\n"), &FUNCTIONS, true)?;

    // A same-width enclosing-name edit leaves the emitted names and ranges
    // unchanged. The source digest still invalidates the lexical oracle.
    let renamed_scope = source.replacen("mod left", "mod lost", 1);
    check_functions(&renamed_scope, &FUNCTIONS, false)?;
    let error = check_fixture_bytes(CLEAN, &renamed_scope, CLEAN_BLAKE3)
        .expect_err("changed enclosing identity must invalidate the oracle");
    assert!(error.to_string().contains("source digest mismatch"));

    let mut wrong_name = FUNCTIONS;
    wrong_name[5].identity = "outer::other";
    let error = check_functions(&source, &wrong_name, false).expect_err("wrong name must fail");
    assert!(
        error
            .to_string()
            .contains("range mismatch for outer::other")
    );

    let mut wrong_range = FUNCTIONS;
    wrong_range[6].bytes[0] -= 1;
    let error =
        check_functions(&source, &wrong_range, false).expect_err("wrong byte range must fail");
    assert!(
        error
            .to_string()
            .contains("range mismatch for outer::éclair")
    );

    let mut wrong_column = FUNCTIONS;
    wrong_column[6].span[3] -= 1;
    let error =
        check_functions(&source, &wrong_column, false).expect_err("wrong UTF-8 column must fail");
    assert!(
        error
            .to_string()
            .contains("syntax name or range mismatch for outer::éclair")
    );
    Ok(())
}

type LineKey<'a> = (&'a str, usize, usize);

fn function_keys(entries: &Value) -> Result<Vec<LineKey<'_>>> {
    let mut keys = Vec::new();
    for entry in entries.as_array().context("expected landmark array")? {
        if entry["kind"] == "function" {
            keys.push((
                entry["name"].as_str().context("missing function name")?,
                serde_json::from_value(entry["start_line"].clone())?,
                serde_json::from_value(entry["end_line"].clone())?,
            ));
        }
    }
    keys.sort_unstable();
    Ok(keys)
}

#[test]
fn rust_function_receipts_report_curated_heuristic_errors() -> Result<()> {
    use std::collections::BTreeSet;

    let root = repo_root();
    let manifest_paths = read_manifest_paths(Path::new(MANIFEST), &root)?;
    for path in [CLEAN, DEGRADED] {
        assert!(manifest_paths.contains(&PathBuf::from(path)), "{path}");
    }
    let paths = [PathBuf::from(CLEAN), PathBuf::from(DEGRADED)];
    let output = tempfile::tempdir()?;
    for (run, paths) in [paths.to_vec(), paths.into_iter().rev().collect()]
        .into_iter()
        .enumerate()
    {
        run_with_root(
            AstShadowCompareArgs {
                paths,
                manifest: None,
                out: output.path().join(run.to_string()),
                summary_md: None,
                timing_json: None,
            },
            &root,
        )?;
    }
    let read = |file: &str| -> Result<Value> {
        let first = fs::read(output.path().join("0").join(file))?;
        let second = fs::read(output.path().join("1").join(file))?;
        assert_eq!(
            first, second,
            "{file} changed when input order was reversed"
        );
        Ok(serde_json::from_slice(&first)?)
    };
    let heuristic = read("heuristic.json")?;
    let ast = read("ast.json")?;
    let diff = read("diff.json")?;
    let clean_heuristic = &heuristic["files"][1];
    let clean_ast = &ast["files"][1];
    let clean_diff = &diff["files"][1];
    for file in [clean_heuristic, clean_ast, clean_diff] {
        assert_eq!(file["path"], CLEAN);
        assert_eq!(file["language"], "rust");
    }
    assert_eq!(clean_diff["status"], "compared");

    // The v1 diff collapses these two occurrences into a single line key.
    // Exclude that key when comparing either backend against the oracle.
    let ambiguous = ("repeat", 1, 1);
    let ast_keys = function_keys(&clean_ast["landmarks"])?;
    assert_eq!(ast_keys.iter().filter(|key| **key == ambiguous).count(), 2);
    let truth: BTreeSet<_> = FUNCTIONS
        .iter()
        .map(|fact| fact.line_key())
        .filter(|key| *key != ambiguous)
        .collect();
    let heuristic_keys = function_keys(&clean_heuristic["landmarks"])?;
    let comparable_heuristic: BTreeSet<_> = heuristic_keys
        .into_iter()
        .filter(|key| *key != ambiguous)
        .collect();
    let comparable_ast: BTreeSet<_> = ast_keys
        .into_iter()
        .filter(|key| *key != ambiguous)
        .collect();
    assert_eq!(comparable_ast, truth);
    assert_eq!(comparable_heuristic.intersection(&truth).count(), 4);
    assert_eq!(
        comparable_heuristic
            .difference(&truth)
            .copied()
            .collect::<Vec<_>>(),
        [("ghost", 18, 18)]
    );
    assert_eq!(
        truth
            .difference(&comparable_heuristic)
            .copied()
            .collect::<Vec<_>>(),
        [("éclair", 16, 16)]
    );

    let degraded = &diff["files"][0];
    assert_eq!(degraded["path"], DEGRADED);
    assert_eq!(degraded["status"], "parse_degraded");
    assert_eq!(
        function_keys(&ast["files"][0]["landmarks"])?,
        [("ok", 1, 1)]
    );
    let source = fs::read_to_string(root.join(DEGRADED))?;
    check_fixture_bytes(DEGRADED, &source, DEGRADED_BLAKE3)?;
    let receipt = parse_syntax_receipt(DEGRADED, &source, SyntaxParseOptions::default());
    assert_eq!(receipt.status, SyntaxParseStatus::ParseDegraded);
    assert!(receipt.has_error);
    assert_eq!(
        receipt
            .facts
            .symbols
            .iter()
            .filter(|fact| fact.kind == "function")
            .map(|fact| fact.name.as_str())
            .collect::<Vec<_>>(),
        ["ok"]
    );
    Ok(())
}

#[test]
fn rust_function_oracle_pins_parser_versions() -> Result<()> {
    let lock: toml::Value = toml::from_str(include_str!("../../../../Cargo.lock"))?;
    let packages = lock["package"].as_array().context("no lockfile packages")?;
    for (name, expected) in [("tree-sitter", "0.26.11"), ("tree-sitter-rust", "0.24.2")] {
        let versions: Vec<_> = packages
            .iter()
            .filter(|package| package["name"].as_str() == Some(name))
            .map(|package| package["version"].as_str())
            .collect();
        assert_eq!(
            versions,
            [Some(expected)],
            "review the Rust qualification corpus for {name}"
        );
    }
    Ok(())
}
