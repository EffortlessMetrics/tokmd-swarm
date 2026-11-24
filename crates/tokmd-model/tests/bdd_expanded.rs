//! Expanded BDD-style scenarios for `tokmd-model`.
//!
//! These tests exercise aggregation, file-row construction, and
//! structural invariants not covered by the base `bdd.rs`:
//! - Lang row aggregation totals (sum invariants)
//! - Module key forward-slash normalization
//! - File row construction and field consistency
//! - Sorting invariants on export data
//! - Top-N "Other" bucket creation
//! - Export filtering (min_code, max_rows)
//! - Embedded language handling via collect_file_rows
//! - unique_parent_file_count accuracy

use std::path::{Path, PathBuf};

use tokei::{Config, Languages};
use tokmd_model::{
    avg, collect_file_rows, create_export_data, create_lang_report, create_module_report,
    normalize_path, unique_parent_file_count,
};
use tokmd_types::{ChildIncludeMode, ChildrenMode, FileKind};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn scan_dir(path: &str) -> Languages {
    let mut languages = Languages::new();
    languages.get_statistics(&[PathBuf::from(path)], &[], &Config::default());
    languages
}

fn crate_src() -> String {
    format!("{}/src", env!("CARGO_MANIFEST_DIR"))
}

/// Use a relative tokei report so module roots and separator spelling are
/// independent of the directory containing the checkout.
fn native_report_fixture() -> (Languages, &'static str) {
    let module = if cfg!(unix) { r"app\core" } else { "app" };
    let source = Path::new(".").join("crates").join(module).join("lib.rs");
    let mut language = tokei::Language::new();
    language.reports.push(tokei::Report::new(source));
    let mut languages = Languages::new();
    languages.insert(tokei::LanguageType::Rust, language);
    (languages, module)
}

/// Scan the entire workspace (provides richer multi-language data).
fn workspace_src() -> String {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

// ===========================================================================
// Scenario group: lang row aggregation totals
// ===========================================================================

#[test]
fn given_lang_report_then_total_code_equals_sum_of_row_codes() -> std::io::Result<()> {
    let langs = scan_dir(&crate_src());
    let report = create_lang_report(&langs, 0, false, ChildrenMode::Collapse)?;

    let sum: usize = report.rows.iter().map(|r| r.code).sum();
    assert_eq!(
        report.total.code, sum,
        "total.code must equal sum of row codes"
    );
    Ok(())
}

#[test]
fn given_lang_report_then_total_lines_equals_sum_of_row_lines() -> std::io::Result<()> {
    let langs = scan_dir(&crate_src());
    let report = create_lang_report(&langs, 0, false, ChildrenMode::Collapse)?;

    let sum: usize = report.rows.iter().map(|r| r.lines).sum();
    assert_eq!(
        report.total.lines, sum,
        "total.lines must equal sum of row lines"
    );
    Ok(())
}

#[test]
fn given_lang_report_then_total_bytes_equals_sum_of_row_bytes() -> std::io::Result<()> {
    let langs = scan_dir(&crate_src());
    let report = create_lang_report(&langs, 0, false, ChildrenMode::Collapse)?;

    let sum: usize = report.rows.iter().map(|r| r.bytes).sum();
    assert_eq!(
        report.total.bytes, sum,
        "total.bytes must equal sum of row bytes"
    );
    Ok(())
}

#[test]
fn given_lang_report_then_total_tokens_equals_sum_of_row_tokens() -> std::io::Result<()> {
    let langs = scan_dir(&crate_src());
    let report = create_lang_report(&langs, 0, false, ChildrenMode::Collapse)?;

    let sum: usize = report.rows.iter().map(|r| r.tokens).sum();
    assert_eq!(
        report.total.tokens, sum,
        "total.tokens must equal sum of row tokens"
    );
    Ok(())
}

// ===========================================================================
// Scenario group: module key forward-slash normalization
// ===========================================================================

#[test]
fn given_native_file_then_module_key_joins_native_directory_names() -> std::io::Result<()> {
    let (languages, module) = native_report_fixture();
    let report = create_module_report(
        &languages,
        &["crates".into()],
        2,
        ChildIncludeMode::ParentsOnly,
        0,
    )?;
    let keys: Vec<_> = report.rows.into_iter().map(|row| row.module).collect();
    assert_eq!(keys, [format!("crates/{module}")]);
    Ok(())
}

#[test]
fn given_module_report_with_roots_then_matching_paths_have_depth_segments() -> std::io::Result<()> {
    let langs = scan_dir(&workspace_src());
    let report = create_module_report(
        &langs,
        &["crates".into()],
        2,
        ChildIncludeMode::ParentsOnly,
        0,
    )?;

    let crate_modules: Vec<_> = report
        .rows
        .iter()
        .filter(|r| r.module.starts_with("crates/"))
        .collect();

    for m in &crate_modules {
        let segments: Vec<_> = m.module.split('/').collect();
        assert_eq!(
            segments.len(),
            2,
            "module '{}' should have 2 segments at depth=2",
            m.module
        );
    }
    Ok(())
}

// ===========================================================================
// Scenario group: file row construction
// ===========================================================================

#[test]
fn given_file_rows_then_every_row_has_nonempty_path_and_lang() -> std::io::Result<()> {
    let langs = scan_dir(&crate_src());
    let rows = collect_file_rows(&langs, &[], 2, ChildIncludeMode::ParentsOnly, None)?;

    for row in &rows {
        assert!(!row.path.is_empty(), "file row path must not be empty");
        assert!(!row.lang.is_empty(), "file row lang must not be empty");
    }
    Ok(())
}

#[test]
fn given_file_rows_then_lines_equals_code_plus_comments_plus_blanks() -> std::io::Result<()> {
    let langs = scan_dir(&crate_src());
    let rows = collect_file_rows(&langs, &[], 2, ChildIncludeMode::ParentsOnly, None)?;

    for row in &rows {
        let expected = row.code + row.comments + row.blanks;
        assert_eq!(
            row.lines, expected,
            "lines must equal code+comments+blanks for '{}'",
            row.path
        );
    }
    Ok(())
}

#[test]
fn given_parent_file_rows_then_bytes_are_positive() -> std::io::Result<()> {
    let langs = scan_dir(&crate_src());
    let rows = collect_file_rows(&langs, &[], 2, ChildIncludeMode::ParentsOnly, None)?;

    for row in &rows {
        assert!(
            row.bytes > 0,
            "parent file '{}' should have positive bytes",
            row.path
        );
        assert!(
            row.tokens > 0,
            "parent file '{}' should have positive tokens",
            row.path
        );
    }
    Ok(())
}

#[test]
fn given_native_file_then_row_path_joins_native_names_with_slashes() -> std::io::Result<()> {
    let (languages, module) = native_report_fixture();
    let rows = collect_file_rows(&languages, &[], 2, ChildIncludeMode::ParentsOnly, None)?;
    let paths: Vec<_> = rows.into_iter().map(|row| row.path).collect();
    assert_eq!(paths, [format!("crates/{module}/lib.rs")]);
    Ok(())
}

// ===========================================================================
// Scenario group: sorting invariants
// ===========================================================================

#[test]
fn given_export_data_then_rows_sorted_desc_code_then_asc_path() -> std::io::Result<()> {
    let langs = scan_dir(&workspace_src());
    let data = create_export_data(
        &langs,
        &["crates".into()],
        2,
        ChildIncludeMode::ParentsOnly,
        None,
        0,
        0,
    )?;

    for pair in data.rows.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        assert!(
            a.code > b.code || (a.code == b.code && a.path <= b.path),
            "export sort violation: ({}, {}) vs ({}, {})",
            a.path,
            a.code,
            b.path,
            b.code
        );
    }
    Ok(())
}

