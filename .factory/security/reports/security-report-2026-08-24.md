# Security Scan Report

**Generated:** 2026-08-24
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium
**Intended Window:** 2026-08-17 → 2026-08-24
**Observed Scope:** Checked-out commit `c8c3aa1`; `git log --since="7 days ago"`
reported exactly one commit in this checkout for the intended window. The
scan does not independently prove window completeness: the appendix records
only the local checkout state, so the zero-finding tally is limited to the
observed checkout and must not be read as a full-history claim.

## Executive Summary

| Severity | Count | Auto-fixed | Manual Required |
|----------|-------|------------|-----------------|
| CRITICAL | 0     | 0          | 0               |
| HIGH     | 0     | 0          | 0               |
| MEDIUM   | 0     | 0          | 0               |
| LOW      | 0     | 0          | 0               |

**Total Findings:** 0
**Evidence limitation:** No automated vulnerability scanners or heavyweight
build/test witnesses ran. This is an advisory zero-finding tally from manual
review, not a security pass or evidence that the repository has no
vulnerabilities.
**Auto-fixed:** 0
**Manual Review Required:** 0

**Summary:** The manual review reported no findings at or above the `medium`
severity threshold for the only commit observed in this checkout for the
intended window,
`c8c3aa1 test(handoff): cover intelligence warning provenance (#622)`,
dated 2026-08-22. The commit is itself a STRIDE-positive test-and-docs
addition. It adds a focused `Risk` / `--no-git` handoff integration test that
asserts the existing `intelligence.json` warning provenance is preserved when
git history is intentionally skipped, and it documents that unavailable
git-history enrichments are recorded in `intelligence.json.warnings`. The
production warning-emitting path
(`crates/tokmd/src/commands/handoff/intelligence.rs`) is unchanged; the test
verifies its deterministic contract and `docs/artifacts.md` line 119 now
explicitly names the `warnings` array as the warning label channel.

This is a **STRIDE-positive** change across the relevant categories:

| STRIDE | Direction | Why |
|--------|-----------|-----|
| Spoofing | N/A | Out of scope for a test/docs-only change. |
| Tampering | Improved | The new test (`handoff_risk_no_git_records_hotspot_warning`) hardens a contract on the existing `intelligence.json` warning path, requiring (i) `hotspots: null` and (ii) a stable warning prefix `"hotspots unavailable: git history skipped"`. Future drift that drops the warning or changes its prefix is now caught at test time. |
| Repudiation | Improved | The test plus the docs update create an auditable artifact for warning provenance: it is now both enforced in CI and documented for downstream consumers (`work-order.md`, `evidence-packet`, review pipelines). |
| Information Disclosure | Improved | The change makes the "why was this enrichment unavailable" channel explicit and bounded. Before the docs update, the `warnings` array existed but was not described as the canonical warning-provenance channel; now the contract is named and tested. |
| Denial of Service | N/A | Out of scope for a test/docs-only change. |
| Elevation of Privilege | N/A | Out of scope for a test/docs-only change. |

The workspace-wide standing defenses were re-read in place and remain
present at the checked-out commit (see Standing Defenses table below). No
defenses were observed to have regressed.

The threat model at `.factory/threat-model/threat-model.md` is dated
2026-08-02 — 22 days old — still well within the 90-day regeneration
window. No regeneration this scan.

## Critical Findings

*None.*

## High Findings

*None.*

## Medium Findings

*None.*

## Low Findings

*None.*

## Observations (Below Threshold — Not Reported As Findings)

These items were considered during the scan but do not meet the `medium`
severity threshold. They are recorded here for traceability and the next
scheduled scan.

The manual review did not introduce any new observations this scan window
beyond what is already documented in the standing defenses and the prior
`2026-08-17` report. The two previously observed MEDIUM-but-not-finding
items remain unchanged:

