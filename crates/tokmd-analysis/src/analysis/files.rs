use std::path::{Path, PathBuf};

#[cfg(feature = "walk")]
use std::{collections::BTreeSet, fs};

use crate::grid::PresetPlan;
#[cfg(feature = "walk")]
use anyhow::{Context, Result};
use tokmd_analysis_types::AnalysisSource;

#[cfg(any(feature = "walk", feature = "content"))]
pub(super) const ROOTLESS_FILE_ANALYSIS_WARNING: &str =
    "in-memory analysis has no host root; skipping file-backed enrichers";
#[cfg(feature = "git")]
pub(super) const ROOTLESS_GIT_ANALYSIS_WARNING: &str =
    "in-memory analysis has no host root; skipping git-backed enrichers";

pub(super) fn has_host_root(root: &Path) -> bool {
    !root.as_os_str().is_empty()
}

#[cfg(any(feature = "walk", feature = "content", feature = "git"))]
pub(super) fn push_warning_once(warnings: &mut Vec<String>, warning: &str) {
    if warnings.iter().all(|existing| existing != warning) {
        warnings.push(warning.to_string());
    }
}

pub(super) fn analysis_roots(source: &AnalysisSource) -> Vec<PathBuf> {
    if !source_uses_ad_hoc_paths(source) {
        return vec![PathBuf::from(".")];
    }

    let roots: Vec<PathBuf> = source
        .inputs
        .iter()
        .map(|input| input.trim())
        .filter(|input| !input.is_empty())
        .map(PathBuf::from)
        .collect();

    if roots.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        roots
    }
}

fn source_uses_ad_hoc_paths(source: &AnalysisSource) -> bool {
    source.export_path.is_none()
        && source.base_receipt_path.is_none()
        && source.export_schema_version.is_none()
}

pub(super) fn collect_required_files(
    root: &Path,
    analysis_roots: &[PathBuf],
    plan: &PresetPlan,
    max_files: Option<usize>,
    has_host_root: bool,
    warnings: &mut Vec<String>,
) -> Option<Vec<PathBuf>> {
    if !plan.needs_files() {
        return None;
    }

    #[cfg(feature = "walk")]
    {
        if has_host_root {
            match collect_scoped_files(root, analysis_roots, max_files) {
                Ok(list) => Some(list),
                Err(err) => {
                    warnings.push(format!("walk failed: {}", err));
                    None
                }
            }
        } else {
            push_warning_once(warnings, ROOTLESS_FILE_ANALYSIS_WARNING);
            None
        }
    }

    #[cfg(not(feature = "walk"))]
    {
        let _ = (root, analysis_roots, max_files, has_host_root);
        warnings.push(
            crate::grid::DisabledFeature::FileInventory
                .warning()
                .to_string(),
        );
        None
    }
}

#[cfg(feature = "walk")]
struct ScopedAnalysisRoot {
    absolute: PathBuf,
    relative: PathBuf,
    is_file: bool,
}

#[cfg(feature = "walk")]
fn collect_scoped_files(
    root: &Path,
    analysis_roots: &[PathBuf],
    max_files: Option<usize>,
) -> Result<Vec<PathBuf>> {
    if max_files == Some(0) {
        return Ok(Vec::new());
    }

    let scopes = if analysis_roots.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        analysis_roots.to_vec()
    };

    let mut files = BTreeSet::new();
    for scope in scopes {
        let scoped = resolve_scope(root, &scope)?;
        if scoped.is_file {
            files.insert(scoped.relative);
        } else {
            // Each scope must contribute its smallest N paths, even when the
            // union is already full or overlaps earlier scopes. Any omitted
            // path has N smaller peers in this scope, so it cannot enter the
            // global smallest N. Retain only N distinct paths as we merge.
            for rel in tokmd_scan::walk::list_files(&scoped.absolute, max_files)? {
                let joined = if scoped.relative.as_os_str().is_empty() {
                    rel
                } else {
                    scoped.relative.join(rel)
                };
                files.insert(joined);
                if max_files.is_some_and(|limit| files.len() > limit) {
                    files.pop_last();
                }
            }
        }

        if max_files.is_some_and(|limit| files.len() > limit) {
            files.pop_last();
        }
    }

    Ok(files.into_iter().collect())
}

