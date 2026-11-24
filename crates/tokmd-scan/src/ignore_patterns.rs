//! Tokei ignore-pattern expansion for canonicalized scan roots.
//!
//! Scan roots are validated and canonicalized before being passed to `tokei`.
//! Relative excludes therefore need canonical-root variants in addition to the
//! caller-provided and globbed forms so excludes keep matching after root
//! normalization.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use ignore::{Match, gitignore::GitignoreBuilder, overrides::OverrideBuilder};
use tokmd_settings::ScanOptions;

use crate::path::{ValidatedRoot, normalize_slashes};

pub(crate) fn ignored_patterns(args: &ScanOptions, roots: &[ValidatedRoot]) -> Vec<String> {
    let mut patterns = BTreeSet::new();

    for pattern in &args.excluded {
        expand_pattern(pattern, roots, &mut patterns);
    }
    patterns.into_iter().collect()
}

fn expand_pattern(pattern: &str, roots: &[ValidatedRoot], patterns: &mut BTreeSet<String>) {
    patterns.insert(pattern.to_owned());
    if is_absolute_pattern(pattern) {
        return;
    }
    let relative = normalize_relative_ignore_pattern(pattern);
    if relative.is_empty() {
        return;
    }
    if !relative.starts_with("**/") {
        patterns.insert(format!("**/{relative}"));
    }
    for root in roots {
        // Only the user's pattern is glob syntax. The physical prefix may
        // itself contain metacharacters or a literal Unix backslash.
        if let Some(root) = root.canonical().to_str() {
            let prefix = if cfg!(windows) {
                crate::exclude::escape_literal_glob_path(&normalize_slashes(root))
            } else {
                crate::exclude::escape_literal_glob_path(root)
            };
            // The first slash is the gitignore anchor. The physical absolute
            // path, including its own leading slash, must follow the anchor.
            patterns.insert(format!("/{}/{relative}", prefix.trim_end_matches('/')));
        }
    }
}

/// Explain which explicit exclusions match a candidate in a scan from the cwd.
/// Match the same expanded overrides and ancestor directories as the tokei walk.
pub fn matching_exclude_flags(path: &Path, patterns: &[String]) -> Result<Vec<String>> {
    let mut reasons = Vec::new();
    if patterns.is_empty() {
        return Ok(reasons);
    }
    let root = ValidatedRoot::new(".")?;
    let absolute = root.canonical().join(path);
    let is_dir = absolute.is_dir();
    for pattern in patterns {
        let mut expanded = BTreeSet::new();
        expand_pattern(pattern, std::slice::from_ref(&root), &mut expanded);
        let mut builder = OverrideBuilder::new(".");
        for glob in expanded {
            builder.add(&format!("!{glob}"))?;
        }
        let matcher = builder.build()?;
        if absolute
            .ancestors()
            .take_while(|p| *p != root.canonical())
            .any(|candidate| {
                matcher
                    .matched(candidate, candidate != absolute || is_dir)
                    .is_ignore()
            })
        {
            reasons.push(pattern.clone());
        }
    }
    Ok(reasons)
}

/// Find the effective .tokeignore rule for a native path and its source file.
pub fn matching_tokeignore(
    path: &Path,
    include_parents: bool,
) -> Result<Option<(std::path::PathBuf, String)>> {
    let path = std::path::absolute(path)?;
    let cwd = std::env::current_dir()?;
    let levels: Vec<_> = path.ancestors().collect();
    let is_dir = path.is_dir();
    let mut active = Vec::<(std::path::PathBuf, ignore::gitignore::Gitignore)>::new();
    for entry in levels.into_iter().rev() {
        // The walker enters its explicit root without applying ignore rules.
        // Ancestor rule files can still govern entries below that root.
        let above_root = cwd.starts_with(entry) && path.starts_with(&cwd);
        if !above_root {
            for (source, matcher) in active.iter().rev() {
                match matcher.matched(entry, entry != path || is_dir) {
                    Match::Ignore(rule) => {
                        return Ok(Some((source.clone(), rule.original().into())));
                    }
                    Match::Whitelist(_) => break,
                    Match::None => {}
                }
            }
        }
        // Read a child's rules only after its parent rules allowed the walk to
        // enter that directory. A file-level whitelist cannot undo pruning.
        if entry == path || (above_root && entry != cwd && !include_parents) {
            continue;
        }
        let source = entry.join(".tokeignore");
        if source.try_exists()? {
            let mut builder = GitignoreBuilder::new(entry);
            if let Some(error) = builder.add(&source) {
                return Err(error.into());
            }
            active.push((source, builder.build()?));
        }
    }
    Ok(None)
}

