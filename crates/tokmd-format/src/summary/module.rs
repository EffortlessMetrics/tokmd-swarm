//! Module summary table rendering.
//!
//! This module owns Markdown and TSV rendering for `ModuleReport`. The parent
//! summary module keeps command dispatch and public helper exports.

use std::fmt::Write as FmtWrite;

use tokmd_types::ModuleReport;

// Encode table delimiters and HTML/Markdown escapes as character references.
// References are decoded after the Markdown table is parsed, keeping the path
// in one cell even when it contains a pipe or a line break.
fn escape_module_cell(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        escaped.push_str(match ch {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '|' => "&#124;",
            '`' => "&#96;",
            '\\' => "&#92;",
            '\n' => "&#10;",
            '\r' => "&#13;",
            _ => {
                escaped.push(ch);
                continue;
            }
        });
    }
    escaped
}

pub(super) fn render_module_md(report: &ModuleReport) -> String {
    // Heuristic: (rows + 3) * 80 chars per row
    let mut s = String::with_capacity((report.rows.len() + 3) * 80);
    s.push_str("|Module|Code|Lines|Files|Bytes|Tokens|Avg|\n");
    s.push_str("|---|---:|---:|---:|---:|---:|---:|\n");
    for r in &report.rows {
        let _ = writeln!(
            s,
            "|{}|{}|{}|{}|{}|{}|{}|",
            escape_module_cell(&r.module),
            r.code,
            r.lines,
            r.files,
            r.bytes,
            r.tokens,
            r.avg_lines
        );
    }
    let _ = writeln!(
        s,
        "|**Total**|{}|{}|{}|{}|{}|{}|",
        report.total.code,
        report.total.lines,
        report.total.files,
        report.total.bytes,
        report.total.tokens,
        report.total.avg_lines
    );
    s
}

pub(super) fn render_module_tsv(report: &ModuleReport) -> String {
    // Heuristic: (rows + 2) * 64 chars per row
    let mut s = String::with_capacity((report.rows.len() + 2) * 64);
    s.push_str("Module\tCode\tLines\tFiles\tBytes\tTokens\tAvg\n");
    for r in &report.rows {
        let _ = writeln!(
            s,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.module, r.code, r.lines, r.files, r.bytes, r.tokens, r.avg_lines
        );
    }
    let _ = writeln!(
        s,
        "Total\t{}\t{}\t{}\t{}\t{}\t{}",
        report.total.code,
        report.total.lines,
        report.total.files,
        report.total.bytes,
        report.total.tokens,
        report.total.avg_lines
    );
    s
}
