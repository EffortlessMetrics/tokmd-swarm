//! Tests for the experimental [`RepoSnapshot`] portability seam.
//!
//! These cover the minimal proof obligations from
//! `docs/specs/repo-snapshot.md` that this slice implements:
//! host/in-memory parity, deterministic enumeration independent of insertion
//! order, path normalization to the forward-slash rule, reading bytes, and
//! listing files through the snapshot view.

use std::path::PathBuf;

use tokmd_io_port::{HostFs, MemFs, RepoSnapshot};

// ---------------------------------------------------------------------------
// Path normalization
// ---------------------------------------------------------------------------

#[test]
fn normalizes_backslash_root_to_forward_slashes() {
    let fs = MemFs::new();
    // Root normalization is pure string work, so a Windows-style backslash root
    // is normalized deterministically on every platform.
    let snapshot = RepoSnapshot::builder(&fs, "repo\\nested\\root").build();
    assert_eq!(snapshot.root(), "repo/nested/root");
    assert!(snapshot.is_empty());
}

#[test]
fn snapshot_paths_use_forward_slashes() {
    let mut fs = MemFs::new();
    fs.add_file(PathBuf::from("src/a.rs"), "a");

    let mut builder = RepoSnapshot::builder(&fs, ".");
    builder.add_path("src/a.rs").unwrap();
    let snapshot = builder.build();

    let only = snapshot.paths().collect::<Vec<_>>();
    assert_eq!(only, vec!["src/a.rs"]);
    assert!(!only[0].contains('\\'));
}

#[test]
fn strips_leading_dot_slash_and_root_slash() {
    let mut fs = MemFs::new();
    fs.add_file(PathBuf::from("./x.txt"), "x");

    let mut builder = RepoSnapshot::builder(&fs, "./root");
    builder.add_path("./x.txt").unwrap();
    let snapshot = builder.build();

    assert_eq!(snapshot.root(), "root");
    assert_eq!(snapshot.paths().collect::<Vec<_>>(), vec!["x.txt"]);
}

// ---------------------------------------------------------------------------
// Read bytes / list files via snapshot view
// ---------------------------------------------------------------------------

#[test]
fn captures_bytes_and_length() {
    let mut fs = MemFs::new();
    fs.add_bytes(PathBuf::from("blob.bin"), vec![0xDE, 0xAD, 0xBE, 0xEF]);

    let mut builder = RepoSnapshot::builder(&fs, ".");
    builder.add_path("blob.bin").unwrap();
    let snapshot = builder.build();

    let entry = snapshot.get("blob.bin").expect("entry present");
    assert_eq!(entry.bytes(), &[0xDE, 0xAD, 0xBE, 0xEF]);
    assert_eq!(entry.len(), 4);
    assert!(!entry.is_empty());
}

#[test]
fn empty_file_is_empty() {
    let mut fs = MemFs::new();
    fs.add_bytes(PathBuf::from("empty"), Vec::<u8>::new());

    let mut builder = RepoSnapshot::builder(&fs, ".");
    builder.add_path("empty").unwrap();
    let snapshot = builder.build();

    let entry = snapshot.get("empty").unwrap();
    assert_eq!(entry.len(), 0);
    assert!(entry.is_empty());
}

#[test]
fn lists_files_in_sorted_order_independent_of_insertion() {
    let mut fs = MemFs::new();
    fs.add_file(PathBuf::from("z.rs"), "z");
    fs.add_file(PathBuf::from("a.rs"), "a");
    fs.add_file(PathBuf::from("m.rs"), "m");

    // Insert in reverse-sorted order; enumeration must still be sorted.
    let mut builder = RepoSnapshot::builder(&fs, ".");
    builder.add_paths(["z.rs", "m.rs", "a.rs"]).unwrap();
    let snapshot = builder.build();

    assert_eq!(snapshot.len(), 3);
    assert_eq!(
        snapshot.paths().collect::<Vec<_>>(),
        vec!["a.rs", "m.rs", "z.rs"]
    );
}

