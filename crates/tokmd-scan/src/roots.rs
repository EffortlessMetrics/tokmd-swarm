//! Validated scan root handling and caller-facing report path rebasing.

use anyhow::Result;
use std::path::{Path, PathBuf};
use tokei::Languages;

use crate::path::ValidatedRoot;

pub(crate) fn validated_scan_roots(paths: &[PathBuf]) -> Result<Vec<ValidatedRoot>> {
    paths
        .iter()
        .map(ValidatedRoot::new)
        .collect::<std::result::Result<_, _>>()
        .map_err(Into::into)
}

pub(crate) fn rebase_report_paths(languages: &mut Languages, roots: &[ValidatedRoot]) {
    // Canonical inputs already have caller-facing report paths. Compare raw
    // spelling: Path equality would also treat an input ending in `/.` as equal.
    if roots
        .iter()
        .all(|root| root.input().as_os_str() == root.canonical().as_os_str())
    {
        return;
    }

    for language in languages.values_mut() {
        for report in &mut language.reports {
            report.name = rebase_report_path(&report.name, roots);
        }
        for reports in language.children.values_mut() {
            for report in reports {
                report.name = rebase_report_path(&report.name, roots);
            }
        }
    }
}

fn rebase_report_path(path: &Path, roots: &[ValidatedRoot]) -> PathBuf {
    roots
        .iter()
        .filter_map(|root| {
            path.strip_prefix(root.canonical())
                .ok()
                .map(|relative| (root, relative))
        })
        .max_by_key(|(root, _)| root.canonical().components().count())
        .map_or_else(
            || path.to_path_buf(),
            |(root, relative)| {
                if relative.as_os_str().is_empty() {
                    root.input().to_path_buf()
                } else {
                    root.input().join(relative)
                }
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn canonical_rebase_retains_parent_and_embedded_path_storage() -> Result<()> {
        let dir = tempfile::tempdir()?;
        fs::write(
            dir.path().join("index.html"),
            "<html><script>const answer = 42;</script></html>\n",
        )?;
        let canonical = fs::canonicalize(dir.path())?;
        let roots = validated_scan_roots(std::slice::from_ref(&canonical))?;
        let mut languages = Languages::new();
        languages.get_statistics(
            std::slice::from_ref(&canonical),
            &[],
            &tokei::Config::default(),
        );
        let before: Vec<_> = languages
            .values()
            .flat_map(|language| {
                language
                    .reports
                    .iter()
                    .chain(language.children.values().flatten())
            })
            .map(|report| report.name.as_os_str().as_encoded_bytes().as_ptr())
            .collect();
        assert!(before.len() >= 2, "fixture must include an embedded report");

        rebase_report_paths(&mut languages, &roots);

        for (report, original_storage) in languages
            .values()
            .flat_map(|language| {
                language
                    .reports
                    .iter()
                    .chain(language.children.values().flatten())
            })
            .zip(before)
        {
            assert_eq!(
                report.name.as_os_str(),
                canonical.join("index.html").as_os_str()
            );
            assert_eq!(
                report.name.as_os_str().as_encoded_bytes().as_ptr(),
                original_storage,
                "canonical reports should retain their existing path allocation"
            );
        }
        Ok(())
    }

    #[test]
    fn rebase_preserves_dot_spelling_even_when_paths_compare_equal() -> Result<()> {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join("lib.rs"), "pub fn lib() {}\n")?;
        let canonical = fs::canonicalize(dir.path())?;
        let input = canonical.join(".");
        assert_eq!(input, canonical);
        assert_ne!(input.as_os_str(), canonical.as_os_str());
        let roots = validated_scan_roots(std::slice::from_ref(&input))?;
        let mut languages = Languages::new();
        languages.get_statistics(&[canonical], &[], &tokei::Config::default());

        rebase_report_paths(&mut languages, &roots);

        let report = &languages[&tokei::LanguageType::Rust].reports[0];
        assert_eq!(report.name.as_os_str(), input.join("lib.rs").as_os_str());
        Ok(())
    }

    #[test]
    fn rebase_mixed_roots_preserves_last_equal_depth_alias() -> Result<()> {
        let dir = tempfile::tempdir()?;
        fs::create_dir(dir.path().join("nested"))?;
        fs::write(dir.path().join("lib.rs"), "pub fn lib() {}\n")?;
        let canonical = fs::canonicalize(dir.path())?;
        let alias = canonical.join("nested/..");
        let roots = validated_scan_roots(&[canonical.clone(), alias.clone()])?;
        let mut languages = Languages::new();
        languages.get_statistics(&[canonical], &[], &tokei::Config::default());

        rebase_report_paths(&mut languages, &roots);

        let report = &languages[&tokei::LanguageType::Rust].reports[0];
        assert_eq!(report.name.as_os_str(), alias.join("lib.rs").as_os_str());
        Ok(())
    }

    #[test]
    fn validated_scan_roots_resolve_parent_segments_before_walking() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("repo");
        let src = root.join("src");
        fs::create_dir_all(&src)?;
        fs::write(src.join("lib.rs"), "pub fn lib() {}\n")?;

        let aliased_root = src.join("..");
        let roots = validated_scan_roots(&[aliased_root])?;

        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].canonical(), fs::canonicalize(&root)?);
        Ok(())
    }
}
