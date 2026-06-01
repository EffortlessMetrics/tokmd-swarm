# Security Scan Report

**Generated:** 2026-06-01
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium
**Scope:** Last 7 days of commits (1 commit: initial import, plus standing defenses audit)

## Executive Summary

| Severity | Count | Auto-fixed | Manual Required |
|----------|-------|------------|-----------------|
| CRITICAL | 0     | 0          | 0               |
| HIGH     | 0     | 0          | 0               |
| MEDIUM   | 0     | 0          | 0               |
| LOW      | 0     | 0          | 0               |

**Total Findings:** 0
**Auto-fixed:** 0
**Manual Review Required:** 0

**Summary:** No vulnerabilities at or above the `medium` severity threshold were
identified during this scan. The codebase demonstrates a security-first design
with multiple defense-in-depth measures already in place (see
`.factory/threat-model/threat-model.md` for the full STRIDE analysis). One
transitive `RUSTSEC-2020-0163` advisory (transitive `term_size` via `tokei`)
is already documented in `deny.toml` and is out of scope per `SECURITY.md`.

## Critical Findings

*None.*

## High Findings

*None.*

## Medium Findings

*None.*

## Low Findings

*None.*

## Observations (Below Threshold — Not Reported As Findings)

These items were considered during the scan but do not meet the `medium` severity
threshold. They are recorded here for traceability and the next scheduled scan.

### OBS-001: FFI JSON payload size not bounded

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Denial of Service |
| **File** | `crates/tokmd-core/src/ffi/mod.rs` |
| **Status** | Not patched — design choice |

**Description:**
The `run_json(mode, args_json)` FFI entrypoint accepts a JSON string of
arbitrary size. While individual in-memory `inputs[].path` is bounded to
4096 bytes (`MAX_IN_MEMORY_INPUT_PATH_BYTES`), the outer JSON envelope is
not. A caller (Python / Node binding) could pass a multi-megabyte JSON
string.

**Why not a finding:**
- Caller controls input. The library runs in the caller's process.
- `serde_json::from_str` allocates predictably; no algorithmic blowup.
- No `medium` reachability: requires the caller to opt in.
- Out of scope per `SECURITY.md` ("Issues in third-party dependencies"
  and "Theoretical attacks without a realistic exploitation scenario").

**Recommended fix (optional, future):**
Add a soft cap on `args_json.len()` (e.g. 8 MiB) returning a typed
`TokmdError::invalid_field("args", "JSON args exceed 8 MiB cap")` from
`run_json_inner`. Document the limit in the Python and Node API docs.

### OBS-002: Transitive `RUSTSEC-2020-0163` advisory

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (transitive) |
| **STRIDE Category** | Elevation of Privilege |
| **File** | `Cargo.lock` (transitive `term_size` via `tokei`) |
| **Status** | Documented in `deny.toml` |

**Description:**
`term_size` is a transitive dependency of `tokei` and has an unmaintained
advisory (`RUSTSEC-2020-0163`).

**Why not a finding:**
- Already documented in `deny.toml` with rationale: "transitive via tokei;
  revisit when upstream removes it".
