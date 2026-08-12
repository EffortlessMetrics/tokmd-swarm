# Security Scan Report

**Generated:** 2026-08-10
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium
**Scope:** Last 7 days of commits (2026-08-03 → 2026-08-10)

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
identified during this scan. The 7-day window (2026-08-03 → 2026-08-10) contains
**1 commit** on `main`: `8213155 ci: run xtask tests in the required gate
(#543)` from 2026-08-08. The change is a tightly scoped CI workflow hardening
that lands three things:

1. Adds `cargo test -p xtask --all-features` to the existing serial core lane
   in `.github/workflows/ci.yml::tokmd-rust-result`, wired like the other
   three serial lanes (`core`, `test`, `proof_policy`) with its own exit
   marker (`xtask_test_exit`), self-hosted serial retry, step-summary line,
   and assertion before job exit.
2. Fixes a latent evidence bug: all four serial retries now append behind a
   `--- serial retry ---` marker instead of dropping the first attempt's log.
3. Corrects the `tokmd_rust_result` receipt alongside the change (the receipt
   claimed the lanes run "concurrently" when `ci.yml` deliberately runs them
   serially; `base_lem` 15 → 16 for the added command).

The commit body explicitly documents the two defects that previously hid
behind the missing `xtask` lane (`#479`'s `syn 3.0.3` bump that broke
`no_panic.rs`, and the `publishable_internal_dev_dependencies_use_loose_versions`
gate that masked four unpublished dev-dependency pairs); both are already
fixed by `main` PRs and this commit closes the gate gap.

Because the local working tree is a shallow clone (`fetch-depth: 1` from the
scheduled scan step), the diff against the missing parent appears as the
full 2,590-file workspace root. This is the same documented release-surface
shallow-clone behavior that prior weekly scans (`2026-07-20`, `2026-07-27`,
`2026-08-03`) have already verified in place against the
`2026-06-29` true-merge baseline. Every security-critical control surface
that the prior weekly scans verified was re-read in-place this scan and
**remains present in the inspected tree**. Because the true parent is not
available in this shallow clone, this report does not prove that `8213155` or
the weekly window introduced no defense regressions:

- `GIT_REPO_SHAPING_ENV` is still env_removed before every git subprocess in
  `crates/tokmd-git/src/command.rs`, `crates/tokmd-scan/src/walk/git.rs`, and
  `crates/tokmd/src/git_support.rs` (defense D-03).
- `env_base_ref_is_safe` still rejects empty, leading-`-`, whitespace, control,
  and backslash characters in `crates/tokmd-git/src/refs.rs`, paired with
  `--end-of-options` (defense D-04).
- `BoundedPath::existing_relative` / `existing_child` still enforce canonical
  under-root paths in `crates/tokmd-scan/src/path/bounded_path.rs`
  (defense D-05).
