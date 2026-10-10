//! Inclusion-policy preparation for context file selection.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use tokmd_scan::normalize_slashes as normalize_path;
use tokmd_types::{
    ContextFileRow, FileClassification, FileKind, FileRow, InclusionPolicy, PolicyExcludedFile,
};

use super::{SelectOptions, assign_policy, classify_file, compute_file_cap};

const POLICY_CHARS_PER_TOKEN: usize = 4;
/// Must stay aligned with `tokmd_model::rows::ESTIMATED_BYTES_PER_LINE`.
const ESTIMATED_BYTES_PER_LINE: usize = 40;

/// Tokens used for inclusion-policy caps and density classification.
///
/// Receipt `row.tokens` is a line-derived estimate after the metadata-free model
/// fast path; policy decisions that gate skip/head-tail need on-disk size when
/// the scanned file is readable and the row still carries the line estimate.
fn policy_tokens(path: &str, row: &FileRow) -> usize {
    let estimated_bytes = row.lines.saturating_mul(ESTIMATED_BYTES_PER_LINE);
    if row.bytes != estimated_bytes {
        return row.tokens;
    }

    let Some(meta) = Path::new(path).metadata().ok() else {
        return row.tokens;
    };

    let disk_bytes = meta.len() as usize;
    if disk_bytes > row.bytes {
        disk_bytes / POLICY_CHARS_PER_TOKEN
    } else {
        row.tokens
    }
}

struct FileContextMeta {
    classifications: Vec<FileClassification>,
    policy: InclusionPolicy,
    policy_reason: Option<String>,
    original_tokens: usize,
}

pub(super) struct PolicySelection {
    pub(super) pack_rows: Vec<FileRow>,
    pub(super) excluded_by_policy: Vec<PolicyExcludedFile>,
    file_meta_map: BTreeMap<String, FileContextMeta>,
}

pub(super) fn prepare_policy_selection(
    candidate_rows: &[FileRow],
    budget: usize,
    options: &SelectOptions,
) -> PolicySelection {
    let file_cap = compute_file_cap(budget, options);
    let mut file_meta_map: BTreeMap<String, FileContextMeta> = BTreeMap::new();
    let mut excluded_by_policy: Vec<PolicyExcludedFile> = Vec::new();

    for row in candidate_rows
        .iter()
        .filter(|row| row.kind == FileKind::Parent)
    {
        let path = normalize_path(&row.path);
        let policy_tokens = policy_tokens(&path, row);
        let classifications =
            classify_file(&path, policy_tokens, row.lines, options.dense_threshold);
        let (policy, reason) = assign_policy(policy_tokens, file_cap, &classifications);

        file_meta_map.insert(
            path.clone(),
            FileContextMeta {
                classifications: classifications.clone(),
                policy,
                policy_reason: reason.clone(),
                original_tokens: policy_tokens,
            },
        );

        if matches!(policy, InclusionPolicy::Skip | InclusionPolicy::Summary) {
            excluded_by_policy.push(PolicyExcludedFile {
                path,
                original_tokens: policy_tokens,
                policy,
                reason: reason.unwrap_or_default(),
                classifications,
            });
        }
    }

    let excluded_paths: BTreeSet<&str> = excluded_by_policy
        .iter()
        .map(|file| file.path.as_str())
        .collect();

    let pack_rows = candidate_rows
        .iter()
        .filter(|row| {
            if row.kind != FileKind::Parent {
                return true;
            }
            let path = normalize_path(&row.path);
            !excluded_paths.contains(path.as_str())
        })
        .map(|row| {
            let path = normalize_path(&row.path);
            if row.kind == FileKind::Parent
                && let Some(meta) = file_meta_map.get(&path)
            {
                // Packing must use the same observation as the policy decision,
                // including full files that remain below the per-file cap.
                let tokens = if meta.policy == InclusionPolicy::HeadTail {
                    meta.original_tokens.min(file_cap)
                } else {
                    meta.original_tokens
                };
                return FileRow {
                    tokens,
                    ..row.clone()
                };
            }
            row.clone()
        })
        .collect();

    PolicySelection {
        pack_rows,
        excluded_by_policy,
        file_meta_map,
    }
}

