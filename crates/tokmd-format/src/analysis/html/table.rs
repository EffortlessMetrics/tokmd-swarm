//! Table-row rendering for analysis HTML reports.

use std::fmt::Write;

use super::format::{escape_html, format_number};
use tokmd_analysis_types::AnalysisReceipt;

pub(super) fn build_table_rows(receipt: &AnalysisReceipt) -> String {
    // Estimate output capacity per rendered row, following summary renderers.
    let row_count = receipt
        .derived
        .as_ref()
        .map_or(0, |derived| derived.top.largest_lines.len().min(100));
    let mut rows = String::with_capacity(row_count * 512);

    if let Some(derived) = &receipt.derived {
        for row in derived.top.largest_lines.iter().take(100) {
            let _ = write!(
                rows,
                r#"<tr><td class="path" data-path="{path}">{path}</td><td data-module="{module}">{module}</td><td data-lang="{lang}"><span class="lang-badge">{lang}</span></td><td class="num" data-lines="{lines}">{lines_fmt}</td><td class="num" data-code="{code}">{code_fmt}</td><td class="num" data-tokens="{tokens}">{tokens_fmt}</td><td class="num" data-bytes="{bytes}">{bytes_fmt}</td></tr>"#,
                path = escape_html(&row.path),
                module = escape_html(&row.module),
                lang = escape_html(&row.lang),
                lines = row.lines,
                lines_fmt = format_number(row.lines),
                code = row.code,
                code_fmt = format_number(row.code),
                tokens = row.tokens,
                tokens_fmt = format_number(row.tokens),
                bytes = row.bytes,
                bytes_fmt = format_number(row.bytes),
            );
        }
    }

    rows
}