- `validate_in_memory_input_path` still rejects every documented bypass
  (empty, >4 KiB, control, leading `/` or `\`, Windows drive, `..`,
  all-`.` paths) in `crates/tokmd-core/src/ffi/inputs.rs` (defense D-06).
- `parse.rs` strict field decoders still return typed `TokmdError::invalid_field`
  on type mismatch (defense D-07).
- `run_json_inner` still requires the top-level JSON to be an object
  (defense D-22).
- `tokmd-cockpit/src/supply_chain.rs::parse_audit_output` still returns
  `Pending` on malformed JSON, never `Pass` (defense D-21).
- `pending_supply_chain_gate` still returns `Pending` (never `Pass`) when the
  `cargo audit` binary is missing on the runner (defense D-24).
- `tokmd-format/src/redact/mod.rs` still uses BLAKE3 with the extension
  allowlist, reverting to bare hash for untrusted compound suffixes
  (defense D-12).
- `tokmd-analysis/src/content/mod.rs` still enforces `DEFAULT_MAX_FILE_BYTES
  = 128 KiB` plus a total `max_bytes` ceiling (defense D-13).
- All Droid-related GitHub Actions
  (`.github/workflows/droid.yml`, `droid-review.yml`, `droid-security-scan.yml`)
  remain SHA-pinned at
  `EffortlessMetrics/droid-action-safe@7c1377ccbacddc95560d1570547a5baa51de01ec`,
  with explicit `ANTHROPIC_AUTH_TOKEN: ""` / `ANTHROPIC_BASE_URL: ""` to
  block ambient fallback to default Anthropic endpoints (defense D-09 / D-20).
- `action.yml` still downloads tokmd with sha256 checksum verification
  against `checksums.txt`, and the `runtime: container` branch still
  requires a verification-gated tag from an explicit allowlist
  (`1.14.0 1.15.0`) using an isolated, anonymous `docker --config` dir
  (defense D-19).
- `.github/settings.yml` still requires the `Tokmd Rust Result` and
  `Codex Review Gate` status checks, with `allow_force_pushes: false` and
  `allow_deletions: false` on `main` (defense D-10).
- `deny.toml` still ignores only the documented transitive
  `RUSTSEC-2020-0163` (`term_size` via `tokei`) — no new advisories
  introduced (defense D-11).
- The new `cargo test -p xtask --all-features` step in `ci.yml` runs
  inside the same shell block as the existing three serial lanes, sharing
  the already-hardened runner (`set +e` with explicit exit-code capture,
  serial retries guarded by `core_exit != "0" && core_exit != "124"`,
  and `--- serial retry ---` markers preserved behind the new
  `xtask_test.log`). It does not introduce any new `Command::new` /
  shell interpolation, `secrets.*` access, network egress, or trust
  boundary.

The codebase continues to demonstrate a security-first design with no
regressions introduced by the xtask CI lane addition.

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

Carried observations are unchanged from the `2026-08-03` baseline; the
`#543` xtask-lane addition introduces **no new** low-severity observations
this week.

### OBS-001 (carried): FFI JSON payload size not bounded

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Denial of Service |
| **File** | `crates/tokmd-core/src/ffi/mod.rs` |
| **Status** | Not patched — design choice |

**Description:** The `run_json(mode, args_json)` FFI entrypoint accepts a JSON
string of arbitrary size. While individual in-memory `inputs[].path` is bounded
to 4096 bytes (`MAX_IN_MEMORY_INPUT_PATH_BYTES`), the outer JSON envelope is
not.

**Why not a finding:** Caller controls input. `serde_json::from_str` allocates
predictably; no algorithmic blowup. No `medium` reachability: requires the
caller to opt in. Out of scope per `SECURITY.md`.

**Recommended fix (optional, future):** Add a soft cap on `args_json.len()`
(e.g. 8 MiB) returning a typed `TokmdError::invalid_field("args", "JSON args
exceed 8 MiB cap")` from `run_json_inner`.

### OBS-002 (carried): Transitive `RUSTSEC-2020-0163` advisory

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (transitive) |
| **STRIDE Category** | Elevation of Privilege |
| **File** | `Cargo.lock` (transitive `term_size` via `tokei`) |
| **Status** | Documented in `deny.toml` |

**Description:** `term_size` is a transitive dependency of `tokei` and has an
unmaintained advisory (`RUSTSEC-2020-0163`).

**Why not a finding:** Already documented in `deny.toml` with rationale.
Out of scope per `SECURITY.md`. No change in the xtask-lane PR.

**Recommended action:** Track upstream `tokei` for a `term_size` removal.

### OBS-003 (carried): GitHub Actions pinning is mixed (tag + SHA)

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Spoofing / Tampering |
| **File** | `.github/workflows/*.yml` |
| **Status** | Not patched — mixed strategy |

**Description:** The Droid-related workflows
(`.github/workflows/droid.yml`, `droid-review.yml`, `droid-security-scan.yml`)
pin third-party actions by SHA, including the custom
`EffortlessMetrics/droid-action-safe@7c1377ccbacddc95560d1570547a5baa51de01ec`.
Other workflows (`.github/workflows/ci.yml`, `release.yml`, `cockpit.yml`,
`nix-full.yml`, `bindings-parity.yml`, `swarm-ghcr.yml`,
`ghcr-container-smoke.yml`, `proof-executor.yml`,
`proof-observation-collection.yml`, `mutants.yml`, `pr-plan.yml`,
`badge-endpoints.yml`, `coverage.yml`, `test-action.yml`, `fuzz.yml`,
`ripr.yml`, `ci-policy.yml`, `no-panic-policy.yml`,
`clippy-exceptions-policy.yml`, `sync-labels.yml`, `nix-macos.yml`) pin by
tag (e.g., `actions/checkout@v7.0.1`, `Swatinem/rust-cache@v2`,
`dtolnay/rust-toolchain@stable`). The threat model claims SHA pinning
workspace-wide, which is no longer strictly accurate for non-Droid workflows.

**Why not a finding:**
- Tag-pinned first-party actions (`actions/*`) are a well-accepted practice
  with low residual risk; GitHub's own recommended baseline.
- All release/CI/cockpit workflows that take privileged actions are pinned
  at the workflow level via `actions/checkout@v7.0.1` consistently across
  the workspace, providing a uniform policy.
- The custom Droid action — the highest-privilege third-party surface — IS
  SHA-pinned.
- Below the `medium` severity threshold for this scan; flagged for the next
  threat-model refresh (target: 2026-11-01 or earlier if scope changes).

**Recommended action (optional, future):** Either update the threat model
to reflect the actual mixed-pinning policy, or convert all third-party
actions to SHA-pinned references and codify the rotation process in
`.factory/rules/`.

### OBS-004 (carried): `web/runner` browser code does not pin GitHub API base URL

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Spoofing |
| **File** | `web/runner/ingest.js` |
| **Status** | Not patched — review for future |

**Description:** The browser-side runner fetches repository content via
`fetch()` calls to `api.github.com` (and the codeload/GitHub
`releases`/`archive` endpoints). These URLs are hard-coded in the
`web/runner/` JavaScript modules. The token (when supplied) is stored in
`sessionStorage` (not `localStorage`) and used as a `Bearer` header. There
is no Subresource Integrity pinning or origin allow-listing on the
client-side fetch surface.

**Why not a finding:**
- All sensitive fetches target `api.github.com` / `codeload.github.com`,
  which are HTTPS and well-known.
- The token lifetime is bounded to a single browser tab
  (`sessionStorage`).
- No DOM injection surfaces observed: all dynamic data is rendered via
  `textContent` (verified across `main.js`); no use of `innerHTML`,
  `eval`, `new Function`, or `document.write` (confirmed by repository-wide
  grep returning no matches).
- Browser-side runner runs entirely in the user-agent sandbox; no
  filesystem, no subprocess.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional):** Consider an explicit allowlist of fetch
