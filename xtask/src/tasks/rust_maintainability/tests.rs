use super::*;
use serde_json::Value;
use std::fs;

struct Repo(tempfile::TempDir);
impl Repo {
    fn init() -> Result<Self> {
        let repo = Self(tempfile::tempdir()?);
        git(repo.0.path(), &["init", "-q"])?;
        fs::write(repo.0.path().join(".gitattributes"), "*.rs -text\n")?;
        fs::create_dir_all(repo.0.path().join("crates/tokmd-analysis/src/ast"))?;
        Ok(repo)
    }

    fn commit(&self, a: &[u8], b: &[u8]) -> Result<Snapshot> {
        for ((path, _), contents) in PATHS.iter().zip([a, b]) {
            fs::write(self.0.path().join(path), contents)?;
        }
        git(self.0.path(), &["add", "."])?;
        self.commit_index()
    }

    fn commit_index(&self) -> Result<Snapshot> {
        git(
            self.0.path(),
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "fixture",
            ],
        )?;
        let sha = String::from_utf8(git(self.0.path(), &["rev-parse", "HEAD"])?)?;
        snapshot(self.0.path(), sha.trim())
    }
}

fn rows(base: &Snapshot, head: &Snapshot) -> Result<Vec<Value>> {
    let report = serde_json::to_value(compare::comparison(base, head))?;
    Ok(report["rows"].as_array().context("rows")?.clone())
}

fn named(row: &Value, side: &str, name: &str) -> bool {
    row[side]
        .as_array()
        .is_some_and(|entries| entries.iter().any(|entry| entry["id"]["name"] == name))
}

#[test]
fn line_shift_and_same_width_owner_rename() -> Result<()> {
    let repo = Repo::init()?;
    let src = "fn outer() {\n    fn nested() {}\n}\n";
    let old = repo.commit(b"", src.as_bytes())?;
    let shifted = repo.commit(b"", src.replace("    fn", "\n    fn").as_bytes())?;
    let renamed = repo.commit(b"", src.replace("outer", "other").as_bytes())?;
    for (head, status, original, candidate) in [
        (&shifted, "exact", [17, 31], [18, 32]),
        (&renamed, "possible_move_or_rename", [17, 31], [17, 31]),
    ] {
        let result = rows(&old, head)?;
        let row = result
            .iter()
            .find(|row| named(row, "baseline", "nested"))
            .context("nested")?;
        assert_eq!(row["disposition"], status);
        assert_eq!(row["baseline"][0]["bytes"], serde_json::json!(original));
        assert_eq!(row["candidate"][0]["bytes"], serde_json::json!(candidate));
        assert_eq!(
            row["candidate"][0]["id"]["owners"][0],
            if status == "exact" {
                "fn outer ()"
            } else {
                "fn other ()"
            }
        );
    }
    Ok(())
}

#[test]
fn distinguishes_possible_cross_file_move_collisions_and_signature_changes() -> Result<()> {
    let repo = Repo::init()?;
    let old = repo.commit(
        b"fn one<T>(_: T) {} fn a() {} fn b() {} fn param(v: i32) {}",
        b"",
    )?;
    let head = repo.commit(
        b"fn param(v: u32) {}",
        b"fn renamed<T>(_: T) {} fn c() {} fn d() {}",
    )?;
    let result = rows(&old, &head)?;
    let moved = result
        .iter()
        .find(|r| named(r, "baseline", "one"))
        .context("move")?;
    assert_eq!(moved["disposition"], "possible_move_or_rename");
    assert!(named(moved, "candidate", "renamed"));
    assert_ne!(moved["baseline"][0]["path"], moved["candidate"][0]["path"]);
    let ambiguous = result
        .iter()
        .find(|r| named(r, "baseline", "a"))
        .context("ambiguous")?;
    assert_eq!(ambiguous["disposition"], "ambiguous_structural");
    for (side, name) in [("baseline", "b"), ("candidate", "c"), ("candidate", "d")] {
        assert!(named(ambiguous, side, name));
    }
    assert!(
        result.iter().any(
            |r| named(r, "baseline", "param") && r["disposition"] == "baseline_only_unresolved"
        )
    );
    assert!(
        result
            .iter()
            .any(|r| named(r, "candidate", "param") && r["disposition"] == "candidate_only")
    );
    Ok(())
}

#[test]
fn loss_of_file_and_loss_inside_measured_file_stay_distinct() -> Result<()> {
    let repo = Repo::init()?;
    let old = repo.commit(b"fn old() {}", b"fn keep() { if true {} } fn tail() {}")?;
    let broken = repo.commit(b"fn broken(", b"fn keep() { opaque!(); }")?;
    let result = rows(&old, &broken)?;
    assert_eq!(broken.files[0].status, "parse_failed");
    assert!(
        result
            .iter()
            .any(|r| r["disposition"] == "lost_measured_file" && named(r, "baseline", "old"))
    );
    assert!(
        result
            .iter()
            .any(|r| r["exact_nesting_observation"] == "lost_measurement"
                && named(r, "candidate", "keep")
                && r["candidate"][0]["metrics"]["nesting"].is_null())
    );
    assert!(result.iter().any(|r| r["disposition"] == "baseline_only_unresolved" && named(r,"baseline","tail")));
    let invalid = repo.commit(&[0xff], b"#![cfg(test)] fn keep() {}")?;
    assert_eq!(invalid.files[0].status, "invalid_utf8");
    assert_eq!(invalid.files[1].status, "excluded_file_attributes");
    assert!(snapshot(repo.0.path(), "HEAD").is_err());
    fs::write(repo.0.path().join(PATHS[0].0), b"fn local() {}")?;
    assert_eq!(
        baseline_json(&snapshot(repo.0.path(), &old.commit)?)?,
        baseline_json(&old)?
    );
    Ok(())
}

