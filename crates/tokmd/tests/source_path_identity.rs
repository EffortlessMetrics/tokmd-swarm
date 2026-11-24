//! Filesystem-to-export identity through the actual CLI and each public format.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, Result, ensure};
use serde_json::Value;

const FORMATS: [&str; 4] = ["json", "jsonl", "csv", "cyclonedx"];

fn export(root: &Path, format: &str, extra: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_tokmd"))
        .current_dir(root)
        .args(["export", "--config", "none", "--format", format])
        .args(extra)
        .output()
}

fn success(output: Output) -> Result<String> {
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

// Independently enumerate actual directory entries and their native parents.
fn enumerate(dir: &Path, prefix: &str, entries: &mut Vec<(String, String)>) -> Result<()> {
    for item in fs::read_dir(dir)? {
        let item = item?;
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("invalid UTF-8 in supported-name fixture"))?;
        if item.file_type()?.is_dir() {
            enumerate(&item.path(), &name, entries)?;
        } else if prefix.is_empty() {
            entries.push((name, "(root)".into()));
        } else {
            entries.push((format!("{prefix}/{name}"), prefix.into()));
        }
    }
    Ok(())
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("expected string {key} in {value}"))
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("expected array {key} in {value}"))
}

fn assert_inventory(format: &str, body: &str, expected: &[(String, String)]) -> Result<()> {
    // Compare full CSV wire records: both commas and newlines can occur in a field.
    if format == "csv" {
        let mut csv =
            String::from("path,module,lang,kind,code,comments,blanks,lines,bytes,tokens\n");
        for (path, module) in expected {
            let field = if path.contains([',', '"', '\n']) {
                format!("\"{}\"", path.replace('"', "\"\""))
            } else {
                path.clone()
            };
            csv.push_str(&format!("{field},{module},Rust,parent,0,0,0,0,0,0\n"));
        }
        assert_eq!(body, csv);
        return Ok(());
    }
    let rows: Vec<Value> = match format {
        "json" => {
            let receipt = serde_json::from_str(body)?;
            assert_eq!(field(&receipt, "status")?, "complete");
            array(&receipt, "rows")?.clone()
        }
        "jsonl" => {
            let mut records = body.lines().map(serde_json::from_str::<Value>);
            let meta = records.next().context("missing JSONL metadata")??;
            assert_eq!(field(&meta, "type")?, "meta");
            assert_eq!(field(&meta, "status")?, "complete");
            records
                .map(|row| {
                    let row = row?;
                    assert_eq!(field(&row, "type")?, "row");
                    Ok(row)
                })
                .collect::<Result<_>>()?
        }
        "cyclonedx" => array(&serde_json::from_str(body)?, "components")?.clone(),
        _ => anyhow::bail!("unknown fixture format {format}"),
    };
    let (path, module) = if format == "cyclonedx" {
        ("name", "group")
    } else {
        ("path", "module")
    };
    let actual = rows
        .iter()
        .map(|r| Ok((field(r, path)?.to_owned(), field(r, module)?.to_owned())))
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(actual, expected, "{format}");
    Ok(())
}