#[cfg(feature = "walk")]
fn resolve_scope(root: &Path, scope: &Path) -> Result<ScopedAnalysisRoot> {
    let root = fs::canonicalize(root)
        .with_context(|| format!("failed to canonicalize analysis root {}", root.display()))?;
    let candidate = if scope.as_os_str().is_empty() || scope == Path::new(".") {
        root.clone()
    } else if scope.is_absolute() {
        scope.to_path_buf()
    } else {
        root.join(scope)
    };
    let absolute = fs::canonicalize(&candidate).with_context(|| {
        format!(
            "failed to canonicalize analysis scope {}",
            candidate.display()
        )
    })?;
    let relative = absolute
        .strip_prefix(&root)
        .with_context(|| {
            format!(
                "analysis scope {} is outside analysis root {}",
                candidate.display(),
                root.display()
            )
        })?
        .to_path_buf();
    let is_file = fs::metadata(&absolute)
        .with_context(|| format!("failed to stat analysis scope {}", absolute.display()))?
        .is_file();

    Ok(ScopedAnalysisRoot {
        absolute,
        relative,
        is_file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_with_inputs(inputs: &[&str]) -> AnalysisSource {
        AnalysisSource {
            inputs: inputs.iter().map(|input| input.to_string()).collect(),
            export_path: None,
            base_receipt_path: None,
            export_schema_version: None,
            export_generated_at_ms: None,
            base_signature: None,
            module_roots: vec![],
            module_depth: 1,
            children: "separate".to_string(),
        }
    }

    #[test]
    fn analysis_roots_use_source_inputs_for_ad_hoc_scans() {
        let source = source_with_inputs(&["src", "README.md"]);

        assert_eq!(
            analysis_roots(&source),
            vec![PathBuf::from("src"), PathBuf::from("README.md")]
        );
    }

    #[test]
    fn analysis_roots_keep_export_backed_sources_on_host_root() {
        let mut source = source_with_inputs(&["src"]);
        source.export_schema_version = Some(2);

        assert_eq!(analysis_roots(&source), vec![PathBuf::from(".")]);
    }

    #[cfg(feature = "walk")]
    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[cfg(feature = "walk")]
    fn plan_needing_files() -> PresetPlan {
        PresetPlan {
            assets: false,
            deps: false,
            todo: true,
            dup: false,
            imports: false,
            git: false,
            fun: false,
            archetype: false,
            topics: false,
            entropy: false,
            license: false,
            complexity: false,
            api_surface: false,
            #[cfg(all(feature = "halstead", feature = "content", feature = "walk"))]
            halstead: false,
            #[cfg(feature = "git")]
            churn: false,
            #[cfg(feature = "git")]
            fingerprint: false,
        }
    }

    #[cfg(feature = "walk")]
    #[test]
    fn collect_required_files_limits_ad_hoc_directory_scope() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("src/main.rs"), "fn main() {}\n");
        write_file(&dir.path().join("test/leak.rs"), "// TODO unrelated\n");
        fs::create_dir_all(dir.path().join(".git")).unwrap();

        let mut warnings = Vec::new();
        let files = collect_required_files(
            dir.path(),
            &[PathBuf::from("src")],
            &plan_needing_files(),
            None,
            true,
            &mut warnings,
        )
        .unwrap();

        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
        assert!(warnings.is_empty());
    }

    #[cfg(feature = "walk")]
    #[test]
    fn collect_required_files_limits_ad_hoc_single_file_scope() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("src/main.rs"), "fn main() {}\n");
        write_file(&dir.path().join("test/leak.rs"), "// TODO unrelated\n");
        fs::create_dir_all(dir.path().join(".git")).unwrap();

        let mut warnings = Vec::new();
        let files = collect_required_files(
            dir.path(),
            &[PathBuf::from("src/main.rs")],
            &plan_needing_files(),
            None,
            true,
            &mut warnings,
        )
        .unwrap();

        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
        assert!(warnings.is_empty());
    }

    #[cfg(feature = "walk")]
    #[test]
    fn collect_required_files_dot_scope_preserves_whole_root() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("src/main.rs"), "fn main() {}\n");
        write_file(
            &dir.path().join("test/leak.rs"),
            "// TODO related for dot\n",
        );
        fs::create_dir_all(dir.path().join(".git")).unwrap();

        let mut warnings = Vec::new();
        let files = collect_required_files(
            dir.path(),
            &[PathBuf::from(".")],
            &plan_needing_files(),
            None,
            true,
            &mut warnings,
        )
        .unwrap();

        assert_eq!(
            files,
            vec![PathBuf::from("src/main.rs"), PathBuf::from("test/leak.rs")]
        );
        assert!(warnings.is_empty());
    }

    #[cfg(feature = "walk")]
    #[test]
    fn limited_scopes_select_the_smallest_paths_across_all_inputs() -> Result<()> {
        let dir = tempfile::tempdir()?;
        for path in ["z/last.rs", "a/second.rs", "a/first.rs", "middle.rs"] {
            write_file(&dir.path().join(path), "fn example() {}\n");
        }
        fs::create_dir_all(dir.path().join(".git"))?;

        let mut scopes = vec![
            PathBuf::from("z"),
            PathBuf::from("a"),
            PathBuf::from("middle.rs"),
        ];
        let all = collect_scoped_files(dir.path(), &scopes, None)?;
        for _ in 0..scopes.len() {
            for limit in 0..=all.len() + 1 {
                let expected: Vec<_> = all.iter().take(limit).cloned().collect();
                assert_eq!(
                    collect_scoped_files(dir.path(), &scopes, Some(limit))?,
                    expected,
                    "scopes {scopes:?}, limit {limit}"
                );
            }
            scopes.rotate_left(1);
        }
        Ok(())
    }

    #[cfg(feature = "walk")]
    #[test]
    fn overlapping_scopes_do_not_spend_the_limit_on_duplicates() -> Result<()> {
        let dir = tempfile::tempdir()?;
        write_file(&dir.path().join("src/a.rs"), "fn a() {}\n");
        write_file(&dir.path().join("src/b.rs"), "fn b() {}\n");
        fs::create_dir_all(dir.path().join(".git"))?;
        let scopes = [
            PathBuf::from("src/a.rs"),
            PathBuf::from("src"),
            dir.path().join("src"),
        ];

        assert_eq!(
            collect_scoped_files(dir.path(), &scopes, Some(2))?,
            vec![PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")]
        );
        Ok(())
    }
}