fn is_absolute_pattern(pattern: &str) -> bool {
    let path = Path::new(pattern);
    path.is_absolute()
        || pattern.starts_with('/')
        || pattern.starts_with('\\')
        || pattern.as_bytes().get(1).is_some_and(|byte| *byte == b':')
}

fn normalize_relative_ignore_pattern(pattern: &str) -> String {
    // A doubled backslash in a Unix ignore glob matches a literal backslash.
    // Rewriting that escape would add an exclusion for a different source name.
    let mut normalized = if cfg!(unix) && pattern.contains(r"\\") {
        pattern.to_owned()
    } else {
        normalize_slashes(pattern)
    };
    while let Some(rest) = normalized.strip_prefix("./") {
        normalized = rest.to_string();
    }
    normalized.trim_start_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_options(excluded: &[&str]) -> ScanOptions {
        ScanOptions {
            excluded: excluded.iter().map(|pattern| pattern.to_string()).collect(),
            ..ScanOptions::default()
        }
    }

    #[test]
    fn ignore_normalize_relative_pattern_strips_dot_and_slashes() {
        assert_eq!(
            normalize_relative_ignore_pattern("./generated/**"),
            "generated/**"
        );
        assert_eq!(
            normalize_relative_ignore_pattern(r".\generated\**"),
            "generated/**"
        );
        assert_eq!(normalize_relative_ignore_pattern("/target/**"), "target/**");
    }

    #[test]
    fn ignore_patterns_expand_relative_patterns_for_canonical_roots() {
        let dir = tempfile::tempdir().unwrap();
        let root = ValidatedRoot::new(dir.path()).unwrap();
        let patterns = ignored_patterns(
            &scan_options(&["secret_folder/**"]),
            std::slice::from_ref(&root),
        );
        let canonical =
            normalize_slashes(&root.canonical().to_string_lossy()).replacen("//?/", "//[?]/", 1);

        assert!(patterns.contains(&"secret_folder/**".to_string()));
        assert!(patterns.contains(&"**/secret_folder/**".to_string()));
        assert!(patterns.contains(&format!("/{canonical}/secret_folder/**")));
    }

    #[cfg(unix)]
    #[test]
    fn expanded_root_is_literal_glob_text() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let physical = dir.path().join(r"source[12]\leaf");
        std::fs::create_dir(&physical)?;
        let root = ValidatedRoot::new(&physical)?;
        let expanded = ignored_patterns(&scan_options(&["*.rs"]), &[root]);
        let rooted = expanded
            .iter()
            .find(|p| p.starts_with('/'))
            .ok_or_else(|| anyhow::anyhow!("missing canonical-root pattern"))?;
        let mut builder = OverrideBuilder::new(".");
        builder.add(&format!("!{rooted}"))?;
        let matcher = builder.build()?;
        assert!(
            matcher
                .matched(physical.join("selected.rs"), false)
                .is_ignore()
        );
        assert!(
            !matcher
                .matched(dir.path().join("source1/leaf/selected.rs"), false)
                .is_ignore()
        );
        Ok(())
    }

    #[test]
    fn ignore_patterns_keep_absolute_patterns_as_given() {
        let dir = tempfile::tempdir().unwrap();
        let root = ValidatedRoot::new(dir.path()).unwrap();
        let absolute = dir.path().join("target/**").to_string_lossy().to_string();
        let patterns = ignored_patterns(&scan_options(&[&absolute]), &[root]);

        assert_eq!(patterns, vec![absolute]);
    }
}
