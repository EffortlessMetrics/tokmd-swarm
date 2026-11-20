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
        );
        let paths: Vec<_> = rows
            .iter()
            .map(|row| (row.path.as_str(), row.code))
            .collect();
        assert_eq!(paths, [(distinct, 2), ("panel/blue.rs", 1)]);
    }
    Ok(())
}