#[test]
fn given_module_report_then_rows_sorted_desc_code_then_asc_module() -> std::io::Result<()> {
    let langs = scan_dir(&workspace_src());
    let report = create_module_report(
        &langs,
        &["crates".into()],
        2,
        ChildIncludeMode::ParentsOnly,
        0,
    )?;

    for pair in report.rows.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        assert!(
            a.code > b.code || (a.code == b.code && a.module <= b.module),
            "module sort violation: ({}, {}) vs ({}, {})",
            a.module,
            a.code,
            b.module,
            b.code
        );
    }
    Ok(())
}

// ===========================================================================
// Scenario group: embedded language handling
// ===========================================================================

#[test]
fn given_separate_mode_then_child_rows_have_zero_bytes() -> std::io::Result<()> {
    let langs = scan_dir(&workspace_src());
    let rows = collect_file_rows(&langs, &[], 2, ChildIncludeMode::Separate, None)?;

    for row in rows.iter().filter(|r| r.kind == FileKind::Child) {
        assert_eq!(
            row.bytes, 0,
            "child row for '{}' lang '{}' must have 0 bytes",
            row.path, row.lang
        );
        assert_eq!(
            row.tokens, 0,
            "child row for '{}' lang '{}' must have 0 tokens",
            row.path, row.lang
        );
    }
    Ok(())
}

#[test]
fn given_parents_only_mode_then_no_child_kind_rows() -> std::io::Result<()> {
    let langs = scan_dir(&workspace_src());
    let rows = collect_file_rows(&langs, &[], 2, ChildIncludeMode::ParentsOnly, None)?;

    let child_count = rows.iter().filter(|r| r.kind == FileKind::Child).count();
    assert_eq!(child_count, 0, "ParentsOnly mode must not emit Child rows");
    Ok(())
}

