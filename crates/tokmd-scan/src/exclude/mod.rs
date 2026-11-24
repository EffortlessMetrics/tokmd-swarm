//! Deterministic exclude-pattern normalization and dedupe helpers.

#![forbid(unsafe_code)]

use std::path::Path;

use crate::normalize_rel_path;

/// Normalize a portable exclusion spelling, where both slash styles are separators.
/// For paths obtained from the host filesystem, use [`output_exclusion`] to quote
/// literal backslashes and glob metacharacters in filenames.
///
/// Rules:
/// - if `path` is absolute and under `root`, strip the `root` prefix
/// - convert backslashes to `/`
/// - strip one leading `./`
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use tokmd_scan::normalize_exclude_pattern;
///
/// let root = Path::new("/project");
/// let relative = Path::new("./out/bundle.js");
/// assert_eq!(normalize_exclude_pattern(root, relative), "out/bundle.js");
/// ```
#[must_use]
pub fn normalize_exclude_pattern(root: &Path, path: &Path) -> String {
    let rel = if path.is_absolute() {
        path.strip_prefix(root).unwrap_or(path)
    } else {
        path
    };
    normalize_rel_path(&rel.to_string_lossy())
}

/// Render a native output path for a receipt and for a literal gitignore glob.
/// The display name and the matcher differ when a filename contains glob syntax.
pub fn output_exclusion(root: &Path, path: &Path) -> anyhow::Result<(String, String)> {
    let relative = if path.is_absolute() {
        path.strip_prefix(root).unwrap_or(path)
    } else {
        path
    };
    let native = tokmd_types::validate_source_path(relative)?;
    let display = if cfg!(windows) {
        native.replace('\\', "/")
    } else {
        native.to_owned()
    };
    let display = display.trim_start_matches("./");
    Ok((display.to_owned(), escape_literal_glob_path(display)))
}

/// Quote a literal path whose directory separators have already been rendered as `/`.
pub(crate) fn escape_literal_glob_path(display: &str) -> String {
    let mut pattern = String::with_capacity(display.len());
    for ch in display.chars() {
        match ch {
            '\\' => pattern.push_str(r"\\"),
            // Gitignore trims trailing whitespace unless the glob quotes it.
            c if matches!(c, '*' | '?' | '[' | ']' | '{' | '}') || c.is_whitespace() => {
                pattern.push('[');
                pattern.push(ch);
                pattern.push(']');
            }
            _ => pattern.push(ch),
        }
    }
    pattern
}

/// Retain glob escapes when comparing patterns. Simple portable path spellings
/// can still use either directory separator.
fn pattern_key(pattern: &str) -> String {
    if pattern.contains(r"\\") || pattern.contains(['*', '?', '[', ']', '{', '}']) {
        pattern.trim_start_matches("./").to_owned()
    } else {
        normalize_rel_path(pattern)
    }
}

/// Return `true` when `existing` already contains `pattern` after normalization.
///
/// # Examples
///
/// ```
/// use tokmd_scan::has_exclude_pattern;
///
/// let existing = vec!["out/bundle".to_string()];
/// assert!(has_exclude_pattern(&existing, "./out/bundle"));
/// assert!(!has_exclude_pattern(&existing, "dist/app"));
/// ```
#[must_use]
pub fn has_exclude_pattern(existing: &[String], pattern: &str) -> bool {
    let normalized = pattern_key(pattern);
    existing
        .iter()
        .any(|candidate| pattern_key(candidate) == normalized)
}

/// Add a pattern only when non-empty and not already present (after normalization).
///
/// Returns `true` when the pattern was inserted.
///
/// # Examples
///
/// ```
/// use tokmd_scan::add_exclude_pattern;
///
/// let mut patterns = vec![];
/// assert!(add_exclude_pattern(&mut patterns, "out/bundle".to_string()));
/// assert!(!add_exclude_pattern(&mut patterns, "./out/bundle".to_string())); // duplicate
/// assert!(!add_exclude_pattern(&mut patterns, String::new())); // empty
/// assert_eq!(patterns.len(), 1);
/// ```
pub fn add_exclude_pattern(existing: &mut Vec<String>, pattern: String) -> bool {
    if pattern.is_empty() || has_exclude_pattern(existing, &pattern) {
        return false;
    }
    existing.push(pattern);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_exclude_pattern_strips_root_for_absolute_paths() {
        let root = std::env::temp_dir().join("tokmd-exclude-lib-root");
        let path = root.join(".handoff").join("manifest.json");
        let got = normalize_exclude_pattern(&root, &path);
        assert_eq!(got, ".handoff/manifest.json");
    }

    #[test]
    fn normalize_exclude_pattern_keeps_outside_absolute_paths() {
        let root = std::env::temp_dir().join("tokmd-exclude-lib-root");
        let outside = std::env::temp_dir()
            .join("tokmd-exclude-lib-outside")
            .join("bundle.txt");
        let got = normalize_exclude_pattern(&root, &outside);
        let expected = crate::normalize_rel_path(&outside.to_string_lossy());
        assert_eq!(got, expected);
    }

    #[test]
    fn add_exclude_pattern_dedupes_after_normalization() {
        let mut existing = vec!["./out\\bundle".to_string()];
        assert!(!add_exclude_pattern(
            &mut existing,
            "out/bundle".to_string()
        ));
        assert_eq!(existing.len(), 1);
    }

    #[test]
    fn escaped_globs_are_not_deduplicated_as_portable_separators() {
        let mut patterns = vec![r"one\\two".to_string(), r"one\*.rs".to_string()];
        assert!(add_exclude_pattern(&mut patterns, "one//two".into()));
        assert!(add_exclude_pattern(&mut patterns, "one/*.rs".into()));
        assert!(!add_exclude_pattern(&mut patterns, r"./one\\two".into()));
    }

    #[test]
    fn add_exclude_pattern_rejects_empty_patterns() {
        let mut existing = vec![];
        assert!(!add_exclude_pattern(&mut existing, String::new()));
        assert!(existing.is_empty());
    }
}
