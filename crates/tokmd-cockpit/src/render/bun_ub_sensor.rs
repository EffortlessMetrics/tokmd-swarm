//! Bun UB sensor artifact references for review packets.

use std::path::Path;

use crate::CockpitReceipt;

pub const BUN_UB_ANALYZE_MD_PATH: &str = "sensors/tokmd/analyze.md";
pub const BUN_UB_ANALYZE_JSON_PATH: &str = "sensors/tokmd/analyze.json";

const BUN_UB_SCOPE_PREFIXES: &[&str] = &[
    "src/runtime/api/",
    "src/bun.js/bindings/",
    "src/bun.js/api/",
];

/// Availability for the default Bun UB analyze artifacts relative to a repo root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BunUbSensorEvidence {
    md_available: bool,
    json_available: bool,
}

impl BunUbSensorEvidence {
    pub fn from_repo_root(repo_root: &Path) -> Self {
        Self {
            md_available: repo_relative_path(repo_root, BUN_UB_ANALYZE_MD_PATH).is_file(),
            json_available: repo_relative_path(repo_root, BUN_UB_ANALYZE_JSON_PATH).is_file(),
        }
    }

    pub(super) fn missing() -> Self {
        Self {
            md_available: false,
            json_available: false,
        }
    }

    pub(super) fn status(&self) -> &'static str {
        if self.md_available && self.json_available {
            "available"
        } else {
            "missing"
        }
    }

    pub(super) fn available_paths(&self) -> Vec<&'static str> {
        let mut paths = Vec::new();
        if self.md_available {
            paths.push(BUN_UB_ANALYZE_MD_PATH);
        }
        if self.json_available {
            paths.push(BUN_UB_ANALYZE_JSON_PATH);
        }
        paths
    }

    pub(super) fn missing_paths(&self) -> Vec<&'static str> {
        let mut paths = Vec::new();
        if !self.md_available {
            paths.push(BUN_UB_ANALYZE_MD_PATH);
        }
        if !self.json_available {
            paths.push(BUN_UB_ANALYZE_JSON_PATH);
        }
        paths
    }
}

pub(super) fn receipt_has_bun_ub_scope(receipt: &CockpitReceipt) -> bool {
    receipt
        .review_plan
        .iter()
        .any(|item| review_item_is_bun_ub_scope(&item.path))
}

pub(super) fn review_item_is_bun_ub_scope(path: &str) -> bool {
    let normalized = normalize_review_path(path);
    BUN_UB_SCOPE_PREFIXES.iter().any(|prefix| {
        let dir = prefix.trim_end_matches('/');
        normalized == dir || normalized.starts_with(prefix)
    })
}

pub(super) fn bun_ub_sensor_refs() -> [&'static str; 2] {
    [BUN_UB_ANALYZE_MD_PATH, BUN_UB_ANALYZE_JSON_PATH]
}

pub(super) fn bun_ub_analyze_commands(path: &str, base_ref: &str, head_ref: &str) -> [String; 2] {
    [
        format!(
            "tokmd analyze --preset bun-ub --format md --effort-base-ref {base_ref} --effort-head-ref {head_ref} --no-progress {path} > {BUN_UB_ANALYZE_MD_PATH}"
        ),
        format!(
            "tokmd analyze --preset bun-ub --format json --effort-base-ref {base_ref} --effort-head-ref {head_ref} --no-progress {path} > {BUN_UB_ANALYZE_JSON_PATH}"
        ),
    ]
}

fn repo_relative_path(repo_root: &Path, rel: &str) -> std::path::PathBuf {
    rel.split('/')
        .fold(repo_root.to_path_buf(), |path, part| path.join(part))
}

fn normalize_review_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    normalized.trim_start_matches("./").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bun_ub_scope_matches_runtime_api_paths() {
        assert!(review_item_is_bun_ub_scope(
            "src/runtime/api/MarkdownObject.rs"
        ));
        assert!(review_item_is_bun_ub_scope("./src/runtime/api"));
    }

    #[test]
    fn bun_ub_scope_matches_bun_bindings_and_api_paths() {
        assert!(review_item_is_bun_ub_scope(
            "src/bun.js/bindings/webcore.cpp"
        ));
        assert!(review_item_is_bun_ub_scope("src/bun.js/api/server.zig"));
    }

    #[test]
    fn bun_ub_scope_rejects_unrelated_paths() {
        assert!(!review_item_is_bun_ub_scope(
            "test/cli/install/dangling-symlink"
        ));
        assert!(!review_item_is_bun_ub_scope("src/runtime/internal.rs"));
    }

    #[test]
    fn bun_ub_analyze_commands_target_default_sensor_paths() {
        let commands = bun_ub_analyze_commands("src/runtime/api/MarkdownObject.rs", "main", "HEAD");
        assert_eq!(
            commands[0],
            "tokmd analyze --preset bun-ub --format md --effort-base-ref main --effort-head-ref HEAD --no-progress src/runtime/api/MarkdownObject.rs > sensors/tokmd/analyze.md"
        );
        assert_eq!(
            commands[1],
            "tokmd analyze --preset bun-ub --format json --effort-base-ref main --effort-head-ref HEAD --no-progress src/runtime/api/MarkdownObject.rs > sensors/tokmd/analyze.json"
        );
    }
}
