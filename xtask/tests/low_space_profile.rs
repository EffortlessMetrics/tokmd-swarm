//! The gate launcher and its child builds share the low-space workspace profile.

use std::{fs, process::Command};

#[test]
fn workspace_profile_applies_to_builds_and_tests_and_can_be_overridden() {
    let manifest = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml"))
        .expect("read workspace manifest");
    let manifest: toml::Value = toml::from_str(&manifest).expect("parse workspace manifest");
    let profiles = manifest.get("profile").expect("workspace profiles");
    let dev = &profiles["dev"];
    assert_eq!(dev["debug"].as_integer(), Some(0));
    assert_eq!(dev["incremental"].as_bool(), Some(false));
    assert!(
        profiles.get("test").is_none(),
        "test must inherit dev defaults"
    );

    let fixture = tempfile::tempdir().expect("create fixture");
    fs::create_dir(fixture.path().join("src")).expect("create source directory");
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub fn answer() -> u8 { 42 }\n",
    )
    .expect("write source");
    fs::write(
        fixture.path().join("Cargo.toml"),
        format!(
            "[package]\nname = \"profile-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[profile.dev]\n{}\n",
            toml::to_string(dev).expect("serialize actual dev profile")
        ),
    )
    .expect("write fixture manifest");

    for (subcommand, override_debug) in [("build", false), ("test", false), ("build", true)] {
        let mut command = Command::new(env!("CARGO"));
        command
            .current_dir(fixture.path())
            .args([subcommand, "-vv"]);
        if subcommand == "test" {
            command.arg("--no-run");
        }
        command
            .env("CARGO_HOME", fixture.path().join("cargo-home"))
            .env_remove("RUSTFLAGS")
            // An explicit empty encoded value takes precedence over target
            // rustflags from the host environment or ancestor Cargo config.
            .env("CARGO_ENCODED_RUSTFLAGS", "")
            .env_remove("CARGO_BUILD_RUSTFLAGS")
            .env_remove("CARGO_PROFILE_DEV_DEBUG")
            .env_remove("CARGO_PROFILE_TEST_DEBUG")
            .env_remove("CARGO_PROFILE_DEV_INCREMENTAL")
            .env_remove("CARGO_PROFILE_TEST_INCREMENTAL")
            .env_remove("CARGO_INCREMENTAL")
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("CARGO_TARGET_DIR");
        if override_debug {
            command.env("CARGO_PROFILE_DEV_DEBUG", "2");
        }
        let output = command.output().expect("run cargo fixture");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "cargo {subcommand} failed: {stderr}"
        );
        let rustc = stderr
            .lines()
            .find(|line| line.contains("--crate-name profile_fixture"))
            .expect("fixture rustc invocation");
        if override_debug {
            assert!(rustc.contains("debuginfo=2"), "{rustc}");
        } else {
            assert!(!rustc.contains("debuginfo="), "{rustc}");
        }
        assert!(!rustc.contains("incremental="), "{rustc}");
    }
}