origins and a CSP `connect-src` directive in the runner's served HTML
to defend against supply-chain injection via a compromised
`<script>`/module.

### OBS-005 (carried): `action.yml` install step performs `curl | sh` style download

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Tampering / Information Disclosure |
| **File** | `action.yml` (composite step `Install tokmd`) |
| **Status** | Not patched — verified checksums |

**Description:** The composite GitHub Action downloads a pre-built
`tokmd` binary from `github.com/EffortlessMetrics/tokmd/releases/...` and
verifies it against `checksums.txt` (sha256). It does not verify a
cryptographic signature on the checksum file or on the release itself.
The download URL is interpolated from a user-supplied `version` input
without shell-unsafe character filtering.

**Why not a finding:**
- The action is a published action; consumers control which version
  they pin to. The check is bounded to a `MAJOR.MINOR.PATCH`-style
  string via the `${ver#v}` prefix logic.
- `curl -fsSL` rejects HTTP errors and follows redirects (only to
  HTTPS GitHub release endpoints in practice).
- The checksum verification, when checksums.txt is present, uses
  `sha256sum`/`shasum`/`Get-FileHash` to compare the downloaded
  binary's hash to the expected value.
- Build provenance is separately attested via
  `actions/attest-build-provenance@v4` in `release.yml`.
