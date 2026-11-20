use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::maintainability::compute_maintainability_index;
use anyhow::Result;
#[cfg(test)]
use tokmd_analysis_types::ComplexityRisk;
#[cfg(test)]
use tokmd_analysis_types::TechnicalDebtLevel;
use tokmd_analysis_types::{
    ComplexityCoverage, ComplexityCoverageStatus, ComplexityFileObservation, ComplexityFileOutcome,
    ComplexityReport, ComplexityTruncation, FileComplexity,
};
use tokmd_types::{ExportData, FileKind};

use tokmd_analysis_types::{AnalysisLimits, normalize_path};

mod debt;
mod details;
mod functions;
mod histogram;
mod language;
mod math;
mod risk;
mod summary;

use debt::{average_parent_loc, compute_technical_debt_ratio};
use details::extract_function_details;
#[cfg(test)]
use details::{
    detect_fn_spans_c_style, detect_fn_spans_go, detect_fn_spans_js, detect_fn_spans_python,
    detect_fn_spans_rust,
};
use functions::count_functions;
#[cfg(test)]
use functions::{count_python_functions, count_rust_functions, is_rust_fn_start};
#[cfg(test)]
pub(crate) use histogram::generate_complexity_histogram;
pub(crate) use histogram::generate_complexity_histogram_for_files;
use language::{is_complexity_lang, map_language_for_complexity};
use risk::{classify_risk_extended, estimate_cyclomatic};
use summary::summarize_file_complexities;

const DEFAULT_MAX_FILE_BYTES: u64 = 128 * 1024;
const MAX_COMPLEXITY_FILES: usize = 100;

#[cfg(test)]
#[path = "tests.rs"]
mod moved_tests;
#[cfg(test)]
#[path = "tests/unit.rs"]
mod unit_tests;

