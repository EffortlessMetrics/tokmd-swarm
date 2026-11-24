use std::fs;
use std::io;

use tokei::{Config, Languages};
use tokmd_model::collect_file_rows;
use tokmd_types::ChildIncludeMode;

#[test]
fn native_file_rows_preserve_distinct_names_under_equivalent_roots() -> io::Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("fixtures");
    fs::create_dir_all(root.join("panel"))?;
    fs::write(root.join("panel/blue.rs"), "fn blue() {}\n")?;
    let distinct = if cfg!(unix) {
        r"panel\blue.rs"
    } else {
        "panel/green.rs"
    };
    fs::write(root.join(distinct), "fn green() {}\nfn lime() {}\n")?;

    let mut languages = Languages::new();
    languages.get_statistics(&[&root], &[], &Config::default());
    let alias = temp.path().join("fixtures//");
    for prefix in [&root, &alias] {
        let rows = collect_file_rows(
            &languages,
            &[],
            1,
            ChildIncludeMode::ParentsOnly,
            Some(prefix),
        )?;
        let paths: Vec<_> = rows
            .iter()
            .map(|row| (row.path.as_str(), row.code))
            .collect();
        assert_eq!(paths, [(distinct, 2), ("panel/blue.rs", 1)]);
    }
    Ok(())
}

#[test]
fn distinct_sources_cannot_acquire_the_same_label_when_a_prefix_is_removed() -> std::io::Result<()>
{
    use std::path::Path;
    use tokei::{Language, LanguageType, Report};

    let mut languages = Languages::new();
    for (lang, name) in [
        (LanguageType::Rust, "src/work.rs"),
        (LanguageType::Text, "work.rs"),
    ] {
        let mut language = Language::new();
        language.reports.push(Report::new(name.into()));
        languages.insert(lang, language);
    }

    let rows = collect_file_rows(&languages, &[], 1, ChildIncludeMode::ParentsOnly, None)?;
    assert_eq!(rows.len(), 2);
    let err = collect_file_rows(
        &languages,
        &[],
        1,
        ChildIncludeMode::ParentsOnly,
        Some(Path::new("src")),
    )
    .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("strip prefix"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn raw_model_inputs_reject_unsupported_bytes_and_preserve_supported_names() -> io::Result<()> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;
    use tokmd_model::{InMemoryRowInput, collect_in_memory_file_rows};

    let inputs = [r"literal\name.rs", "literal/name.rs", "literal-�.rs"]
        .map(|path| InMemoryRowInput::new(Path::new(path), b"fn measured() {}\n"));
    let collect = |inputs: &[InMemoryRowInput<'_>]| {
        collect_in_memory_file_rows(
            inputs,
            &[],
            1,
            ChildIncludeMode::ParentsOnly,
            &Config::default(),
        )
    };
    let rows = collect(&inputs)?;
    assert_eq!(
        rows.into_iter()
            .map(|row| (row.path, row.module))
            .collect::<Vec<_>>(),
        [
            ("literal-�.rs".into(), "(root)".into()),
            ("literal/name.rs".into(), "literal".into()),
            (r"literal\name.rs".into(), "(root)".into()),
        ],
    );
    for (raw, hex) in [
        (b"x-\xff.rs", "782dff2e7273"),
        (b"x-\xfe.rs", "782dfe2e7273"),
    ] {
        let path = Path::new(OsStr::from_bytes(raw));
        let input = InMemoryRowInput::new(path, b"fn measured() {}\n");
        let err = collect(&[input]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains(hex), "{err}");

        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join(path), "fn measured() {}\n")?;
        let mut languages = Languages::new();
        languages.get_statistics(&[dir.path()], &[], &Config::default());
        let err = collect_file_rows(
            &languages,
            &[],
            1,
            ChildIncludeMode::ParentsOnly,
            Some(dir.path()),
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains(hex), "{err}");
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_separators_share_a_label_and_unpaired_surrogates_are_rejected() -> io::Result<()> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::Path;
    use tokmd_model::native_report_path;

    for path in [r"root\src\lib.rs", "root/src/lib.rs"] {
        assert_eq!(
            native_report_path(Path::new(path), Some(Path::new("root")))?,
            "src/lib.rs"
        );
    }
    let invalid = OsString::from_wide(&[0xd800]);
    let failure = native_report_path(Path::new(&invalid), None).unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::InvalidData);
    assert!(failure.to_string().contains("Rename"));
    assert_eq!(native_report_path(Path::new("�"), None)?, "�");
    Ok(())
}
