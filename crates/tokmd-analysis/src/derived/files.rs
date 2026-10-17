use tokmd_analysis_types::{FileStatRow, MaxFileReport, MaxFileRow, TopOffenders};
use tokmd_analysis_types::{empty_file_row, path_depth};
use tokmd_scan::safe_ratio;
use tokmd_types::FileRow;

const TOP_N: usize = 10;
const MIN_DOC_LINES: usize = 50;
const MIN_DENSE_LINES: usize = 10;

pub(crate) fn to_stat_row(r: &FileRow) -> FileStatRow {
    FileStatRow {
        path: r.path.clone(),
        module: r.module.clone(),
        lang: r.lang.clone(),
        code: r.code,
        comments: r.comments,
        blanks: r.blanks,
        lines: r.lines,
        bytes: r.bytes,
        tokens: r.tokens,
        doc_pct: if r.code + r.comments == 0 {
            None
        } else {
            Some(safe_ratio(r.comments, r.code + r.comments))
        },
        bytes_per_line: if r.lines == 0 {
            None
        } else {
            Some(safe_ratio(r.bytes, r.lines))
        },
        depth: path_depth(&r.path),
    }
}

pub(super) fn build_max_file_report(rows: &[&FileRow]) -> MaxFileReport {
    let overall = rows
        .iter()
        .copied()
        .max_by(|a, b| a.lines.cmp(&b.lines).then_with(|| a.path.cmp(&b.path)))
        .map(to_stat_row)
        .unwrap_or_else(empty_file_row);

    let mut by_lang: std::collections::BTreeMap<&str, &FileRow> = std::collections::BTreeMap::new();
    let mut by_module: std::collections::BTreeMap<&str, &FileRow> =
        std::collections::BTreeMap::new();

    for row in rows {
        if let Some(existing) = by_lang.get_mut(row.lang.as_str()) {
            if row.lines > existing.lines
                || (row.lines == existing.lines && row.path < existing.path)
            {
                *existing = *row;
            }
        } else {
            by_lang.insert(row.lang.as_str(), *row);
        }

        if let Some(existing) = by_module.get_mut(row.module.as_str()) {
            if row.lines > existing.lines
                || (row.lines == existing.lines && row.path < existing.path)
            {
                *existing = *row;
            }
        } else {
            by_module.insert(row.module.as_str(), *row);
        }
    }

    MaxFileReport {
        overall,
        by_lang: by_lang
            .into_iter()
            .map(|(key, file)| MaxFileRow {
                key: key.to_string(),
                file: to_stat_row(file),
            })
            .collect(),
        by_module: by_module
            .into_iter()
            .map(|(key, file)| MaxFileRow {
                key: key.to_string(),
                file: to_stat_row(file),
            })
            .collect(),
    }
}

pub(super) fn build_top_offenders(rows: &[&FileRow]) -> TopOffenders {
    let by_lines = select_top(rows.iter().copied(), |a, b| {
        b.lines.cmp(&a.lines).then_with(|| a.path.cmp(&b.path))
    });

    let by_tokens = select_top(rows.iter().copied(), |a, b| {
        b.tokens.cmp(&a.tokens).then_with(|| a.path.cmp(&b.path))
    });

    let by_bytes = select_top(rows.iter().copied(), |a, b| {
        b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path))
    });

    let least_doc = select_top(
        rows.iter().copied().filter(|r| r.lines >= MIN_DOC_LINES),
        |a, b| {
            let a_doc = if a.code + a.comments == 0 {
                0.0
            } else {
                safe_ratio(a.comments, a.code + a.comments)
            };
            let b_doc = if b.code + b.comments == 0 {
                0.0
            } else {
                safe_ratio(b.comments, b.code + b.comments)
            };
            a_doc
                .partial_cmp(&b_doc)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.lines.cmp(&a.lines))
                .then_with(|| a.path.cmp(&b.path))
        },
    );

    let dense = select_top(
        rows.iter().copied().filter(|r| r.lines >= MIN_DENSE_LINES),
        |a, b| {
            let a_rate = if a.lines == 0 {
                0.0
            } else {
                safe_ratio(a.bytes, a.lines)
            };
            let b_rate = if b.lines == 0 {
                0.0
            } else {
                safe_ratio(b.bytes, b.lines)
            };
            b_rate
                .partial_cmp(&a_rate)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path))
        },
    );

    TopOffenders {
        largest_lines: by_lines.into_iter().map(to_stat_row).collect(),
        largest_tokens: by_tokens.into_iter().map(to_stat_row).collect(),
        largest_bytes: by_bytes.into_iter().map(to_stat_row).collect(),
        least_documented: least_doc.into_iter().map(to_stat_row).collect(),
        most_dense: dense.into_iter().map(to_stat_row).collect(),
    }
}

fn select_top<'a>(
    mut rows: impl Iterator<Item = &'a FileRow>,
    compare: impl Fn(&FileRow, &FileRow) -> std::cmp::Ordering,
) -> Vec<&'a FileRow> {
    let Some(first) = rows.next() else {
        return Vec::new();
    };
    let mut top: Vec<&FileRow> = Vec::with_capacity(TOP_N);
    top.push(first);
    for row in rows {
        if top.len() == TOP_N
            && top
                .last()
                .is_some_and(|last| compare(row, last) != std::cmp::Ordering::Less)
        {
            continue;
        }
        // Insert after equal keys so the old stable-sort input order survives,
        // including distinct rows with equal metrics and equal paths.
        let position =
            top.partition_point(|existing| compare(existing, row) != std::cmp::Ordering::Greater);
        if top.len() == TOP_N {
            top.pop();
        }
        top.insert(position, row);
    }
    top
}

#[cfg(test)]
#[path = "files/tests.rs"]
mod tests;