/// Build a complexity report by analyzing function counts, lengths, cyclomatic and cognitive complexity.
pub(crate) fn build_complexity_report(
    root: &Path,
    files: &[PathBuf],
    export: &ExportData,
    limits: &AnalysisLimits,
    detail_functions: bool,
) -> Result<ComplexityReport> {
    let mut observations: BTreeMap<_, _> = export
        .rows
        .iter()
        .filter(|row| row.kind == FileKind::Parent)
        .map(|row| {
            let path = normalize_path(&row.path, root);
            let outcome = if is_complexity_lang(&row.lang) {
                ComplexityFileOutcome::NotSelected
            } else {
                ComplexityFileOutcome::UnsupportedLanguage
            };
            (
                path.clone(),
                (
                    row,
                    ComplexityFileObservation {
                        path,
                        outcome,
                        truncated_by: None,
                        omitted_bytes: 0,
                        analyzed_lines: 0,
                    },
                ),
            )
        })
        .collect();

    let mut file_complexities: Vec<FileComplexity> = Vec::new();
    let mut per_file_max_cyclomatic: Vec<usize> = Vec::new();
    let mut total_bytes = 0u64;
    let max_total = limits.max_bytes;
    let per_file_limit = limits.max_file_bytes.unwrap_or(DEFAULT_MAX_FILE_BYTES);

    for rel in files {
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let Some((row, observation)) = observations.get_mut(&rel_str) else {
            continue;
        };
        if observation.outcome != ComplexityFileOutcome::NotSelected {
            continue;
        }
        let remaining = max_total.map_or(u64::MAX, |limit| limit.saturating_sub(total_bytes));
        if remaining == 0 {
            observation.outcome = ComplexityFileOutcome::ByteLimit;
            continue;
        }
        let (cap, cap_reason) = if remaining < per_file_limit {
            (remaining, ComplexityTruncation::TotalByteLimit)
        } else {
            (per_file_limit, ComplexityTruncation::FileByteLimit)
        };
        let cap = cap.min(usize::MAX as u64) as usize;
        let path = root.join(rel);
        let (bytes, omitted_bytes) = match crate::content::io::read_head_with_omission(&path, cap) {
            Ok(observation) => observation,
            Err(_) => {
                observation.outcome = ComplexityFileOutcome::Unreadable;
                continue;
            }
        };
        observation.omitted_bytes = omitted_bytes;
        if omitted_bytes > 0 {
            observation.truncated_by = Some(cap_reason);
            if cap == 0 {
                observation.outcome = ComplexityFileOutcome::FileByteLimit;
                continue;
            }
        }
        total_bytes += bytes.len() as u64;

        let Some(text) = crate::content::io::as_text(&bytes) else {
            observation.outcome = ComplexityFileOutcome::UnsupportedContent;
            continue;
        };
        observation.outcome = ComplexityFileOutcome::Analyzed;
        observation.analyzed_lines = text.lines().count();
        let lang_mapped = map_language_for_complexity(&row.lang);
        let (function_count, max_function_length) = count_functions(&row.lang, text);
        let cyclomatic_analysis =
            crate::content::complexity::estimate_cyclomatic_complexity(text, lang_mapped);
        let (cyclomatic, max_function_cyclomatic) = if cyclomatic_analysis.function_count > 0 {
            (cyclomatic_analysis.total_cc, cyclomatic_analysis.max_cc)
        } else if function_count > 0 {
            let file_level = estimate_cyclomatic(&row.lang, text);
            (file_level, file_level)
        } else {
            (0, 0)
        };

        // Compute cognitive complexity and nesting depth
        let cognitive_result =
            crate::content::complexity::estimate_cognitive_complexity(text, lang_mapped);
        let nesting_result = crate::content::complexity::analyze_nesting_depth(text, lang_mapped);

        let cognitive_complexity = if cognitive_result.function_count > 0 {
            Some(cognitive_result.total)
        } else {
            None
        };
        let max_nesting = if nesting_result.max_depth > 0 {
            Some(nesting_result.max_depth)
        } else {
            None
        };

        let risk_level = classify_risk_extended(
            function_count,
            max_function_length,
            max_function_cyclomatic,
            cognitive_complexity,
            max_nesting,
        );

        let functions = if detail_functions {
            Some(extract_function_details(&row.lang, text))
        } else {
            None
        };

        per_file_max_cyclomatic.push(max_function_cyclomatic);
        file_complexities.push(FileComplexity {
            path: rel_str,
            module: row.module.clone(),
            function_count,
            max_function_length,
            cyclomatic_complexity: cyclomatic,
            cognitive_complexity,
            max_nesting,
            risk_level,
            functions,
        });
    }

    let outcomes: Vec<_> = observations
        .into_values()
        .map(|(_, observation)| observation)
        .collect();
    let eligible_files = outcomes
        .iter()
        .filter(|entry| entry.outcome != ComplexityFileOutcome::UnsupportedLanguage)
        .count();
    let analyzed_lines = outcomes.iter().map(|entry| entry.analyzed_lines).sum();
    let analyzed_files = file_complexities.len();
    let status = if analyzed_lines == 0 {
        ComplexityCoverageStatus::Unavailable
    } else if outcomes.iter().any(|entry| {
        entry.outcome != ComplexityFileOutcome::Analyzed || entry.truncated_by.is_some()
    }) {
        ComplexityCoverageStatus::Partial
    } else {
        ComplexityCoverageStatus::Complete
    };

    // Sort by cyclomatic complexity descending, then by path
    file_complexities.sort_by(|a, b| {
        b.cyclomatic_complexity
            .cmp(&a.cyclomatic_complexity)
            .then_with(|| a.path.cmp(&b.path))
    });

    // Compute aggregates before truncating
    let summary = summarize_file_complexities(&file_complexities, &per_file_max_cyclomatic);

    // Generate histogram from all files before truncating
    let histogram =
        generate_complexity_histogram_for_files(&file_complexities, &per_file_max_cyclomatic, 5);

    // Compute maintainability index
    let maintainability_index = if status == ComplexityCoverageStatus::Unavailable {
        None
    } else {
        average_parent_loc(export).and_then(|avg_loc| {
            compute_maintainability_index(summary.avg_cyclomatic, avg_loc, None)
        })
    };
    let technical_debt = if status == ComplexityCoverageStatus::Unavailable {
        None
    } else {
        compute_technical_debt_ratio(export, &file_complexities)
    };

    // Only keep top files by complexity
    file_complexities.truncate(MAX_COMPLEXITY_FILES);

    Ok(ComplexityReport {
        coverage: Some(ComplexityCoverage {
            status,
            eligible_files,
            analyzed_files,
            analyzed_lines,
            outcomes,
        }),
        total_functions: summary.total_functions,
        avg_function_length: summary.avg_function_length,
        max_function_length: summary.max_function_length,
        avg_cyclomatic: summary.avg_cyclomatic,
        max_cyclomatic: summary.max_cyclomatic,
        avg_cognitive: summary.avg_cognitive,
        max_cognitive: summary.max_cognitive,
        avg_nesting_depth: summary.avg_nesting_depth,
        max_nesting_depth: summary.max_nesting_depth,
        high_risk_files: summary.high_risk_files,
        histogram: Some(histogram),
        halstead: None, // Populated when halstead feature is enabled
        maintainability_index,
        technical_debt,
        files: file_complexities,
    })
}

