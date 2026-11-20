//! Integration tests for `build_complexity_report`.
//!
//! Each test writes source files to a temp directory and feeds matching
//! `ExportData` rows to exercise the end-to-end complexity pipeline.

use std::fs;
use std::path::PathBuf;

use crate::complexity::build_complexity_report;
use tokmd_analysis_types::AnalysisLimits;
use tokmd_analysis_types::ComplexityRisk;
use tokmd_types::{ChildIncludeMode, ExportData, FileKind, FileRow};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_row(path: &str, module: &str, lang: &str, code: usize) -> FileRow {
    FileRow {
        path: path.to_string(),
        module: module.to_string(),
        lang: lang.to_string(),
        kind: FileKind::Parent,
        code,
        comments: 0,
        blanks: 0,
        lines: code,
        bytes: code * 40,
        tokens: code * 8,
    }
}

fn make_export(rows: Vec<FileRow>) -> ExportData {
    ExportData {
        rows,
        module_roots: vec![],
        module_depth: 1,
        children: ChildIncludeMode::Separate,
    }
}

fn default_limits() -> AnalysisLimits {
    AnalysisLimits::default()
}

fn write_temp_files(files: &[(&str, &str)]) -> (tempfile::TempDir, Vec<PathBuf>) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let mut paths = Vec::new();
    for (rel, content) in files {
        let full = dir.path().join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
        paths.push(PathBuf::from(rel));
    }
    (dir, paths)
}

// ===========================================================================
// Scenario: Empty input
// ===========================================================================

#[test]
fn given_no_files_report_has_zero_totals() {
    let dir = tempfile::tempdir().unwrap();
    let export = make_export(vec![]);
    let report =
        build_complexity_report(dir.path(), &[], &export, &default_limits(), false).unwrap();

    assert_eq!(report.total_functions, 0);
    assert_eq!(report.avg_cyclomatic, 0.0);
    assert_eq!(report.max_cyclomatic, 0);
    assert_eq!(report.high_risk_files, 0);
    assert!(report.files.is_empty());
}

// ===========================================================================
// Scenario: Single Rust file with simple functions
// ===========================================================================

#[test]
fn given_rust_file_detects_functions_and_cyclomatic() {
    let code = "\
fn simple() {
    println!(\"hello\");
}

pub fn with_branch(x: i32) -> &'static str {
    if x > 0 {
        \"positive\"
    } else {
        \"non-positive\"
    }
}
";
    let (dir, paths) = write_temp_files(&[("src/lib.rs", code)]);
    let export = make_export(vec![make_row("src/lib.rs", "src", "Rust", 12)]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    assert_eq!(report.total_functions, 2);
    assert!(
        report.max_cyclomatic >= 2,
        "branching should add complexity"
    );
    assert_eq!(report.files.len(), 1);
    assert_eq!(report.files[0].function_count, 2);
}

// ===========================================================================
// Scenario: Function details when detail_functions=true
// ===========================================================================

#[test]
fn given_detail_flag_then_function_details_present() {
    let code = "\
fn alpha() {
    let x = 1;
}

fn beta(a: i32, b: i32) {
    if a > b {
        println!(\"a\");
    }
}
";
    let (dir, paths) = write_temp_files(&[("lib.rs", code)]);
    let export = make_export(vec![make_row("lib.rs", ".", "Rust", 10)]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), true).unwrap();

    assert_eq!(report.files.len(), 1);
    let functions = report.files[0].functions.as_ref().expect("details present");
    assert_eq!(functions.len(), 2);

    let names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
}

// ===========================================================================
// Scenario: detail_functions=false omits function details
// ===========================================================================