- **OBS-003 (carried):** Mixed GitHub Action pinning posture in
  `.github/workflows/*.yml`. The first-party `EffortlessMetrics/droid-action-safe`,
  `EffortlessMetrics/ub-review`, `EffortlessMetrics/release-packager`,
  `taiki-e/install-action`, `Swatinem/rust-cache`, `github/codeql-action`,
  `docker/setup-buildx-action`, `docker/login-action`, `docker/build-push-action`,
  and the `actions/*` set are SHA-pinned at the checked-out commit. The
  `2026-08-17` typos lane hardening closed the most material gap (a mutable
  `@v1` floating tag) for that lane; no new pin drift was observed in the
  `c8c3aa1` window.
- **OBS-006 (carried):** `RUSTSEC-2020-0163` (transitive `term_size`) is
  recorded as ignored in `deny.toml`. This is a transitive advisory on the
  `home` crate vendored at `vendor/home-0.5.12` (intentional temporary
  patch via `[patch.crates-io]`). Not in the change scope this window.

The `c8c3aa1` change adds no new observation. The test verifies existing
behavior; the docs update names an existing channel.

## Commit-level Analysis

The only commit reviewed for the intended 2026-08-17 → 2026-08-24 window was:

```
c8c3aa1987aeac40d5397936ec84519a82f8993a
Author: Steven Zimmerman, CPA <15812269+EffortlessSteven@users.noreply.github.com>
Date:   Sat Aug 22 02:25:06 2026 -0400
Subject: test(handoff): cover intelligence warning provenance (#622)
```

- **Type:** Single-commit test-and-docs hardening for the handoff
  `intelligence.json` warning provenance contract.
- **Claimed scope (from commit body):** "Test/docs only: no production,
  ranking, CLI, schema, CI, release, dependency, or security changes."
  Reviewed-and-confirmed by this scan.
- **Substantive delta in this checkout for the window:**
  - `crates/tokmd/tests/handoff_w71.rs` — adds the test
    `handoff_risk_no_git_records_hotspot_warning`. The test runs
    `tokmd handoff --preset risk --no-git --budget 20k --out-dir <tmp>`,
    parses `.handoff/intelligence.json`, and asserts:
    1. `hotspots` is `null` (not missing, not empty array).
    2. `warnings` is an array containing at least one entry whose string
       starts with `"hotspots unavailable: git history skipped"`.
    Failure modes are exercised via `anyhow::Result` + `ensure!`, so a
    missing/empty/renamed field surfaces as a test failure rather than a
    panic.
  - `docs/artifacts.md` line 119 — the `intelligence.json` row of the
    handoff-artifact table now explicitly names the `warnings` array as
    the channel for unavailable git-history enrichments, and treats a
    `hotspots unavailable: git history skipped...` string as an explicit
    capability warning. No schema change, no JSON-shape change.
  - All other files present in the `c8c3aa1` checkout are part of the
    repository's existing checked-in surface and were last substantively
    reviewed in earlier security scans referenced in this file's prior
    reports. They are not in the diff scope for this window.
- **Production code touched:** none. The warning-emitting path
  (`crates/tokmd/src/commands/handoff/intelligence.rs::build_intelligence`,
  lines 60–95) is unchanged at this commit. The four warning message
  strings the test exercises — `"hotspots unavailable: no git history
  found"`, `"hotspots unavailable: git history skipped (<reason>)"`,
  `"hotspots unavailable: git history skipped"`,
  `"hotspots unavailable: git history unavailable (<reason>)"` — are
  already present and unchanged.