#[test]
fn given_collapse_vs_separate_lang_report_then_total_files_match() -> std::io::Result<()> {
    let langs = scan_dir(&crate_src());

    let collapse = create_lang_report(&langs, 0, false, ChildrenMode::Collapse)?;
    let separate = create_lang_report(&langs, 0, false, ChildrenMode::Separate)?;

    assert_eq!(
        collapse.total.files, separate.total.files,
        "total files must agree between Collapse and Separate modes"
    );
    Ok(())
}

// ===========================================================================
// Scenario group: top-N "Other" bucket
// ===========================================================================

#[test]
fn given_top_1_lang_report_then_other_bucket_created() -> std::io::Result<()> {
    let langs = scan_dir(&workspace_src());
    let report = create_lang_report(&langs, 1, false, ChildrenMode::Collapse)?;

    assert!(
        report.rows.len() <= 2,
        "top=1 should produce at most 2 rows (1 + Other)"
    );
    if report.rows.len() == 2 {
        assert_eq!(
            report.rows[1].lang, "Other",
            "second row should be 'Other' bucket"
        );
    }
    Ok(())
}

#[test]
fn given_top_n_lang_report_then_other_bucket_totals_are_correct() -> std::io::Result<()> {
    let langs = scan_dir(&workspace_src());
    let full = create_lang_report(&langs, 0, false, ChildrenMode::Collapse)?;
    let top2 = create_lang_report(&langs, 2, false, ChildrenMode::Collapse)?;

    if full.rows.len() > 2 {
        let other = top2.rows.iter().find(|r| r.lang == "Other");
        assert!(other.is_some(), "should have Other bucket with top=2");

        let expected_other_code: usize = full.rows[2..].iter().map(|r| r.code).sum();
        assert_eq!(
            other.unwrap().code,
            expected_other_code,
            "Other bucket code must equal sum of truncated rows"
        );
    }
    Ok(())
}

// ===========================================================================
// Scenario group: export filtering
// ===========================================================================

#[test]
fn given_min_code_filter_then_low_code_files_excluded() -> std::io::Result<()> {
    let langs = scan_dir(&workspace_src());
    let data = create_export_data(
        &langs,
        &[],
        2,
        ChildIncludeMode::ParentsOnly,
        None,
        50, // min_code = 50
        0,
    )?;

    for row in &data.rows {
        assert!(
            row.code >= 50,
            "file '{}' has {} code lines, should be >= 50",
            row.path,
            row.code
        );
    }
    Ok(())
}

#[test]
fn given_max_rows_then_output_truncated() -> std::io::Result<()> {
    let langs = scan_dir(&workspace_src());
    let data = create_export_data(
        &langs,
        &[],
        2,
        ChildIncludeMode::ParentsOnly,
        None,
        0,
        5, // max_rows = 5
    )?;

    assert!(
        data.rows.len() <= 5,
        "max_rows=5 should produce at most 5 rows, got {}",
        data.rows.len()
    );
    Ok(())
}

// ===========================================================================
// Scenario group: unique_parent_file_count accuracy
// ===========================================================================

#[test]
fn given_scanned_crate_then_unique_file_count_matches_report_count() -> std::io::Result<()> {
    let langs = scan_dir(&crate_src());
    let count = unique_parent_file_count(&langs)?;

    let mut paths = std::collections::BTreeSet::new();
    for lang in langs.values() {
        for report in &lang.reports {
            paths.insert(normalize_path(&report.name, None));
        }
    }

    assert_eq!(
        count,
        paths.len(),
        "unique_parent_file_count must match deduplicated report paths"
    );
    Ok(())
}

// ===========================================================================
// Scenario group: normalize_path edge cases
// ===========================================================================

#[test]
fn given_windows_backslash_prefix_then_strip_and_normalize() {
    let result = normalize_path(
        Path::new("C:\\Code\\Repo\\src\\main.rs"),
        Some(Path::new("C:\\Code\\Repo")),
    );
    assert_eq!(result, "src/main.rs");
}

#[test]
fn given_dot_slash_prefix_in_both_path_and_strip_then_normalized() {
    let result = normalize_path(
        Path::new("./crates/foo/src/lib.rs"),
        Some(Path::new("./crates/foo")),
    );
    assert_eq!(result, "src/lib.rs");
}

#[test]
fn given_plain_filename_then_normalize_returns_as_is() {
    let result = normalize_path(Path::new("lib.rs"), None);
    assert_eq!(result, "lib.rs");
}

// ===========================================================================
// Scenario group: avg edge cases for aggregation
// ===========================================================================

#[test]
fn given_large_values_then_avg_does_not_overflow() {
    let result = avg(usize::MAX - 1, 2);
    assert!(result > 0, "avg of large values should not be zero");
}

#[test]
fn given_equal_lines_and_files_then_avg_equals_one_line_per_file() {
    assert_eq!(avg(10, 10), 1);
}