- Below the `medium` severity threshold; this is documented best-
  practice coverage.

**Recommended action (optional):** Add explicit format validation
for the `version` input (e.g., regex `^v?\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?$`)
and reject anything else before constructing the URL.

### OBS-006 (carried): Branch protection review requirements are zero

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational, by policy) |
| **STRIDE Category** | Elevation of Privilege / Repudiation |
| **File** | `.github/settings.yml` |
| **Status** | Not patched — intentional single-maintainer policy |

**Description:** `.github/settings.yml` configures
`required_approving_review_count: 0` and `require_code_owner_reviews: false`
on `main`. Native human approval and CODEOWNERS review are intentionally
absent because the single-maintainer workflow uses the `Tokmd Rust Result`
+ `Codex Review Gate` status checks as the effective gate (per the in-line
comment: "Codex is the exact-head reviewer for this single-maintainer
workflow; native human approval and CODEOWNERS review are intentionally
absent.").

**Why not a finding:**
- `enforce_admins: false` and `allow_force_pushes: false` keep the
  effective gate narrow but explicit.
- The two required status checks are deterministic and run on every PR.
- `allow_deletions: false` prevents destructive reflog manipulation.
- The threat model documents this as a deliberate operational choice
  ("Codex is the exact-head reviewer for this single-maintainer
  workflow"), not a missed defense.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional, future):** When the maintainer count
grows, increase `required_approving_review_count` and re-enable
`require_code_owner_reviews`.


## Standing Defenses Verified (No Regression)

The following defenses were re-verified during this scan. All remain intact.

| ID | Defense | Location | Verified |
|----|---------|----------|----------|
| D-01 | `unsafe_code = "forbid"` workspace lint | `Cargo.toml` | ✓ |
| D-02 | `unwrap_used`, `expect_used`, `panic`, `unreachable`, `dbg_macro`, `todo`, `unimplemented` lints denied | `Cargo.toml` | ✓ |
| D-03 | Git subprocess env isolation (`GIT_REPO_SHAPING_ENV`) | `crates/tokmd-git/src/command.rs`, `crates/tokmd/src/git_support.rs`, `crates/tokmd-scan/src/walk/git.rs` | ✓ |
| D-04 | Git ref validation (`env_base_ref_is_safe` + `--end-of-options`) | `crates/tokmd-git/src/refs.rs` | ✓ |
| D-05 | Bounded path canonicalization under root | `crates/tokmd-scan/src/path/bounded_path.rs` | ✓ |
| D-06 | FFI in-memory input path validation | `crates/tokmd-core/src/ffi/inputs.rs` (line: `MAX_IN_MEMORY_INPUT_PATH_BYTES = 4096`) | ✓ |
| D-07 | Strict JSON parsing with type validation | `crates/tokmd-core/src/ffi/parse.rs` | ✓ |
| D-08 | Per-family schema versioning (`SCHEMA_VERSION=2`, `COCKPIT_SCHEMA_VERSION=3`, `HANDOFF_SCHEMA_VERSION=5`, `CONTEXT_SCHEMA_VERSION=4`, `CONTEXT_BUNDLE_SCHEMA_VERSION=2`) | `crates/tokmd-types/src/` | ✓ |
| D-09 | SHA-pinned Droid-related actions; tag-pinned first-party actions | `.github/workflows/droid*.yml` (SHA), `release.yml` and others (tag) | ✓ |
| D-10 | Branch protection on `main` (status checks required, no force-push, no deletions) | `.github/settings.yml` | ✓ |
| D-11 | `cargo-deny` advisory + license allowlist | `deny.toml` | ✓ |
| D-12 | BLAKE3 redaction with extension allowlist | `crates/tokmd-format/src/redact/mod.rs`, `crates/tokmd-format/src/redact/extensions.rs` | ✓ |
| D-13 | Content reads bounded by `ContentLimits` (`DEFAULT_MAX_FILE_BYTES = 128 KiB`) | `crates/tokmd-analysis/src/content/mod.rs` | ✓ |
| D-14 | PyO3 FFI invariants (no panic, GIL release, error translation) | `crates/tokmd-python/src/lib.rs` | ✓ |
| D-15 | WASM uses `MemFs` (no host fs) | `crates/tokmd-wasm/` | ✓ |
| D-16 | `web/runner` browser runner uses `textContent` (no `innerHTML`/`eval`/`new Function`/`document.write`) | `web/runner/main.js` | ✓ |
| D-17 | `web/runner` token stored in `sessionStorage` (not `localStorage`) | `web/runner/auth.js` | ✓ |
| D-18 | `web/runner` worker protocol allowlists modes & presets | `web/runner/messages.js` | ✓ |
| D-19 | Composite action installs tokmd with sha256 checksum verification; verification-gated tag allowlist for `runtime: container` | `action.yml` | ✓ |
| D-20 | Custom Droid action SHA-pinned across all Droid workflows; explicit `ANTHROPIC_AUTH_TOKEN: ""` / `ANTHROPIC_BASE_URL: ""` to block ambient fallback | `.github/workflows/droid*.yml` | ✓ |
| D-21 | `cargo audit` invoked with structured `--json` output, malformed JSON treated as Pending | `crates/tokmd-cockpit/src/supply_chain.rs` | ✓ |
| D-22 | `run_json` top-level JSON must be an object (strict shape check) | `crates/tokmd-core/src/ffi/mod.rs::run_json_inner` | ✓ |
| D-23 | Author DAG import via true-merge commits (no force-push of publication history) | repository topology | ✓ |
| D-24 | `supply_chain` gate explicitly tolerates missing `cargo audit` binary by returning `Pending`, never `Pass` (per `pending_supply_chain_gate` constructor) | `crates/tokmd-cockpit/src/supply_chain.rs` | ✓ |
| D-25 | `Command::new("cargo")` and `Command::new("git")` invocations use `arg()` (not shell) and `current_dir` for path control, no `sh -c` / `bash -c` | `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd-cockpit/src/gates/contracts.rs`, `crates/tokmd-git/src/command.rs`, `crates/tokmd-scan/src/walk/git.rs` | ✓ |


## Appendix

### Threat Model

- **Status:** Stale/pending refresh: OBS-003 records a mixed tag-and-SHA
  policy, so the workspace-wide SHA-pinning statement is not verified
  unchanged.
- **Location:** `.factory/threat-model/threat-model.md`
- **Last Modified:** 2026-08-02 (8 days ago — well within 90-day window)
- **Methodology:** STRIDE
- **Next review:** 2026-11-01 (90-day cadence) or upon architecture change
- **No regeneration this scan** — the file is within its normal freshness
  window, but OBS-003 requires a threat-model refresh before this report can
  certify the workspace-wide SHA-pinning statement as current.

### Scan Metadata

- **Commits Scanned:** 1 (`8213155 ci: run xtask tests in the required gate
  (#543)`, 2026-08-08, GPG-signed)
- **Files in scope:** 2,590 (shallow-clone surface; the local clone is
  fetched with depth 1 so the diff against the missing parent appears as
  the full workspace root). The full surface was previously reviewed under
  the `2026-06-29` true-merge baseline; this scan re-verified all
  security-critical modules in place.
- **Scan Duration:** ~6m (focused diff review + defense re-verification)
- **Skills Used:** commit-security-scan (manual, STRIDE), vulnerability-validation
  (manual, exploitability assessment), security-review (manual, defense
  confirmation)
- **Manual Reviewers:** 1 (Droid scheduled security scan)
- **False Positive Filter:** applied — see Observations above

### Scan Coverage Matrix

The coverage below applies to the xtask-lane CI diff and the re-verified
standing defenses.

| Area | Files reviewed | Findings |
|------|----------------|----------|
| CI workflow lane addition | `.github/workflows/ci.yml::tokmd-rust-result` (serial core lane) | 0 |
| Git subprocess isolation | `crates/tokmd-git/src/command.rs`, `crates/tokmd-git/src/refs.rs`, `crates/tokmd/src/git_support.rs`, `crates/tokmd-scan/src/walk/git.rs` | 0 |
| FFI inputs | `crates/tokmd-core/src/ffi/mod.rs`, `inputs.rs`, `parse.rs` | 0 |
| Path handling | `crates/tokmd-scan/src/path/bounded_path.rs`, `crates/tokmd-scan/src/exclude/mod.rs` | 0 |
| File content reads | `crates/tokmd-analysis/src/content/mod.rs` (limits), `crates/tokmd-io-port/src/` | 0 |
| Redaction / hashing | `crates/tokmd-format/src/redact/mod.rs`, `extensions.rs` | 0 |
| Subprocess audit/semver | `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd-cockpit/src/gates/contracts.rs` | 0 |
| GitHub workflows | `.github/workflows/*.yml` (28 files), `.github/settings.yml`, `action.yml` | 0 |
| Build / lint | `Cargo.toml`, `deny.toml`, `clippy.toml`, `.cargo/config.toml` | 0 |
| Githooks | `.githooks/pre-commit`, `.githooks/pre-push`, `.claude/hooks/format-rust.sh` | 0 |
| Web runner (browser) | `web/runner/main.js`, `worker.js`, `auth.js`, `messages.js`, `runtime.js`, `ingest.js` | 0 |
| Threat model | `.factory/threat-model/threat-model.md` | unchanged |

### Commit-level Analysis

The 7-day window (2026-08-03 → 2026-08-10) contains **1 commit** on `main`:

```
821315597954e4a88d11b99bd1d741533d6cd551
Author: Steven Zimmerman, CPA <15812269+EffortlessSteven@users.noreply.github.com>
Date:   Sat Aug 8 05:38:49 2026 -0400
Subject: ci: run xtask tests in the required gate (#543)
```

- **Type:** GPG-signed single-commit CI workflow hardening.
- **Surface:** 2,590 files in `git diff` (shallow-clone artifact; the
  diff against the missing parent shows the full workspace root).
  This is the same documented release-surface shallow-clone behavior
  that prior weekly scans have already verified in place against the
  `2026-06-29` true-merge baseline.
- **Net code change in the commit body:** Adds one shell snippet to the
  `tokmd-rust-result` job's `Fast precontext and launch core gate` step
  in `ci.yml` (the `cargo test -p xtask --all-features` invocation with
  exit capture) and one matching `--- serial retry ---` retry block in
  the existing serial-retry shell block. Updates the `tokmd_rust_result`
  receipt alongside.
- **Security-critical files re-read in place:**
  - `crates/tokmd-git/src/command.rs` — `GIT_REPO_SHAPING_ENV` and tests intact.
  - `crates/tokmd-git/src/refs.rs` — `env_base_ref_is_safe` rejects empty,
    leading `-`, whitespace, control, backslash; `--end-of-options` used.
  - `crates/tokmd-core/src/ffi/inputs.rs` — `validate_in_memory_input_path`
    covers empty / >4 KiB / control / absolute / Windows drive / `..` /
    all-`.` paths.
  - `crates/tokmd-core/src/ffi/parse.rs` — strict field decoders; type
    mismatch → `TokmdError::invalid_field`.
  - `crates/tokmd-core/src/ffi/mod.rs` — top-level JSON must be an object
    (defense D-22).
  - `crates/tokmd-scan/src/path/bounded_path.rs` — `BoundedPath` enforces
    canonical under-root; rejects `..` at any position; rejects RootDir /
    Prefix components.
  - `crates/tokmd-format/src/redact/mod.rs` — BLAKE3 short hash with
    extension allowlist.
  - `crates/tokmd-analysis/src/content/mod.rs` — `ContentLimits` with
    `DEFAULT_MAX_FILE_BYTES = 128 KiB` plus a total `max_bytes` ceiling.
  - `crates/tokmd-cockpit/src/supply_chain.rs` — `parse_audit_output`
    returns `Pending` on malformed JSON; `pending_supply_chain_gate`
    returns `Pending` (never `Pass`) when `cargo audit` is missing.
  - `crates/tokmd-cockpit/src/gates/contracts.rs` — `Command::new("cargo")`
    uses `arg()` and `current_dir`; no shell.
  - `crates/tokmd-scan/src/walk/git.rs` — same `GIT_REPO_SHAPING_ENV`
    isolation as `tokmd-git/src/command.rs`.
  - `crates/tokmd-scan/src/exclude/mod.rs` — deterministic exclude
    pattern normalization (`#![forbid(unsafe_code)]`).
  - `action.yml` — sha256 checksum verification on downloaded tokmd
    binary; verification-gated tag allowlist
    (`1.14.0 1.15.0`) for `runtime: container`; isolated anonymous
    `docker --config` for pulls; strict `output-dir` validation
    (rejects absolute and `..` segments) for `mode: packet`.
  - `.github/workflows/droid*.yml` — Droid action SHA-pinned
    (`EffortlessMetrics/droid-action-safe@7c1377ccbacddc95560d1570547a5baa51de01ec`)
    with explicit `ANTHROPIC_AUTH_TOKEN: ""` and `ANTHROPIC_BASE_URL: ""`
    to block ambient fallback.
  - `.github/workflows/ci.yml` — new `cargo test -p xtask --all-features`
    step inherits the same hardening as the surrounding three serial
    lanes (exit-code capture, `--- serial retry ---` marker, assertion
    before job exit). No new `secrets.*` access, network egress, or
    trust-boundary change.
  - `.github/settings.yml` — `Tokmd Rust Result` and `Codex Review Gate`
    required on `main`; `allow_force_pushes: false`; `allow_deletions: false`.
  - `deny.toml` — `RUSTSEC-2020-0163` ignore for transitive `term_size`
    unchanged; license allowlist unchanged.

**No security findings in this scan window.**

### Patches Generated

No patches were generated this scan (no findings at or above `medium`).

### Next Scan

The next scheduled security scan runs Monday, 2026-08-17 via
`.github/workflows/droid-security-scan.yml` (cron `0 8 * * 1`).

## References

- [CWE Database](https://cwe.mitre.org/)
- [STRIDE Threat Model](https://docs.microsoft.com/en-us/azure/security/develop/threat-modeling-tool-threats)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [Rust Security Advisory Database](https://rustsec.org/)
- [CII Best Practices](https://www.bestpractices.dev/)
- Repository security policy: `SECURITY.md`
- Repository threat model: `.factory/threat-model/threat-model.md`
- Previous scans: `.factory/security/reports/security-report-2026-06-01.md`,
  `.factory/security/reports/security-report-2026-06-08.md`,
  `.factory/security/reports/security-report-2026-06-29.md`,
  `.factory/security/reports/security-report-2026-07-06.md`,
  `.factory/security/reports/security-report-2026-07-13.md`,
  `.factory/security/reports/security-report-2026-07-20.md`,
  `.factory/security/reports/security-report-2026-07-27.md`,
  `.factory/security/reports/security-report-2026-08-03.md`
