use tokmd_model::create_export_data_from_rows;
use tokmd_types::{ChildIncludeMode, FileKind, FileRow};

fn row(lang: &str, kind: FileKind, comments: usize) -> FileRow {
    FileRow {
        path: "mixed.html".into(),
        module: "(root)".into(),
        lang: lang.into(),
        kind,
        code: 10,
        comments,
        blanks: 0,
        lines: 10 + comments,
        bytes: 0,
        tokens: 0,
    }
}

#[test]
fn export_ties_are_independent_of_input_order_and_limit() {
    let rows = vec![
        row("JavaScript", FileKind::Child, 2),
        row("HTML", FileKind::Parent, 1),
        row("JavaScript", FileKind::Child, 1),
    ];
    for limit in [0, 1, 2] {
        let export =
            |rows| create_export_data_from_rows(rows, &[], 1, ChildIncludeMode::Separate, 0, limit);
        let forward = serde_json::to_value(export(rows.clone())).unwrap();
        let reverse = serde_json::to_value(export(rows.iter().cloned().rev().collect())).unwrap();
        assert_eq!(forward, reverse, "limit {limit}");
    }
}