- **Security-critical files re-read in place:**
  - `crates/tokmd-git/src/command.rs` — `GIT_REPO_SHAPING_ENV`
    (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_SSH`, `GIT_SSH_COMMAND`,
    `GIT_ASKPASS`, `GIT_PAGER`, `GIT_EDITOR`, `GIT_PROXY_COMMAND`,
    `GIT_EXTERNAL_DIFF`) still `env_remove`'d before subprocess invocation.
  - `crates/tokmd-git/src/refs.rs` — `env_base_ref_is_safe` still rejects
    empty, leading `-`, whitespace, control, backslash. `--end-of-options`
    separator still used.
  - `crates/tokmd-core/src/ffi/inputs.rs` —
    `validate_in_memory_input_path` still rejects empty, >4 KiB, control,
    leading `/` or `\`, Windows drive prefixes, `..` segments, all-`.`
    paths.
  - `crates/tokmd-core/src/ffi/parse.rs` — strict field decoders; type
    mismatch → `TokmdError::invalid_field`.
  - `crates/tokmd-core/src/ffi/mod.rs` — top-level JSON must be an object.
  - `crates/tokmd-scan/src/path/bounded_path.rs` — `BoundedPath` enforces
    canonical under-root; rejects `..` at any position; rejects RootDir /
    Prefix components.
  - `crates/tokmd-format/src/redact/mod.rs` — BLAKE3 short hash with
    extension allowlist (`tokmd-format/src/redact/extensions.rs`).
  - `crates/tokmd-analysis/src/content/mod.rs` — `ContentLimits` with
    `DEFAULT_MAX_FILE_BYTES = 128 KiB` plus a total `max_bytes` ceiling.
  - `crates/tokmd-cockpit/src/supply_chain.rs` — `parse_audit_output`
    returns `Pending` on malformed JSON; `pending_supply_chain_gate`
    returns `Pending` (never `Pass`) when `cargo audit` is missing.
  - `crates/tokmd-cockpit/src/gates/contracts.rs` — `Command::new("cargo")`
    uses `arg()` and `current_dir`; no shell.
  - `crates/tokmd-scan/src/walk/git.rs` — same `GIT_REPO_SHAPING_ENV`
    isolation as `tokmd-git/src/command.rs`.
  - `crates/tokmd-scan/src/exclude/mod.rs` — deterministic exclude pattern
    normalization; `unsafe_code` still forbidden.
  - `action.yml` — sha256 checksum verification on downloaded tokmd
    binary; verification-gated tag allowlist
    (`1.14.0 1.15.0`) for `runtime: container`; isolated anonymous
    `docker --config` for pulls; strict `output-dir` validation
    (rejects absolute and `..` segments) for `mode: packet`.
  - `.github/workflows/droid*.yml` — Droid action SHA-pinned
    (`EffortlessMetrics/droid-action-safe@7c1377ccbacddc95560d1570547a5baa51de01ec`)
    with explicit `ANTHROPIC_AUTH_TOKEN: ""` and `ANTHROPIC_BASE_URL: ""`
    to block ambient fallback.
  - `.github/workflows/ci.yml::typos` — SHA-pinned installer
    (`taiki-e/install-action@91ddec75689c4c78665b598d188dc821c5a43e5c
    # v2.85.9`) with `checksum: true` and `fallback: none`, then
    `run: typos`. Enforced by
    `xtask/tests/proof_plan_w92.rs::typos_install_contract_is_immutable_verified_and_fail_closed`.
    This is the `2026-08-17` hardening; it carries through `c8c3aa1`
    unchanged.
  - `.github/settings.yml` — `Tokmd Rust Result` and `Codex Review Gate`
    declared as status contexts for `main`; `allow_force_pushes: false`;
    `allow_deletions: false`. Live branch-protection state was not
    independently proven at scan time.
  - `deny.toml` — `RUSTSEC-2020-0163` ignore for transitive `term_size`
    unchanged; license allowlist unchanged.
  - `Cargo.toml` — workspace lints intact: `unsafe_code = "forbid"`,
    `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`,
    `unreachable = "deny"`, `dbg_macro = "deny"`,
    `unimplemented = "deny"`, `todo = "deny"`.

**The manual review recorded no security findings at or above the `medium`
threshold for `c8c3aa1`. The change is itself a STRIDE-positive hardening
across Tampering, Repudiation, and Information Disclosure, scoped entirely
to the test file and the docs row for `intelligence.json.warnings`. No
production, CLI, schema, CI, release, dependency, or security surface is
altered.**

## Scan Coverage Matrix

The coverage below applies to the `c8c3aa1` handoff warning provenance
addition and the re-verified standing defenses.

| Area | Files reviewed | Findings |
|------|----------------|----------|
| Handoff warning provenance test | `crates/tokmd/tests/handoff_w71.rs::handoff_risk_no_git_records_hotspot_warning` | 0 |
| Handoff artifacts docs update | `docs/artifacts.md` (intelligence.json row, line 119) | 0 |
| Handoff warning emitter (unchanged) | `crates/tokmd/src/commands/handoff/intelligence.rs` | 0 (no production change) |
| Handoff command entry (unchanged) | `crates/tokmd/src/commands/handoff.rs` | 0 (no production change) |
| Handoff output writers (unchanged) | `crates/tokmd/src/commands/handoff/output.rs` | 0 (no production change) |
| Git subprocess isolation | `crates/tokmd-git/src/command.rs`, `crates/tokmd-git/src/refs.rs`, `crates/tokmd/src/git_support.rs`, `crates/tokmd-scan/src/walk/git.rs` | 0 |
| FFI inputs | `crates/tokmd-core/src/ffi/mod.rs`, `inputs.rs`, `parse.rs` | 0 |
| Path handling | `crates/tokmd-scan/src/path/bounded_path.rs`, `crates/tokmd-scan/src/exclude/mod.rs` | 0 |
| File content reads | `crates/tokmd-analysis/src/content/mod.rs` (limits), `crates/tokmd-io-port/src/` | 0 |
| Redaction / hashing | `crates/tokmd-format/src/redact/mod.rs`, `extensions.rs` | 0 |
| Subprocess audit/semver | `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd-cockpit/src/gates/contracts.rs` | 0 |
| GitHub workflows | `.github/workflows/*.yml` (29 files), `.github/settings.yml`, `action.yml` | 0 |
| Build / lint | `Cargo.toml`, `deny.toml`, `clippy.toml`, `.cargo/config.toml` | 0 |
| Githooks | `.githooks/pre-commit`, `.githooks/pre-push`, `.claude/hooks/format-rust.sh` | 0 |
| Web runner (browser) | `web/runner/main.js`, `worker.js`, `auth.js`, `messages.js`, `runtime.js`, `ingest.js` | 0 |
| Threat model | `.factory/threat-model/threat-model.md` | unchanged |

## Patches Generated

No patches were generated this scan (no findings at or above `medium`).

## Appendix

### Threat Model

- **Status:** Within freshness window. The OBS-003 mixed-pinning note and
  OBS-006 transitive `term_size` ignore remain known pending refresh items;
  neither was touched in the `c8c3aa1` window.
- **Location:** `.factory/threat-model/threat-model.md`
- **Last Modified:** 2026-08-02 (22 days ago — well within 90-day window)
- **Methodology:** STRIDE
- **Next review:** 2026-11-01 (90-day cadence) or upon architecture change
- **No regeneration this scan** — the file is within its normal freshness
  window.

### Scan Metadata

- **Known commit reviewed:** `c8c3aa1987aeac40d5397936ec84519a82f8993a
  test(handoff): cover intelligence warning provenance (#622)`,
  2026-08-22.
- **Window completeness:** Not independently proven. `git log --since="7 days ago"
  --pretty=format:"%H %s"` returned exactly one commit (`c8c3aa1`) in the
  observed checkout, but no `git fetch` was executed in this report PR and
  the local checkout cannot establish full-history completeness on its own.
- **Files in scope:** 2 substantive files (the test addition and the docs
  row update). The full surface was previously reviewed under the
  `2026-06-29` true-merge baseline and re-verified in subsequent weekly
  scans; this scan re-verified all security-critical modules in place.
- **Scan Duration:** ~5m (focused diff review + defense re-verification)
- **Skills Used:** commit-security-scan (manual, STRIDE),
  vulnerability-validation (manual, exploitability assessment),
  security-review (manual, defense confirmation)
- **Manual Reviewers:** 1 (Droid scheduled security scan)
- **False Positive Filter:** applied — see Observations above

### Next Scan

The next scheduled security scan runs Monday, 2026-08-31 via
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
  `.factory/security/reports/security-report-2026-08-03.md`,
  `.factory/security/reports/security-report-2026-08-10.md`,
  `.factory/security/reports/security-report-2026-08-17.md`