pub(crate) fn complexity_warnings(
    coverage: &ComplexityCoverage,
    limits: &AnalysisLimits,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let file_clipped = coverage
        .outcomes
        .iter()
        .filter(|entry| entry.truncated_by == Some(ComplexityTruncation::FileByteLimit))
        .count();
    if file_clipped > 0 {
        let limit_label = limits.max_file_bytes.map_or_else(
            || format!("default max_file_bytes={DEFAULT_MAX_FILE_BYTES}"),
            |limit| format!("max_file_bytes={limit}"),
        );
        let eligible = coverage.eligible_files;
        warnings.push(format!(
            "complexity scan bounded: {file_clipped} of {eligible} eligible file(s) exceed {limit_label}; complexity metrics are partial"
        ));
    }
    if let Some(max_bytes) = limits.max_bytes
        && coverage.outcomes.iter().any(|entry| {
            entry.outcome == ComplexityFileOutcome::ByteLimit
                || entry.truncated_by == Some(ComplexityTruncation::TotalByteLimit)
        })
    {
        warnings.push(format!(
            "complexity scan bounded: max_bytes={max_bytes} clipped or skipped eligible source; complexity metrics are partial"
        ));
    }
    let omitted_bytes = coverage.outcomes.iter().fold(0u64, |total, entry| {
        total.saturating_add(entry.omitted_bytes)
    });
    if omitted_bytes > 0 {
        warnings.push(format!(
            "complexity scan bounded: at least {omitted_bytes} byte(s) of eligible source were outside the scanned content window"
        ));
    }
    if let Some(max_files) = limits.max_files
        && coverage
            .outcomes
            .iter()
            .any(|entry| entry.outcome == ComplexityFileOutcome::NotSelected)
    {
        warnings.push(format!(
            "complexity scan bounded: eligible files were outside the selected inventory (max_files={max_files})"
        ));
    }
    match coverage.status {
        ComplexityCoverageStatus::Complete => {}
        ComplexityCoverageStatus::Partial => warnings.push(format!(
            "complexity measurement is partial ({} of {} eligible files analyzed); see complexity.coverage for file outcomes",
            coverage.analyzed_files, coverage.eligible_files
        )),
        ComplexityCoverageStatus::Unavailable => {
            warnings.push("complexity measurement is unavailable; see complexity.coverage for file outcomes".into());
        }
    }
    warnings
}