#[test]
fn exported_rows_match_native_source_names_and_module_groups_in_order() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    fs::create_dir(root.join("literal"))?;
    // On Windows the native join uses a backslash, and the exported separator is '/'.
    fs::File::create_new(root.join("literal").join("name.rs"))?;
    for name in ["comma,x.rs", "日本語.rs", "collision-�.rs", "é.rs"] {
        fs::File::create_new(root.join(name))?;
    }
    if let Err(err) = fs::File::create_new(root.join("e\u{301}.rs")) {
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        eprintln!("Fixture filesystem aliases composed/decomposed é; testing the stored name");
    }
    #[cfg(unix)]
    for name in [r"literal\name.rs", "quote\".rs", "tab\t.rs", "new\nline.rs"] {
        fs::File::create_new(root.join(name))?;
    }
    let mut expected = Vec::new();
    enumerate(root, "", &mut expected)?;
    expected.sort_unstable();
    for format in FORMATS {
        for _ in 0..2 {
            assert_inventory(format, &success(export(root, format, &[])?)?, &expected)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn exclusion_and_redaction_distinguish_native_backslash_from_separator() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    fs::create_dir(root.join("literal"))?;
    fs::File::create_new(root.join("literal/name.rs"))?;
    fs::File::create_new(root.join(r"literal\name.rs"))?;

    let filtered = success(export(root, "json", &["--exclude", "literal/name.rs"])?)?;
    assert_inventory(
        "json",
        &filtered,
        &[(r"literal\name.rs".into(), "(root)".into())],
    )?;
    for pattern in [r"literal\\name.rs", r"literal[\\]name.rs"] {
        let filtered = success(export(root, "json", &["--exclude", pattern])?)?;
        assert_inventory(
            "json",
            &filtered,
            &[("literal/name.rs".into(), "literal".into())],
        )?;
    }

    let mut excluded_hashes = std::collections::BTreeSet::new();
    for pattern in ["literal/name.rs", r"literal\\name.rs"] {
        let body = success(export(
            root,
            "json",
            &["--exclude", pattern, "--redact", "paths"],
        )?)?;
        let receipt: Value = serde_json::from_str(&body)?;
        let label = receipt
            .pointer("/scan/excluded/0")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing redacted exclusion label"))?;
        assert!(
            excluded_hashes.insert(label.to_owned()),
            "distinct exclusions share a hash"
        );
    }

    let redacted = success(export(root, "json", &["--redact", "paths"])?)?;
    let receipt = serde_json::from_str(&redacted)?;
    let rows = array(&receipt, "rows")?;
    let [first, second] = rows.as_slice() else {
        anyhow::bail!("expected two distinct redacted rows: {rows:?}");
    };
    let (first, second) = (field(first, "path")?, field(second, "path")?);
    assert_ne!(first, second, "redacted path identities collided");
    assert!(
        [first, second]
            .iter()
            .all(|name| name.ends_with(".rs") && !name.contains("literal"))
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn unsupported_byte_names_fail_before_each_export_and_identify_the_offending_component()
-> Result<()> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let ff = b"collision-\xff.rs".as_slice();
    let fe = b"collision-\xfe.rs".as_slice();
    let cases: &[&[&[u8]]] = &[&[ff, fe], &[ff], &[fe]];
    for present in cases {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        fs::File::create_new(root.join("collision-�.rs"))?;
        let mut ids = Vec::new();
        for bytes in *present {
            fs::File::create_new(root.join(OsStr::from_bytes(bytes)))?;
            ids.push(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
        }
        let mut prior = None;
        for format in FORMATS {
            let out = export(root, format, &[])?;
            assert!(!out.status.success(), "{format} accepted unsupported path");
            assert!(out.stdout.is_empty(), "{format} emitted a partial receipt");
            let error = String::from_utf8(out.stderr)?;
            assert!(error.contains("Unsupported source path"), "{error}");
            let lower = error.to_lowercase();
            assert!(
                lower.contains("rename") && lower.contains("retry"),
                "{error}"
            );
            assert!(ids.iter().any(|id| error.contains(id)), "{error}");
            assert!(
                !error.contains(root.to_str().context("non-UTF-8 fixture root")?),
                "{error}"
            );
            if let Some(previous) = &prior {
                assert_eq!(&error, previous, "unsupported outcome differs by format");
            }
            prior = Some(error);
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn context_bundle_reads_each_advertised_path_under_an_absolute_native_root() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().join(r"source\root");
    let out = dir.path().join("bundle");
    fs::create_dir_all(root.join("literal"))?;
    fs::write(root.join("literal/name.rs"), "fn nested() {}\n")?;
    fs::write(root.join(r"literal\name.rs"), "fn literal() {}\n")?;

    success(
        Command::new(env!("CARGO_BIN_EXE_tokmd"))
            .current_dir(dir.path())
            .args([
                "context",
                "--no-git",
                "--config",
                "none",
                "--mode",
                "bundle",
                "--output-dir",
            ])
            .arg(&out)
            .arg(&root)
            .output()?,
    )?;

    let bundle = fs::read_to_string(out.join("bundle.txt"))?;
    for (path, body) in [
        ("literal/name.rs", "fn nested() {}"),
        (r"literal\name.rs", "fn literal() {}"),
    ] {
        assert!(
            bundle.contains(&format!("/{path} ===\n{body}\n")),
            "{bundle}"
        );
    }
    let receipt = serde_json::from_slice(&fs::read(out.join("receipt.json"))?)?;
    let manifest = serde_json::from_slice(&fs::read(out.join("manifest.json"))?)?;
    assert_eq!(array(&receipt, "files")?.len(), 2);
    assert_eq!(
        array(&receipt, "files")?,
        array(&manifest, "included_files")?
    );
    let artifact = array(&manifest, "artifacts")?
        .iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some("bundle"))
        .context("missing bundle artifact")?;
    assert_eq!(
        artifact.pointer("/hash/hash").and_then(Value::as_str),
        Some(blake3::hash(bundle.as_bytes()).to_hex().as_str())
    );
    Ok(())
}

#[cfg(all(unix, feature = "git"))]
#[test]
fn context_ranking_reports_fallback_for_quoted_git_paths() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    let git = |args: &[&str]| -> Result<()> {
        success(
            Command::new("git")
                .current_dir(root)
                .args([
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                ])
                .args(args)
                .output()?,
        )?;
        Ok(())
    };
    let rank = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_tokmd"))
            .current_dir(root)
            .args([
                "context",
                "--mode",
                "json",
                "--config",
                "none",
                "--rank-by",
                "hotspot",
            ])
            .args(extra)
            .output()
    };
    git(&["init", "--quiet"])?;
    fs::create_dir(root.join("literal"))?;
    fs::write(root.join("literal/name.rs"), "fn nested() {}\n")?;
    git(&["add", "."])?;
    git(&["commit", "--quiet", "-m", "nested"])?;
    let ranked = serde_json::from_str(&success(rank(&["--require-git-scores"])?)?)?;
    let [only] = array(&ranked, "files")?.as_slice() else {
        anyhow::bail!("expected the tracked nested file");
    };
    assert_eq!(field(only, "rank_reason")?, "hotspot");

    fs::write(root.join(r"literal\name.rs"), "fn literal() {}\n")?;
    git(&["add", "."])?;
    git(&["commit", "--quiet", "-m", "literal"])?;
    let fallback = serde_json::from_str(&success(rank(&[])?)?)?;
    assert_eq!(field(&fallback, "rank_by_effective")?, "code");
    assert!(field(&fallback, "fallback_reason")?.contains("git scores"));
    let mut names = array(&fallback, "files")?
        .iter()
        .map(|row| field(row, "path"))
        .collect::<Result<Vec<_>>>()?;
    names.sort_unstable();
    assert_eq!(names, ["literal/name.rs", r"literal\name.rs"]);
    let required = rank(&["--require-git-scores"])?;
    assert!(!required.status.success());
    assert!(required.stdout.is_empty());
    assert!(String::from_utf8(required.stderr)?.contains("Git scores required"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn check_ignore_explains_the_same_native_path_as_export() -> Result<()> {
    use std::os::unix::ffi::OsStrExt;

    let temp = tempfile::tempdir()?;
    let root = temp.path().join(r"source[ab]\root");
    fs::create_dir_all(root.join("sub/literal"))?;
    let literal = r"sub/literal\name.rs";
    let nested = "sub/literal/name.rs";
    for path in [literal, nested] {
        fs::File::create_new(root.join(path))?;
    }
    // The rule is relative to its source. A root-level basename with the same
    // spelling is a positive control for overly broad .tokeignore matching.
    fs::File::create_new(root.join(r"literal\name.rs"))?;
    for file_rule in [false, true] {
        for (pattern, ignored, kept) in [
            (r"literal\\name.rs", literal, nested),
            ("literal/name.rs", nested, literal),
        ] {
            let cli_pattern = format!("sub/{pattern}");
            let flags = if file_rule {
                fs::write(root.join("sub/.tokeignore"), format!("/{pattern}\n"))?;
                vec![]
            } else {
                vec!["--exclude", cli_pattern.as_str()]
            };
            let inventory = serde_json::from_str(&success(export(&root, "json", &flags)?)?)?;
            let paths = array(&inventory, "rows")?
                .iter()
                .map(|row| field(row, "path"))
                .collect::<Result<Vec<_>>>()?;
            assert!(
                paths.contains(&kept)
                    && paths.contains(&r"literal\name.rs")
                    && !paths.contains(&ignored),
                "{paths:?}"
            );
            for path in [literal, nested, r"literal\name.rs"] {
                let output = Command::new(env!("CARGO_BIN_EXE_tokmd"))
                    .current_dir(&root)
                    .args(&flags)
                    .args(["check-ignore", "-v", path])
                    .output()?;
                let expected = if path == ignored {
                    let reason = if file_rule {
                        format!("sub/.tokeignore: /{pattern}")
                    } else {
                        format!("--exclude: {cli_pattern}")
                    };
                    format!("{path}: ignored\n  {reason}\n")
                } else {
                    format!("{path}: not ignored\n")
                };
                assert_eq!(
                    output.status.code(),
                    Some(i32::from(path != ignored)),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(String::from_utf8(output.stdout)?, expected);
            }
        }
    }
    let raw = std::ffi::OsStr::from_bytes(b"bad-\xff.rs");
    fs::File::create_new(root.join(raw))?;
    let unsupported = Command::new(env!("CARGO_BIN_EXE_tokmd"))
        .current_dir(&root)
        .arg("check-ignore")
        .arg(raw)
        .output()?;
    assert!(!unsupported.status.success() && unsupported.stdout.is_empty());
    let diagnostic = String::from_utf8(unsupported.stderr)?;
    assert!(
        diagnostic.contains("Unsupported source path") && diagnostic.contains("6261642dff2e7273"),
        "{diagnostic}"
    );
    Ok(())
}
