use super::*;
use std::{cell::Cell, cmp::Ordering};
use tokmd_types::FileKind;

fn row(i: usize) -> FileRow {
    let value = i.wrapping_mul(2654435761);
    let lines = [0, 9, 10, 49, 50, 51, 1000, 1001]
        .get(value % 8)
        .copied()
        .unwrap_or(0);
    let comments = value % (lines + 1);
    FileRow {
        path: format!("src/{:03}.rs", value % 71),
        module: format!("item{i}"),
        lang: "Rust".into(),
        kind: FileKind::Parent,
        code: lines - comments,
        comments,
        blanks: 0,
        lines,
        bytes: value % 100_000,
        tokens: value % 1000,
    }
}

#[test]
fn all_lists_match_stable_full_sort_at_cutoffs_and_thresholds() -> Result<(), serde_json::Error> {
    for n in [0, 1, 9, 10, 11, 64, 4096] {
        let mut rows: Vec<_> = (0..n).map(row).collect();
        for _ in 0..3 {
            let refs: Vec<_> = rows.iter().collect();
            assert_eq!(
                serde_json::to_value(build_top_offenders(&refs))?,
                serde_json::to_value(full_sort_oracle(&refs))?,
                "n={n}"
            );
            rows.reverse();
            if n > 1 {
                rows.rotate_left(n / 3);
            }
        }
    }
    Ok(())
}

#[test]
fn equal_keys_keep_the_first_ten_distinct_payloads() {
    let rows: Vec<_> = (0..32)
        .map(|i| FileRow {
            path: "same.rs".into(),
            module: format!("item{i}"),
            lang: "Rust".into(),
            kind: FileKind::Parent,
            code: 80,
            comments: 20,
            blanks: 0,
            lines: 100,
            bytes: 4000,
            tokens: 800,
        })
        .collect();
    let refs: Vec<_> = rows.iter().collect();
    let top = build_top_offenders(&refs);
    let expected: Vec<_> = (0..10).map(|i| format!("item{i}")).collect();
    for list in [
        &top.largest_lines,
        &top.largest_tokens,
        &top.largest_bytes,
        &top.least_documented,
        &top.most_dense,
    ] {
        assert_eq!(
            list.iter().map(|r| &r.module).collect::<Vec<_>>(),
            expected.iter().collect::<Vec<_>>()
        );
    }
}

#[test]
fn empty_and_fully_filtered_inputs_do_not_allocate_selection_scratch() {
    let empty = select_top(std::iter::empty(), |a, b| a.path.cmp(&b.path));
    assert_eq!(empty.capacity(), 0);
    let rows: Vec<_> = (0..32).map(row).collect();
    let filtered = select_top(rows.iter().filter(|_| false), |a, b| a.path.cmp(&b.path));
    assert_eq!(filtered.capacity(), 0);
}

#[test]
fn selection_scratch_is_bounded_by_the_output_limit() {
    let rows: Vec<_> = (0..4096).map(row).collect();
    let top = select_top(rows.iter(), |a, b| a.path.cmp(&b.path));
    assert_eq!(top.len(), TOP_N);
    assert!(
        top.capacity() <= TOP_N,
        "scratch capacity {} exceeds {TOP_N}",
        top.capacity()
    );
}

#[test]
fn selection_comparison_work_is_bounded_for_large_inputs() {
    let n = 16_384;
    let mut rows: Vec<_> = (0..n).map(row).collect();
    for mode in 0..4 {
        match mode {
            1 => rows.sort_by(|a, b| a.path.cmp(&b.path)),
            2 => rows.reverse(),
            3 => rows.iter_mut().for_each(|r| r.path = "equal.rs".into()),
            _ => {}
        }
        let comparisons = Cell::new(0);
        let top = select_top(rows.iter(), |a, b| {
            comparisons.set(comparisons.get() + 1);
            a.path.cmp(&b.path)
        });
        assert_eq!(top.len(), TOP_N);
        assert!(
            comparisons.get() <= n * 6,
            "mode={mode}, comparisons={}",
            comparisons.get()
        );
        assert!(top.windows(2).all(|w| {
            let mut pair = w.iter();
            pair.next()
                .zip(pair.next())
                .is_some_and(|(a, b)| a.path.cmp(&b.path) != Ordering::Greater)
        }));
    }
}

// The pre-optimization stable full-sort implementation is an independent
// algorithmic oracle for every serialized field and for input-order ties.
fn full_sort_oracle(rows: &[&FileRow]) -> TopOffenders {
    let mut by_lines: Vec<&FileRow> = rows.to_vec();
    by_lines.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.path.cmp(&b.path)));
    by_lines.truncate(TOP_N);

    let mut by_tokens: Vec<&FileRow> = rows.to_vec();
    by_tokens.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.path.cmp(&b.path)));
    by_tokens.truncate(TOP_N);

    let mut by_bytes: Vec<&FileRow> = rows.to_vec();
    by_bytes.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    by_bytes.truncate(TOP_N);

    let mut least_doc: Vec<&FileRow> = rows
        .iter()
        .copied()
        .filter(|r| r.lines >= MIN_DOC_LINES)
        .collect();
    least_doc.sort_by(|a, b| {
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
    });
    least_doc.truncate(TOP_N);

    let mut dense: Vec<&FileRow> = rows
        .iter()
        .copied()
        .filter(|r| r.lines >= MIN_DENSE_LINES)
        .collect();
    dense.sort_by(|a, b| {
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
    });
    dense.truncate(TOP_N);

    TopOffenders {
        largest_lines: by_lines.into_iter().map(to_stat_row).collect(),
        largest_tokens: by_tokens.into_iter().map(to_stat_row).collect(),
        largest_bytes: by_bytes.into_iter().map(to_stat_row).collect(),
        least_documented: least_doc.into_iter().map(to_stat_row).collect(),
        most_dense: dense.into_iter().map(to_stat_row).collect(),
    }
}