#[test]
fn given_no_detail_flag_then_function_details_none() {
    let code = "fn foo() { let x = 1; }\n";
    let (dir, paths) = write_temp_files(&[("lib.rs", code)]);
    let export = make_export(vec![make_row("lib.rs", ".", "Rust", 1)]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    assert!(report.files[0].functions.is_none());
}

// ===========================================================================
// Scenario: Unsupported language is skipped
// ===========================================================================

#[test]
fn given_unsupported_lang_file_is_skipped() {
    let code = "# Markdown heading\nSome text.\n";
    let (dir, paths) = write_temp_files(&[("README.md", code)]);
    let export = make_export(vec![make_row("README.md", ".", "Markdown", 2)]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    assert_eq!(report.total_functions, 0);
    assert!(report.files.is_empty());
}

// ===========================================================================
// Scenario: Child rows are excluded
// ===========================================================================

#[test]
fn given_child_rows_they_are_excluded() {
    let code = "pub fn visible() {}\n";
    let (dir, paths) = write_temp_files(&[("lib.rs", code)]);
    let mut row = make_row("lib.rs", ".", "Rust", 1);
    row.kind = FileKind::Child;
    let export = make_export(vec![row]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    assert_eq!(report.total_functions, 0);
    assert!(report.files.is_empty());
}

// ===========================================================================
// Scenario: Multiple files sorted by complexity descending
// ===========================================================================

#[test]
fn given_multiple_files_sorted_by_complexity_desc() {
    let simple = "fn a() { let x = 1; }\n";
    let complex = "\
fn branchy(x: i32) -> i32 {
    if x > 0 {
        if x > 10 {
            for i in 0..x {
                if i % 2 == 0 {
                    println!(\"{}\", i);
                }
            }
            42
        } else {
            x
        }
    } else {
        0
    }
}
";
    let (dir, paths) = write_temp_files(&[("src/simple.rs", simple), ("src/complex.rs", complex)]);
    let export = make_export(vec![
        make_row("src/simple.rs", "src", "Rust", 1),
        make_row("src/complex.rs", "src", "Rust", 18),
    ]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    assert_eq!(report.files.len(), 2);
    // First file should have higher complexity
    assert!(
        report.files[0].cyclomatic_complexity >= report.files[1].cyclomatic_complexity,
        "files should be sorted by cyclomatic desc"
    );
}

// ===========================================================================
// Scenario: Histogram is included
// ===========================================================================

#[test]
fn given_files_then_histogram_is_present() {
    let code = "fn f() { let x = 1; }\n";
    let (dir, paths) = write_temp_files(&[("lib.rs", code)]);
    let export = make_export(vec![make_row("lib.rs", ".", "Rust", 1)]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    let hist = report.histogram.as_ref().expect("histogram present");
    assert_eq!(hist.total, 1);
    assert_eq!(hist.counts.iter().sum::<u32>(), 1);
}

// ===========================================================================
// Scenario: Python file analysis
// ===========================================================================

#[test]
fn given_python_file_detects_functions() {
    let code = "\
def greet(name):
    if name:
        print(f'Hello, {name}!')
    else:
        print('Hello!')

def add(a, b):
    return a + b
";
    let (dir, paths) = write_temp_files(&[("main.py", code)]);
    let export = make_export(vec![make_row("main.py", ".", "Python", 8)]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    assert_eq!(report.total_functions, 2);
    assert!(report.max_cyclomatic >= 2);
}

// ===========================================================================
// Scenario: Go file analysis
// ===========================================================================

#[test]
fn given_go_file_detects_functions() {
    let code = "\
package main

func hello() {
    fmt.Println(\"hello\")
}

func decide(x int) int {
    if x > 0 {
        return x
    }
    return 0
}
";
    let (dir, paths) = write_temp_files(&[("main.go", code)]);
    let export = make_export(vec![make_row("main.go", ".", "Go", 12)]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    assert_eq!(report.total_functions, 2);
    assert!(report.max_cyclomatic >= 2);
}

// ===========================================================================
// Scenario: Max bytes limit stops scanning early
// ===========================================================================

#[test]
fn given_max_bytes_limit_scanning_stops() {
    let code_a = "fn a() { let x = 1; }\n";
    let code_b = "fn b() { let y = 2; }\nfn c() { let z = 3; }\n";
    let (dir, paths) = write_temp_files(&[("a.rs", code_a), ("b.rs", code_b)]);
    let export = make_export(vec![
        make_row("a.rs", ".", "Rust", 1),
        make_row("b.rs", ".", "Rust", 2),
    ]);
    let limits = AnalysisLimits {
        max_bytes: Some(code_a.len() as u64),
        ..Default::default()
    };
    let report = build_complexity_report(dir.path(), &paths, &export, &limits, false).unwrap();

    // Only the first file should be scanned
    assert!(
        report.files.len() <= 1,
        "expected at most 1 file, got {}",
        report.files.len()
    );
}

// ===========================================================================
// Scenario: Aggregates are consistent
// ===========================================================================

#[test]
fn given_files_then_aggregates_are_consistent() {
    let code_a = "\
fn one() {
    if true { println!(\"a\"); }
}
";
    let code_b = "\
fn two() {
    for i in 0..10 {
        if i > 5 {
            println!(\"{}\", i);
        }
    }
}

fn three() {
    match 1 {
        1 => println!(\"one\"),
        _ => println!(\"other\"),
    }
}
";
    let (dir, paths) = write_temp_files(&[("a.rs", code_a), ("b.rs", code_b)]);
    let export = make_export(vec![
        make_row("a.rs", ".", "Rust", 3),
        make_row("b.rs", ".", "Rust", 16),
    ]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    // total_functions is sum across files
    let fn_sum: usize = report.files.iter().map(|f| f.function_count).sum();
    assert_eq!(report.total_functions, fn_sum);

    // max_cyclomatic >= avg_cyclomatic
    assert!(report.max_cyclomatic as f64 >= report.avg_cyclomatic);

    // high_risk_files <= files.len()
    assert!(report.high_risk_files <= report.files.len());
}

// ===========================================================================
// Scenario: Risk classification
// ===========================================================================

#[test]
fn given_simple_function_risk_is_low() {
    let code = "fn simple() { let x = 1; }\n";
    let (dir, paths) = write_temp_files(&[("lib.rs", code)]);
    let export = make_export(vec![make_row("lib.rs", ".", "Rust", 1)]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    assert_eq!(report.files[0].risk_level, ComplexityRisk::Low);
}

// ===========================================================================
// Scenario: Technical debt ratio
// ===========================================================================

#[test]
fn given_files_with_code_then_technical_debt_present() {
    let code = "\
fn complex(x: i32) -> i32 {
    if x > 0 {
        if x > 10 {
            for i in 0..x {
                if i % 2 == 0 {
                    return i;
                }
            }
        }
    }
    0
}
";
    let (dir, paths) = write_temp_files(&[("lib.rs", code)]);
    let export = make_export(vec![make_row("lib.rs", ".", "Rust", 12)]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    let debt = report
        .technical_debt
        .as_ref()
        .expect("debt should be present");
    assert!(debt.ratio > 0.0);
    assert!(debt.complexity_points > 0);
    assert!(debt.code_kloc > 0.0);
}

// ===========================================================================
// Scenario: Cognitive complexity is populated
// ===========================================================================

#[test]
fn given_branchy_code_cognitive_complexity_populated() {
    let code = "\
fn branchy(x: i32) -> i32 {
    if x > 0 {
        if x > 10 {
            42
        } else {
            x
        }
    } else {
        0
    }
}
";
    let (dir, paths) = write_temp_files(&[("lib.rs", code)]);
    let export = make_export(vec![make_row("lib.rs", ".", "Rust", 12)]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    assert!(
        report.files[0].cognitive_complexity.is_some(),
        "cognitive complexity should be populated for branchy code"
    );
}

// ===========================================================================
// Scenario: JavaScript file analysis
// ===========================================================================

#[test]
fn given_javascript_file_detects_functions() {
    let code = "\
function greet(name) {
    if (name) {
        console.log('Hello, ' + name);
    }
}

function add(a, b) {
    return a + b;
}
";
    let (dir, paths) = write_temp_files(&[("index.js", code)]);
    let export = make_export(vec![make_row("index.js", ".", "JavaScript", 10)]);
    let report =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false).unwrap();

    assert_eq!(report.total_functions, 2);
    assert!(report.avg_cyclomatic > 0.0);
}

// ===========================================================================
// Regression: issue #219 avg/max cyclomatic share per-function units
// ===========================================================================

#[test]
#[expect(
    clippy::assertions_on_constants,
    reason = "test failure path when build_complexity_report returns Err"
)]
fn given_many_functions_and_one_branchy_fn_avg_le_max_cyclomatic() {
    let mut code = String::from(
        r#"
fn branchy(x: i32, y: bool) -> i32 {
    if x > 0 {
        if y {
            for i in 0..x {
                if i % 2 == 0 {
                    return i;
                }
            }
        }
    }
    match x {
        0 => 0,
        1 => 1,
        _ => x,
    }
}
"#,
    );
    for idx in 0..40 {
        code.push_str(&format!("fn trivial_{idx}() {{ let _ = {idx}; }}\n"));
    }

    let (dir, paths) = write_temp_files(&[("src/main.rs", &code)]);
    let export = make_export(vec![make_row(
        "src/main.rs",
        "src",
        "Rust",
        code.lines().count(),
    )]);
    if let Ok(report) =
        build_complexity_report(dir.path(), &paths, &export, &default_limits(), false)
    {
        assert!(
            report.total_functions > 10,
            "fixture should detect many functions"
        );
        if let Some(file) = report
            .files
            .iter()
            .max_by_key(|entry| entry.cyclomatic_complexity)
        {
            assert!(
                file.cyclomatic_complexity > file.function_count,
                "file total cyclomatic should exceed per-function max for this fixture"
            );
        }

        assert!(
            report.avg_cyclomatic <= report.max_cyclomatic as f64,
            "avg ({}) must be <= max ({}) in shared per-function units",
            report.avg_cyclomatic,
            report.max_cyclomatic
        );
        assert!(
            report.avg_cyclomatic < report.max_cyclomatic as f64 * 2.0,
            "avg should stay near per-function scale, not file-total scale"
        );
    } else {
        assert!(false, "build_complexity_report failed");
    }
}

#[test]
fn coverage_discloses_failed_read_and_recovers_on_retry() -> anyhow::Result<()> {
    let (dir, mut paths) = write_temp_files(&[("simple.rs", "fn simple() {}\n")]);
    paths.push(PathBuf::from("branching.rs"));
    let export = make_export(vec![
        make_row("simple.rs", ".", "Rust", 1),
        make_row("branching.rs", ".", "Rust", 4),
    ]);
    let measure = || {
        let report =
            build_complexity_report(dir.path(), &paths, &export, &default_limits(), false)?;
        serde_json::to_value(report).map_err(anyhow::Error::from)
    };
    let partial = measure()?;
    assert_eq!(partial["coverage"]["status"], "partial");
    assert_eq!(partial["coverage"]["eligible_files"], 2);
    assert_eq!(partial["coverage"]["analyzed_files"], 1);
    assert_eq!(partial["coverage"]["analyzed_lines"], 1);
    assert_eq!(partial["coverage"]["outcomes"][0]["path"], "branching.rs");
    assert_eq!(partial["coverage"]["outcomes"][0]["outcome"], "unreadable");
    assert_eq!(measure()?, partial);

    fs::write(
        dir.path().join("branching.rs"),
        "fn branching(x: bool, y: bool) {\n    if x { return; }\n    if y { return; }\n}\n",
    )?;
    let complete = measure()?;
    assert_eq!(complete["coverage"]["status"], "complete");
    assert_eq!(complete["coverage"]["analyzed_files"], 2);
    assert_eq!(complete["coverage"]["analyzed_lines"], 5);
    assert_eq!(complete["max_cyclomatic"], 3);
    Ok(())
}

#[test]
fn coverage_reconciles_unsupported_clipped_and_unselected_files() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    fs::write(dir.path().join("binary.rs"), b"\0")?;
    fs::write(dir.path().join("invalid.rs"), b"\xff")?;
    fs::write(
        dir.path().join("clipped.rs"),
        "fn clipped() {\n    if true { return; }\n}\n",
    )?;
    let export = make_export(vec![
        make_row("./unselected.rs", ".", "Rust", 8),
        make_row("notes.md", ".", "Markdown", 1),
        make_row("invalid.rs", ".", "Rust", 3),
        make_row("clipped.rs", ".", "Rust", 3),
        make_row("binary.rs", ".", "Rust", 1),
        make_row("after.rs", ".", "Rust", 1),
    ]);
    let selected = ["binary.rs", "invalid.rs", "clipped.rs", "after.rs"].map(PathBuf::from);
    let limits = AnalysisLimits {
        max_bytes: Some(16),
        max_file_bytes: Some(14),
        max_files: Some(4),
        ..default_limits()
    };
    let report = build_complexity_report(dir.path(), &selected, &export, &limits, true)?;
    let json = serde_json::to_value(report)?;
    assert_eq!(
        json["coverage"],
        serde_json::json!({
            "status": "partial",
            "eligible_files": 5,
            "analyzed_files": 1,
            "analyzed_lines": 1,
            "outcomes": [
                {"path":"after.rs","outcome":"byte_limit","truncated_by":null,"omitted_bytes":0,"analyzed_lines":0},
                {"path":"binary.rs","outcome":"unsupported_content","truncated_by":null,"omitted_bytes":0,"analyzed_lines":0},
                {"path":"clipped.rs","outcome":"analyzed","truncated_by":"file_byte_limit","omitted_bytes":27,"analyzed_lines":1},
                {"path":"invalid.rs","outcome":"unsupported_content","truncated_by":null,"omitted_bytes":0,"analyzed_lines":0},
                {"path":"notes.md","outcome":"unsupported_language","truncated_by":null,"omitted_bytes":0,"analyzed_lines":0},
                {"path":"unselected.rs","outcome":"not_selected","truncated_by":null,"omitted_bytes":0,"analyzed_lines":0}
            ]
        })
    );
    Ok(())
}

#[test]
fn coverage_for_unselected_inventory_has_no_grade() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let export = make_export(vec![make_row("a.rs", ".", "Rust", 4)]);
    let limits = AnalysisLimits {
        max_files: Some(0),
        ..default_limits()
    };
    let report = build_complexity_report(dir.path(), &[], &export, &limits, false)?;
    assert!(report.maintainability_index.is_none());
    assert!(report.technical_debt.is_none());
    let json = serde_json::to_value(report)?;
    assert_eq!(json["coverage"]["status"], "unavailable");
    assert_eq!(json["coverage"]["eligible_files"], 1);
    assert_eq!(json["coverage"]["analyzed_files"], 0);
    assert_eq!(json["coverage"]["outcomes"][0]["outcome"], "not_selected");
    Ok(())
}

#[test]
fn coverage_distinguishes_exact_file_cap_from_clip_and_zero_cap() -> anyhow::Result<()> {
    let source = "fn a() {}\n";
    let (dir, paths) = write_temp_files(&[("a.rs", source)]);
    let export = make_export(vec![make_row("a.rs", ".", "Rust", 1)]);
    let measure = |cap| {
        let limits = AnalysisLimits {
            max_file_bytes: Some(cap),
            ..default_limits()
        };
        let report = build_complexity_report(dir.path(), &paths, &export, &limits, false)?;
        serde_json::to_value(report).map_err(anyhow::Error::from)
    };

    let exact = measure(source.len() as u64)?;
    assert_eq!(exact["coverage"]["status"], "complete");
    assert_eq!(
        exact["coverage"]["outcomes"][0]["truncated_by"],
        serde_json::Value::Null
    );
    let zero = measure(0)?;
    assert_eq!(zero["coverage"]["status"], "unavailable");
    assert_eq!(zero["coverage"]["analyzed_files"], 0);
    assert_eq!(
        zero["coverage"]["outcomes"][0]["outcome"],
        "file_byte_limit"
    );
    assert_eq!(
        zero["coverage"]["outcomes"][0]["truncated_by"],
        "file_byte_limit"
    );
    assert!(zero.get("maintainability_index").is_none());
    assert!(zero.get("technical_debt").is_none());
    Ok(())
}

#[test]
fn coverage_clips_the_final_sample_to_the_remaining_total_budget() -> anyhow::Result<()> {
    let source = "fn a() {\n    if true { return; }\n}\n";
    let (dir, paths) = write_temp_files(&[("a.rs", source)]);
    let export = make_export(vec![make_row("a.rs", ".", "Rust", 3)]);
    let measure = |cap| {
        let limits = AnalysisLimits {
            max_bytes: Some(cap),
            ..default_limits()
        };
        let report = build_complexity_report(dir.path(), &paths, &export, &limits, false)?;
        let warnings = report.coverage.as_ref().map_or_else(Vec::new, |coverage| {
            super::super::complexity_warnings(coverage, &limits)
        });
        Ok::<_, anyhow::Error>((serde_json::to_value(report)?, warnings))
    };

    let (partial, warnings) = measure(9)?;
    assert_eq!(partial["coverage"]["status"], "partial");
    assert_eq!(partial["coverage"]["analyzed_lines"], 1);
    assert_eq!(
        partial["coverage"]["outcomes"][0]["truncated_by"],
        "total_byte_limit"
    );
    assert_eq!(
        partial["coverage"]["outcomes"][0]["omitted_bytes"],
        source.len() - 9
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("max_bytes=9"))
    );
    let (exact, warnings) = measure(source.len() as u64)?;
    assert_eq!(exact["coverage"]["status"], "complete");
    assert_eq!(exact["coverage"]["analyzed_lines"], 3);
    assert!(warnings.is_empty());
    Ok(())
}

#[test]
fn debt_is_invariant_to_unmeasured_lines() {
    let mut code = String::new();
    for index in 0..120 {
        code.push_str(&format!("fn f{index}() {{\n}}\n"));
    }
    code.push_str(&"const _: () = ();\n".repeat(760));
    let extra = "value\n".repeat(19_000);
    let (dir, paths) = write_temp_files(&[("lib.rs", &code), ("extra.txt", &extra)]);
    let rows = [
        make_row("lib.rs", ".", "Rust", 1000),
        make_row("extra.txt", ".", "Plain Text", 19_000),
    ];
    for count in [1, 2] {
        let report = build_complexity_report(
            dir.path(),
            &paths[..count],
            &make_export(rows[..count].to_vec()),
            &default_limits(),
            false,
        )
        .unwrap();
        assert_eq!(report.coverage.unwrap().analyzed_lines, 1000);
        let debt = report.technical_debt.unwrap();
        assert_eq!(debt.complexity_points, 120);
        assert_eq!(debt.ratio, 120.0);
        assert_eq!(debt.code_kloc, 1.0);
        assert_eq!(
            debt.level,
            tokmd_analysis_types::TechnicalDebtLevel::Critical
        );
    }
}
