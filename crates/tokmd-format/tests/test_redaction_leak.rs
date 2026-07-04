use std::io::Cursor;
use std::path::PathBuf;

use tokmd_format::{redact_path, short_hash, write_export_jsonl_to};
use tokmd_settings::ScanOptions;
use tokmd_types::{
    ChildIncludeMode, ExportArgs, ExportData, ExportFormat, FileKind, FileRow, RedactMode,
};

#[test]
fn test_redact_path_leak() {
    for leaked_data in ["super_secret_password_123", "passwd", "secret", "pass1234"] {
        let path = format!("file.{}", leaked_data);
        let redacted = redact_path(&path);
        assert!(
            !redacted.contains(leaked_data),
            "Path redaction leaked extension {leaked_data:?}: {redacted}"
        );
    }
}

#[test]
fn redaction_preserves_known_compound_archive_suffix() {
    let redacted = redact_path("archive.tar.gz");
    assert!(redacted.ends_with(".tar.gz"));
}

#[test]
fn redaction_preserves_only_final_extension_for_unknown_safe_chains() {
    let redacted = redact_path("fixture.json.rs");
    assert!(redacted.ends_with(".rs"));
    assert!(!redacted.ends_with(".json.rs"));
}

#[test]
fn redaction_drops_suffixes_when_final_extension_is_unsafe() {
    let redacted = redact_path("secret.rs.bak");
    assert_eq!(redacted.len(), 16);
    assert!(!redacted.contains(".rs"));
    assert!(!redacted.contains(".bak"));
}

#[test]
fn redaction_normalizes_safe_extension_case() {
    let redacted = redact_path("file.JSON");
    assert!(redacted.ends_with(".json"));
    assert!(!redacted.ends_with(".JSON"));
}

#[test]
fn redaction_normalizes_known_compound_archive_suffix_case() {
    let redacted = redact_path("archive.TAR.GZ");
    assert!(redacted.ends_with(".tar.gz"));
    assert!(!redacted.ends_with(".TAR.GZ"));
}

#[test]
fn strip_prefix_redaction_uses_short_hash_without_extension_leak() {
    let prefix = "myproject.super_secret_password_123";
    let export = ExportData {
        rows: vec![FileRow {
            path: "myproject/src/main.rs".to_string(),
            module: "myproject".to_string(),
            lang: "Rust".to_string(),
            kind: FileKind::Parent,
            code: 10,
            comments: 1,
            blanks: 1,
            lines: 12,
            bytes: 100,
            tokens: 20,
        }],
        module_roots: vec!["myproject".to_string()],
        module_depth: 1,
        children: ChildIncludeMode::Separate,
    };
    let args = ExportArgs {
        paths: vec![PathBuf::from(".")],
        format: ExportFormat::Jsonl,
        output: None,
        module_roots: vec!["myproject".to_string()],
        module_depth: 1,
        children: ChildIncludeMode::Separate,
        min_code: 0,
        max_rows: 0,
        meta: true,
        redact: RedactMode::Paths,
        strip_prefix: Some(PathBuf::from(prefix)),
    };

    let mut buffer = Cursor::new(Vec::new());
    assert!(
        write_export_jsonl_to(&mut buffer, &export, &ScanOptions::default(), &args).is_ok(),
        "export jsonl must succeed"
    );

    let output = match String::from_utf8(buffer.into_inner()) {
        Ok(output) => output,
        Err(_) => {
            assert!(false, "output must be valid UTF-8");
            return;
        }
    };
    let meta_line = match output.lines().next() {
        Some(line) => line,
        None => {
            assert!(false, "meta line must exist");
            return;
        }
    };
    let meta = match serde_json::from_str::<serde_json::Value>(meta_line) {
        Ok(meta) => meta,
        Err(_) => {
            assert!(false, "meta line must parse as JSON");
            return;
        }
    };
    let redacted = match meta
        .get("args")
        .and_then(|args| args.get("strip_prefix"))
        .and_then(|value| value.as_str())
    {
        Some(redacted) => redacted,
        None => {
            assert!(false, "strip_prefix must be a JSON string");
            return;
        }
    };
    assert_eq!(
        redacted,
        short_hash(prefix),
        "strip_prefix redaction must use short_hash, not redact_path"
    );
    assert_eq!(redacted.len(), 16, "redacted strip_prefix must be opaque");
    assert!(
        !redacted.contains("super_secret_password_123"),
        "strip_prefix redaction leaked extension content: {redacted}"
    );
    assert!(
        !redacted.contains('.'),
        "strip_prefix redaction must not preserve file extensions: {redacted}"
    );
}