#[test]
fn admitted_syntax_thresholds_scopes_and_unmeasured_cases() -> Result<()> {
    let repo = Repo::init()?;
    let src = "\u{feff}#!/bin/rust\n// café\r\nmod a { pub trait T<X> { fn repeat(&self, x: X); } }\r\nimpl a::T<u8> for S { fn repeat(&self, x: u8) {} }\r\nfn debt(a:u8,b:u8,c:u8,d:u8,e:u8,f:u8,g:u8) { if true { loop { while true { match 0 { _ => {} } } } } }\n#[cfg(test)] mod tests { fn excluded() {} }\nfn hidden() { let _ = || { fn local() {} }; }";
    let base = repo.commit(b"", src.as_bytes())?;
    let head = repo.commit(b"", src.replace("_ => {}", "_ => if true {}").as_bytes())?;
    let result = rows(&base, &head)?;
    assert_eq!(result.len(), 4);
    let debt = result
        .iter()
        .find(|r| named(r, "baseline", "debt"))
        .context("debt")?;
    assert_eq!(debt["exact_nesting_observation"], "worsened_over_threshold");
    assert_eq!(
        debt["candidate"][0]["metrics"],
        serde_json::json!({"parameters":7,"nesting":5})
    );
    for symbol in &base.files[1].symbols {
        let range = symbol.bytes[0]..symbol.bytes[1];
        assert!(
            src.get(range)
                .context("byte span")?
                .starts_with(&format!("fn {}", symbol.id.name))
        );
    }
    let repeat: Vec<_> = base.files[1]
        .symbols
        .iter()
        .filter(|s| s.id.name == "repeat")
        .collect();
    assert_eq!(repeat.len(), 2);
    assert!(repeat.iter().all(|s| s.metrics.parameters == 1));
    assert!(repeat.iter().any(|s| s.metrics.nesting.is_none()));
    assert_ne!(repeat[0].id.owners, repeat[1].id.owners);
    Ok(())
}

#[test]
fn opaque_tokens_and_size_limit_preserve_unknowns() -> Result<()> {
    let repo = Repo::init()?;
    let old = repo.commit(b"fn f() { if true {} } fn pattern(x: i32) {}", b"fn g() {}")?;
    let large = vec![b' '; DEFAULT_MAX_SYNTAX_BYTES + 1];
    let head = repo.commit(b"fn f() { become next(if true {1} else {2}); } fn pattern(x: i32) { match x { const { if true {1} else {2} } => (), _ => () } }", &large)?;
    let result = rows(&old, &head)?;
    assert!(result.iter().any(|r| named(r, "candidate", "pattern")
        && r["exact_nesting_observation"] == "lost_measurement"));
    assert_eq!(head.files[1].status, "size_limit");
    assert!(result.iter().any(
        |r| named(r, "candidate", "f") && r["exact_nesting_observation"] == "lost_measurement"
    ));
    assert!(
        result
            .iter()
            .any(|r| named(r, "baseline", "g") && r["disposition"] == "lost_measured_file")
    );
    Ok(())
}

#[test]
fn absent_and_nonregular_entries_keep_file_provenance_even_without_symbols() -> Result<()> {
    let repo = Repo::init()?;
    let old = repo.commit(b"", b"")?;
    git(repo.0.path(), &["rm", "-q", "--", PATHS[0].0])?;
    let blob = old.files[1].blob.as_deref().context("blob")?;
    git(
        repo.0.path(),
        &["update-index", "--cacheinfo", "120000", blob, PATHS[1].0],
    )?;
    let head = repo.commit_index()?;
    assert_eq!(head.files[0].status, "absent");
    assert_eq!(head.files[1].status, "unsupported_entry");
    let result = rows(&old, &head)?;
    for (row, (path, _)) in result.iter().zip(PATHS) {
        assert_eq!(row["disposition"], "lost_measured_file");
        assert_eq!(row["file_path"], path);
    }
    assert_eq!(result.len(), 2);
    Ok(())
}

#[test]
fn repeated_names_and_duplicate_exact_keys_have_separate_dispositions() -> Result<()> {
    let repo = Repo::init()?;
    let src = b"mod left { fn f() {} } mod right { fn f() {} } fn duplicate() {} fn duplicate() {}";
    let before = repo.commit(src, b"")?;
    let after = repo.commit(src, b"// new blob")?;
    let result = rows(&before, &after)?;
    assert_eq!(
        result
            .iter()
            .filter(|r| r["disposition"] == "exact" && named(r, "candidate", "f"))
            .count(),
        2
    );
    let duplicate = result
        .iter()
        .find(|r| named(r, "baseline", "duplicate"))
        .context("duplicate")?;
    assert_eq!(duplicate["disposition"], "ambiguous_exact_identity");
    assert_eq!(
        duplicate["baseline"]
            .as_array()
            .context("old duplicates")?
            .len(),
        2
    );
    assert_eq!(
        duplicate["candidate"]
            .as_array()
            .context("new duplicates")?
            .len(),
        2
    );
    Ok(())
}