impl PolicySelection {
    pub(super) fn annotate_selected(&self, selected: &mut [ContextFileRow]) {
        for file in selected {
            let path = normalize_path(&file.path);
            if let Some(meta) = self.file_meta_map.get(&path) {
                file.classifications = meta.classifications.clone();
                file.policy = meta.policy;
                file.policy_reason = meta.policy_reason.clone();
                if meta.policy == InclusionPolicy::HeadTail {
                    file.effective_tokens = Some(file.tokens);
                    file.tokens = meta.original_tokens;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_row(path: &str, tokens: usize, lines: usize) -> FileRow {
        FileRow {
            path: path.to_string(),
            module: "src".to_string(),
            lang: "Rust".to_string(),
            kind: FileKind::Parent,
            code: lines,
            comments: 0,
            blanks: 0,
            lines,
            bytes: tokens,
            tokens,
        }
    }

    fn selected_row(path: &str, tokens: usize) -> ContextFileRow {
        ContextFileRow {
            path: path.to_string(),
            module: "src".to_string(),
            lang: "Rust".to_string(),
            tokens,
            code: 1,
            lines: 1,
            bytes: tokens,
            value: tokens,
            rank_reason: "test".to_string(),
            policy: InclusionPolicy::Full,
            effective_tokens: None,
            policy_reason: None,
            classifications: Vec::new(),
        }
    }

    #[test]
    fn head_tail_rows_are_capped_for_packing_and_restored_on_annotation() {
        let options = SelectOptions {
            max_file_pct: 0.10,
            max_file_tokens: Some(100),
            ..Default::default()
        };
        let selection =
            prepare_policy_selection(&[file_row("src/big.rs", 150, 150)], 1_000, &options);

        assert_eq!(selection.excluded_by_policy.len(), 0);
        assert_eq!(selection.pack_rows[0].tokens, 100);

        let mut selected = vec![selected_row("src/big.rs", 100)];
        selection.annotate_selected(&mut selected);
        assert_eq!(selected[0].policy, InclusionPolicy::HeadTail);
        assert_eq!(selected[0].effective_tokens, Some(100));
        assert_eq!(selected[0].tokens, 150);
    }

    #[test]
    fn oversized_generated_rows_are_excluded_from_packing() {
        let options = SelectOptions {
            max_file_pct: 0.10,
            max_file_tokens: Some(100),
            ..Default::default()
        };
        let selection = prepare_policy_selection(
            &[file_row("src/generated.pb.rs", 150, 150)],
            1_000,
            &options,
        );

        assert!(selection.pack_rows.is_empty());
        assert_eq!(selection.excluded_by_policy.len(), 1);
        assert_eq!(
            selection.excluded_by_policy[0].policy,
            InclusionPolicy::Skip
        );
    }

    #[test]
    fn full_rows_use_policy_tokens_without_mutating_inventory() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("ordinary.rs");
        std::fs::write(&path, vec![b'x'; 5_600])?;
        let path = path.to_string_lossy();
        let mut row = file_row(&path, 500, 50);
        row.bytes = 2_000;
        let selection = prepare_policy_selection(
            std::slice::from_ref(&row),
            10_000,
            &SelectOptions::default(),
        );
        let packed = selection
            .pack_rows
            .first()
            .ok_or_else(|| anyhow::anyhow!("ordinary full file was excluded"))?;
        anyhow::ensure!(packed.tokens == 1_400, "full file retained a stale charge");
        anyhow::ensure!(row.tokens == 500 && row.bytes == 2_000, "inventory mutated");
        let mut selected = vec![selected_row(&path, packed.tokens)];
        selection.annotate_selected(&mut selected);
        let file = selected
            .first()
            .ok_or_else(|| anyhow::anyhow!("selected row disappeared"))?;
        anyhow::ensure!(file.policy == InclusionPolicy::Full, "unexpected policy");
        anyhow::ensure!(file.tokens == 1_400, "annotation changed full-file charge");
        anyhow::ensure!(file.effective_tokens.is_none(), "unexpected partial charge");
        Ok(())
    }

    #[test]
    fn measured_rows_and_children_keep_their_supplied_charges() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("ordinary.rs");
        std::fs::write(&path, vec![b'x'; 8_000])?;
        let path = path.to_string_lossy();
        let mut measured = file_row(&path, 1_234, 50);
        measured.bytes = 5_600;
        let child = FileRow {
            kind: FileKind::Child,
            tokens: 17,
            ..measured.clone()
        };
        let selection =
            prepare_policy_selection(&[measured, child], 10_000, &SelectOptions::default());
        let charges = selection
            .pack_rows
            .iter()
            .map(|row| row.tokens)
            .collect::<Vec<_>>();
        anyhow::ensure!(charges == vec![1_234, 17], "supplied charges changed");
        Ok(())
    }

    #[test]
    fn smaller_and_missing_files_keep_inventory_fallback() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("ordinary.rs");
        std::fs::write(&path, b"fn f() {}\n")?;
        let path_string = path.to_string_lossy();
        let mut row = file_row(&path_string, 500, 50);
        row.bytes = 2_000;
        for missing in [false, true] {
            if missing {
                std::fs::remove_file(&path)?;
            }
            let selection = prepare_policy_selection(
                std::slice::from_ref(&row),
                10_000,
                &SelectOptions::default(),
            );
            let packed = selection
                .pack_rows
                .first()
                .ok_or_else(|| anyhow::anyhow!("fallback file was excluded"))?;
            anyhow::ensure!(packed.tokens == 500, "inventory fallback changed");
        }
        Ok(())
    }
}