- Out of scope per `SECURITY.md` ("Bugs in third-party dependencies — report
  these upstream").

**Recommended action:**
Track upstream `tokei` for a `term_size` removal. No action required from
this repo.

### OBS-003: GitHub Actions SHA-pinning is not automated

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Spoofing / Tampering |
| **File** | `.github/workflows/*.yml` |
| **Status** | Not patched — manual process |

**Description:**
All GitHub Actions in this repo are pinned by SHA with a comment naming the
upstream version (e.g. `actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2`).
There is no automated tooling (e.g., Dependabot for GitHub Actions) verifying
the SHA → tag mapping is consistent and unrotated.

**Why not a finding:**
- SHA-pinning is the current best practice.
- Dependabot is a quality concern, not a vulnerability.

**Recommended action (optional):**
Consider enabling Dependabot for GitHub Actions (`.github/dependabot.yml`
already exists for cargo) to keep SHAs current.

## Standing Defenses Verified (No Regression)

The following defenses were re-verified during this scan. All remain intact.

| ID | Defense | Location | Verified |
|----|---------|----------|----------|
| D-01 | `unsafe_code = "forbid"` workspace lint | `Cargo.toml` | ✓ |
| D-02 | `unwrap_used`, `expect_used`, `panic`, `unreachable` lints denied | `Cargo.toml` | ✓ |
| D-03 | Git subprocess env isolation (`GIT_REPO_SHAPING_ENV`) | `crates/tokmd-git/src/command.rs`, `crates/tokmd/src/git_support.rs` | ✓ |
| D-04 | Git ref validation (`env_base_ref_is_safe` + `--end-of-options`) | `crates/tokmd-git/src/refs.rs` | ✓ |
| D-05 | Bounded path canonicalization under root | `crates/tokmd-scan/src/path/bounded_path.rs` | ✓ |
| D-06 | FFI in-memory input path validation | `crates/tokmd-core/src/ffi/inputs.rs` | ✓ |
| D-07 | Strict JSON parsing with type validation | `crates/tokmd-core/src/ffi/parse.rs` | ✓ |
| D-08 | Per-family schema versioning | `crates/tokmd-types/src/` | ✓ |
| D-09 | SHA-pinned GitHub Actions | `.github/workflows/*.yml` | ✓ |
| D-10 | Branch protection on `main` (CODEOWNERS, 1 approval, CI required) | `.github/settings.yml` | ✓ |
| D-11 | `cargo-deny` advisory + license allowlist | `deny.toml` | ✓ |
| D-12 | BLAKE3 redaction with extension allowlist | `crates/tokmd-format/src/redact/mod.rs`, `crates/tokmd-format/src/redact/extensions.rs` | ✓ |
| D-13 | Content reads bounded by `ContentLimits` | `crates/tokmd-analysis/src/content/mod.rs` | ✓ |
| D-14 | PyO3 FFI invariants (no panic, GIL release, error translation) | `crates/tokmd-python/src/lib.rs` | ✓ |
| D-15 | WASM uses `MemFs` (no host fs) | `crates/tokmd-wasm/` | ✓ |

## Appendix

### Threat Model

- **Status:** Newly generated
- **Location:** `.factory/threat-model/threat-model.md`
- **Methodology:** STRIDE
- **Next review:** 2026-09-01 (90-day cadence) or upon architecture change

### Scan Metadata

- **Commits Scanned:** 1 (`00dbbbf549d4b13e0c44bc750cd80e9527bb2306` — "ci(plan): show estimate source in summary")
- **Files in scope:** 2428 (entire repository — single-commit import)
- **Scan Duration:** ~3m
- **Skills Used:** commit-security-scan (manual), vulnerability-validation (manual), security-review (manual), threat-model-generation
- **Manual Reviewers:** 1 (Droid scheduled security scan)
- **False Positive Filter:** applied — see Observations above

### Scan Coverage Matrix

| Area | Files reviewed | Findings |
|------|----------------|----------|
| CLI argv parsing | `crates/tokmd/src/cli/`, `crates/tokmd/src/commands/*.rs` | 0 |
| Subprocess invocation | `crates/tokmd-git/`, `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd/src/git_support.rs` | 0 |
| Path handling | `crates/tokmd-scan/src/path/`, `crates/tokmd-scan/src/roots.rs`, `crates/tokmd-scan/src/walk/` | 0 |
| FFI inputs | `crates/tokmd-core/src/ffi/`, `crates/tokmd-python/src/`, `crates/tokmd-node/src/` | 0 |
| File content reads | `crates/tokmd-analysis/src/content/`, `crates/tokmd-io-port/src/` | 0 |
| Redaction / hashing | `crates/tokmd-format/src/redact/` | 0 |
| GitHub workflows | `.github/workflows/*.yml`, `.github/settings.yml`, `action.yml` | 0 |
| Build / lint | `Cargo.toml`, `deny.toml`, `clippy.toml`, `.cargo/` | 0 |
| Githooks | `.githooks/pre-commit`, `.githooks/pre-push`, `.claude/hooks/format-rust.sh` | 0 |

### Commit-level Analysis

Only one commit falls within the 7-day window:

```
00dbbbf549d4b13e0c44bc750cd80e9527bb2306
Author: Steven Zimmerman, CPA <15812269+EffortlessSteven@users.noreply.github.com>
Date:   Mon Jun 1 05:35:46 2026 -0400
Subject: ci(plan): show estimate source in summary
```

This commit is the initial repository import (`git log --all --oneline` returns
exactly 1 commit). It contains 2428 files (`.cargo/config.toml`, the workspace
source tree, all GitHub workflows, agent manifests, etc.). The CI workflow change
referenced in the subject ("ci(plan): show estimate source in summary") modifies
`.github/workflows/pr-plan.yml` to surface the `estimate_source` field.

**Review of the CI change:**
- Touches only `.github/workflows/pr-plan.yml` (CI step summary content).
- No new secrets, no new permissions, no new third-party action.
- No shell-out to untrusted input.
- No change to environment variable handling.

**No security findings in this commit.**

### Patches Generated

No patches were generated this scan (no findings at or above `medium`).

### Next Scan

The next scheduled security scan runs Monday, 2026-06-08 via
`.github/workflows/droid-security-scan.yml` (cron `0 8 * * 1`).

## References

- [CWE Database](https://cwe.mitre.org/)
- [STRIDE Threat Model](https://docs.microsoft.com/en-us/azure/security/develop/threat-modeling-tool-threats)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [Rust Security Advisory Database](https://rustsec.org/)
- [CII Best Practices](https://www.bestpractices.dev/)
- Repository security policy: `SECURITY.md`
- Repository threat model: `.factory/threat-model/threat-model.md`