#[test]
fn re_adding_path_overwrites_entry() {
    let mut fs = MemFs::new();
    fs.add_file(PathBuf::from("f.txt"), "first");

    let mut builder = RepoSnapshot::builder(&fs, ".");
    builder.add_path("f.txt").unwrap();
    let snapshot = builder.build();
    assert_eq!(snapshot.get("f.txt").unwrap().bytes(), b"first");

    // A fresh builder over updated provider state captures the new bytes.
    fs.add_file(PathBuf::from("f.txt"), "second");
    let mut builder = RepoSnapshot::builder(&fs, ".");
    builder.add_path("f.txt").unwrap();
    builder.add_path("f.txt").unwrap();
    let snapshot = builder.build();

    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot.get("f.txt").unwrap().bytes(), b"second");
}

#[test]
fn contains_uses_normalized_lookup() {
    let mut fs = MemFs::new();
    fs.add_file(PathBuf::from("src/lib.rs"), "x");

    let mut builder = RepoSnapshot::builder(&fs, ".");
    builder.add_path("src/lib.rs").unwrap();
    let snapshot = builder.build();

    assert!(snapshot.contains("src/lib.rs"));
    assert!(snapshot.contains("./src/lib.rs"));
    assert!(!snapshot.contains("src/main.rs"));
}

// ---------------------------------------------------------------------------
// Fail-closed on missing entry
// ---------------------------------------------------------------------------

#[test]
fn missing_path_fails_closed_with_named_error() {
    let fs = MemFs::new();
    let mut builder = RepoSnapshot::builder(&fs, ".");
    let msg = match builder.add_path("nope.rs") {
        Ok(_) => panic!("expected fail-closed error for missing path"),
        Err(err) => err.to_string(),
    };
    assert!(msg.contains("nope.rs"), "error names the path: {msg}");
    assert!(msg.contains("not found"), "error names the cause: {msg}");
}

// ---------------------------------------------------------------------------
// Host / in-memory parity
// ---------------------------------------------------------------------------

#[test]
fn host_and_mem_snapshots_match_for_same_fixture() {
    // Fixture: a small tree of files with identical contents in both backends.
    let fixture: &[(&str, &[u8])] = &[
        ("src/main.rs", b"fn main() {}"),
        ("src/util/helpers.rs", b"pub fn h() {}"),
        ("README.md", b"# demo"),
        ("data/blob.bin", &[0x00, 0x01, 0x02, 0xFF]),
    ];

    // MemFs snapshot, rooted logically at ".".
    let mut mem = MemFs::new();
    for (path, bytes) in fixture {
        mem.add_bytes(PathBuf::from(*path), bytes.to_vec());
    }
    let mut mem_builder = RepoSnapshot::builder(&mem, ".");
    mem_builder
        .add_paths(fixture.iter().map(|(p, _)| *p))
        .unwrap();
    let mem_snapshot = mem_builder.build();

    // HostFs snapshot over the same fixture written to a temp dir.
    let dir = tempfile::tempdir().unwrap();
    let mut host_paths = Vec::new();
    for (path, bytes) in fixture {
        let full = dir.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, bytes).unwrap();
        host_paths.push(full);
    }
    let host = HostFs;
    let mut host_builder = RepoSnapshot::builder(&host, dir.path());
    for full in &host_paths {
        host_builder.add_path(full).unwrap();
    }
    let host_snapshot = host_builder.build();

    for (logical, bytes) in fixture {
        // MemFs side: exact normalized key.
        let mem_entry = mem_snapshot
            .get(logical)
            .unwrap_or_else(|| panic!("mem missing {logical}"));
        assert_eq!(mem_entry.bytes(), *bytes);

        // HostFs side: find the entry whose normalized path ends with the
        // logical suffix (temp dir prefix differs per run).
        let host_entry = host_snapshot
            .files()
            .find(|f| f.path().ends_with(logical))
            .unwrap_or_else(|| panic!("host missing {logical}"));
        assert_eq!(host_entry.bytes(), *bytes);
        assert_eq!(host_entry.len(), mem_entry.len());
    }

    // Same number of files captured in both backends.
    assert_eq!(host_snapshot.len(), mem_snapshot.len());
    assert_eq!(host_snapshot.len(), fixture.len());
}
