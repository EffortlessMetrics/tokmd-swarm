//! Module totals must count each physical file once, including its embedded code.

use std::path::Path;

use tokei::{Config, Languages};
use tokmd_model::{
    InMemoryRowInput, collect_in_memory_file_rows, create_lang_report_from_rows,
    create_module_report, create_module_report_from_rows,
};
use tokmd_types::{ChildIncludeMode, ChildrenMode, FileKind};

const MIXED: &[u8] = b"<html>\n<script>\nconst answer = 42;\n</script>\n<style>\nbody { color: red; }\n</style>\n</html>\n";

#[test]
fn embedded_module_totals_match_language_and_parent_only_totals() {
    let rows = collect_in_memory_file_rows(
        &[
            InMemoryRowInput::new(Path::new("web/index.html"), MIXED),
            InMemoryRowInput::new(Path::new("src/lib.rs"), b"fn library() {}\n"),
        ],
        &[],
        1,
        ChildIncludeMode::Separate,
        &Config::default(),
    );
    assert!(rows.iter().any(|r| r.kind == FileKind::Child && r.code > 0));
    let parents: Vec<_> = rows
        .iter()
        .filter(|r| r.kind == FileKind::Parent)
        .cloned()
        .collect();
    let expected =
        create_module_report_from_rows(&parents, &[], 1, ChildIncludeMode::ParentsOnly, 0);
    let collapsed = create_lang_report_from_rows(&rows, 0, true, ChildrenMode::Collapse);
    let separated = create_lang_report_from_rows(&rows, 0, true, ChildrenMode::Separate);
    assert_eq!(
        serde_json::to_value(&collapsed.total).unwrap(),
        serde_json::to_value(&expected.total).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&separated.total).unwrap(),
        serde_json::to_value(&expected.total).unwrap()
    );

    for top in [0, 1] {
        for input in [rows.clone(), rows.iter().rev().cloned().collect()] {
            let actual =
                create_module_report_from_rows(&input, &[], 1, ChildIncludeMode::Separate, top);
            assert_eq!(actual.total.code, 9);
            assert_eq!(actual.total.lines, 9);
            assert_eq!(actual.total.files, 2);
            assert_eq!(
                serde_json::to_value(&actual.total).unwrap(),
                serde_json::to_value(&expected.total).unwrap()
            );
            assert_eq!(
                actual.rows.iter().map(|r| r.code).sum::<usize>(),
                expected.total.code
            );
            assert_eq!(
                actual.rows.iter().map(|r| r.lines).sum::<usize>(),
                expected.total.lines
            );
            assert_eq!(
                actual.rows.iter().map(|r| r.files).sum::<usize>(),
                expected.total.files
            );
            assert_eq!(
                actual.rows.iter().map(|r| r.bytes).sum::<usize>(),
                expected.total.bytes
            );
            assert_eq!(
                actual.rows.iter().map(|r| r.tokens).sum::<usize>(),
                expected.total.tokens
            );
            if top == 0 {
                assert_eq!(
                    serde_json::to_value(&actual.rows).unwrap(),
                    serde_json::to_value(&expected.rows).unwrap()
                );
            }
        }
    }
}

#[test]
fn filesystem_module_totals_are_independent_of_child_detail() -> std::io::Result<()> {
    let temp = tempfile::tempdir()?;
    std::fs::write(temp.path().join("index.html"), MIXED)?;
    let mut languages = Languages::new();
    languages.get_statistics(&[temp.path()], &[], &Config::default());
    assert!(languages.values().any(|lang| !lang.children.is_empty()));
    let parents = create_module_report(&languages, &[], 1, ChildIncludeMode::ParentsOnly, 0);
    let separate = create_module_report(&languages, &[], 1, ChildIncludeMode::Separate, 0);
    assert_eq!(separate.total.code, 8);
    assert_eq!(separate.total.lines, 8);
    assert_eq!(separate.total.files, 1);
    assert_eq!(
        serde_json::to_value(&separate.total).unwrap(),
        serde_json::to_value(&parents.total).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&separate.rows).unwrap(),
        serde_json::to_value(&parents.rows).unwrap()
    );
    Ok(())
}

#[test]
fn module_preserves_children_without_a_matching_parent() {
    let mut rows = collect_in_memory_file_rows(
        &[InMemoryRowInput::new(Path::new("web/index.html"), MIXED)],
        &[],
        1,
        ChildIncludeMode::Separate,
        &Config::default(),
    );
    rows.retain(|r| r.kind == FileKind::Child);
    assert!(!rows.is_empty());
    let actual = create_module_report_from_rows(&rows, &[], 1, ChildIncludeMode::Separate, 0);
    assert_eq!(actual.total.code, 2);
    assert_eq!(actual.total.lines, 2);
    assert_eq!(
        actual.total.code,
        rows.iter().map(|r| r.code).sum::<usize>()
    );
    assert_eq!(
        actual.total.lines,
        rows.iter().map(|r| r.lines).sum::<usize>()
    );
    assert_eq!(actual.total.files, 0);
}
