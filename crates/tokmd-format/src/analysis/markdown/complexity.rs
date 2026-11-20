//! Complexity Markdown rendering.
//!
//! This module owns the complexity metrics table and top-complex-file detail
//! rendering for analysis Markdown output.

use std::fmt::Write;

use super::fmt_f64;
use tokmd_analysis_types::{ComplexityCoverageStatus, ComplexityReport};

pub(super) fn render_complexity_report(out: &mut String, cx: &ComplexityReport) {
    out.push_str("## Complexity\n\n");
    if let Some(coverage) = &cx.coverage {
        let status = match coverage.status {
            ComplexityCoverageStatus::Complete => "complete",
            ComplexityCoverageStatus::Partial => "partial",
            ComplexityCoverageStatus::Unavailable => "unavailable",
        };
        let _ = writeln!(
            out,
            "Coverage is {status}. Analyzed {} of {} eligible files across {} inventory paths ({} input lines).\n",
            coverage.analyzed_files,
            coverage.eligible_files,
            coverage.outcomes.len(),
            coverage.analyzed_lines
        );
        if coverage.status == ComplexityCoverageStatus::Unavailable {
            return;
        }
    }
    out.push_str("|Metric|Value|\n");
    out.push_str("|---|---:|\n");
    let _ = writeln!(out, "|Total functions|{}|", cx.total_functions);
    let _ = writeln!(
        out,
        "|Avg function length|{}|",
        fmt_f64(cx.avg_function_length, 1)
    );
    let _ = writeln!(out, "|Max function length|{}|", cx.max_function_length);
    let _ = writeln!(out, "|Avg cyclomatic|{}|", fmt_f64(cx.avg_cyclomatic, 2));
    let _ = writeln!(out, "|Max cyclomatic|{}|", cx.max_cyclomatic);
    if let Some(cog) = cx.avg_cognitive {
        let _ = writeln!(out, "|Avg cognitive|{}|", fmt_f64(cog, 2));
    }
    if let Some(cog) = cx.max_cognitive {
        let _ = writeln!(out, "|Max cognitive|{}|", cog);
    }
    if let Some(avg_nesting) = cx.avg_nesting_depth {
        let _ = writeln!(out, "|Avg nesting depth|{}|", fmt_f64(avg_nesting, 2));
    }
    if let Some(max_nesting) = cx.max_nesting_depth {
        let _ = writeln!(out, "|Max nesting depth|{}|", max_nesting);
    }
    if let Some(debt) = cx
        .technical_debt
        .as_ref()
        .filter(|d| d.analyzed_lines.is_some())
    {
        let _ = writeln!(
            out,
            "|Debt points per thousand analyzed lines|{}|",
            debt.ratio
        );
    }
    let _ = writeln!(out, "|High risk files|{}|\n", cx.high_risk_files);

    if !cx.files.is_empty() {
        out.push_str("### Top complex files\n\n");
        out.push_str("|Path|CC|Functions|Max fn length|\n");
        out.push_str("|---|---:|---:|---:|\n");
        for f in cx.files.iter().take(10) {
            let _ = writeln!(
                out,
                "|{}|{}|{}|{}|",
                f.path, f.cyclomatic_complexity, f.function_count, f.max_function_length
            );
        }
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_debt_with_coverage_keeps_its_original_meaning() {
        let mut receipt: serde_json::Value = serde_json::from_str(r#"{
            "coverage":{"status":"partial","eligible_files":1,"analyzed_files":1,"analyzed_lines":1000,"outcomes":[]},
            "total_functions":1,"avg_function_length":1.0,"max_function_length":1,
            "avg_cyclomatic":1.0,"max_cyclomatic":1,"high_risk_files":0,"files":[],
            "technical_debt":{"ratio":6.0,"complexity_points":120,"code_kloc":20.0,"level":"low"}
        }"#).unwrap();
        let label = "Debt points per thousand analyzed lines";
        let mut output = String::new();
        render_complexity_report(
            &mut output,
            &serde_json::from_value(receipt.clone()).unwrap(),
        );
        assert!(!output.contains(label));
        receipt["technical_debt"] = serde_json::json!({
            "ratio":120.0,"complexity_points":120,"analyzed_lines":1000,"code_kloc":1.0,"level":"critical"
        });
        output.clear();
        render_complexity_report(&mut output, &serde_json::from_value(receipt).unwrap());
        assert!(output.contains("|Debt points per thousand analyzed lines|120|"));
    }
}
