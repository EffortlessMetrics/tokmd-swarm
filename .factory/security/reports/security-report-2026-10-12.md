# Security Scan Report

**Generated:** 2026-10-12
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium
**Intended Window:** 2026-10-05 → 2026-10-12
**Observed Scope:** Checked-out commit `93e1ce2`; manual review of the
post-`93e1ce2` working tree plus a re-read of standing defenses.
**Window completeness limitation:** the inspected repository contains a
single root commit (`93e1ce2`, the only commit reachable from
`refs/heads/main`), so `git log --since="7 days ago"` returns exactly
that one commit. A `git log --reflog` and `git for-each-ref` of local
branches confirm no other reachable commits exist in the local working
copy. The 7-day window therefore reduces to a single-commit diff, but
the absence of historical commits prevents this scan from independently
proving the window covers all changes that landed in that interval on
the upstream history.

## Executive Summary

| Severity | Count | Auto-fixed | Manual Required |
|----------|-------|------------|-----------------|
| CRITICAL | 0     | 0          | 0               |
| HIGH     | 0     | 0          | 0               |
| MEDIUM   | 0     | 0          | 0               |
| LOW      | 0     | 0          | 0               |

**Total Findings:** 0
**Evidence limitation:** No automated vulnerability scanners or
heavyweight build/test witnesses ran. This is an advisory zero-finding
tally from manual STRIDE review, not a security pass or proof that the
repository has no vulnerabilities.
**Auto-fixed:** 0
**Manual Review Required:** 0

**Summary:** The only commit reachable from `refs/heads/main` in the
observed checkout is `93e1ce2 fix(config): expose failed legacy JSON
sources for recovery (#708)`, authored 2026-10-11 by `Steven Zimmerman,
CPA <EffortlessSteven@users.noreply.github.com>`. The commit message
describes a tightly scoped config-loading fix in `crates/tokmd/`, but
the diff itself covers **2,615 files / 1,041,915 insertions** and the
commit has **no parent** (`git rev-list --parents -n 1 93e1ce2` returns
only the commit hash; `git rev-list --max-parents=0 HEAD` lists
`93e1ce2` as the root). This is the only commit on the working branch,
and `origin/main` resolves to the same SHA. The repository's history
has been squashed into a single root commit, and the in-scope code
change relative to the commit message is the legacy-JSON-failure
exposure work described below.

The change in scope of the commit message is a STRIDE-positive
**Information Disclosure** / **Repudiation** reduction:

- `crates/tokmd/src/config.rs::load_config_checked` now distinguishes
  three failure modes for the legacy `~/.config/tokmd/config.json`:
  file absence (`Ok(None)`), `std::io::Error` read failures, and
  `serde_json::Error` parse failures. The failure is captured as
  `LoadedConfig::legacy_failure: Option<anyhow::Error>` instead of
  being silently swallowed.
- `check_selected_profile` chains the captured legacy failure onto an
  `UnknownProfile` error so the user sees both "your selected profile
  is unknown" and "your legacy JSON could not be read/parsed" with the
  repair guidance `Repair the legacy JSON configuration and retry.`
- `crates/tokmd/src/config/explain.rs::write_config_report_with_failure`
  renders the captured failure into the `--show-config` diagnostic
  surface so users can see exactly which source failed and why.
- `crates/tokmd/src/lib.rs` threads the captured failure from
  `load_config_checked` into both the `cli.show_config` diagnostic
  branch and the `check_selected_profile` call before command dispatch.

This is consistent with the repo's existing "expose source failures
for recovery" pattern (the previous `2026-08-31` report noted the
STRIDE-positive supply-chain hardening series, and the `2026-07-13`
report documented the redact / handoff provenance widening). The
new code does not introduce any new I/O sink, does not accept
attacker-controlled paths beyond the already-validated
`TOKMD_CONFIG` / XDG precedence chain, and does not weaken any
standing defense.

The change in scope of the actual diff (the rest of the
2,615-file diff) is a **repository history import / squash**: every
file in the working tree is `new file` relative to an absent parent,
including unrelated surfaces like `xtask/`, `fuzz/`, `web/runner/`,
`crates/tokmd-{core,scan,git,analysis,cockpit,format,...}/`,
`policy/*.toml`, `scripts/*`, `vendor/home-0.5.12/`, and the
previous `.factory/security/reports/security-report-*.md` baselines.
Because the parent is not available, this scan cannot reconstruct the
original commit-level diff for those surfaces. The scan therefore
re-verifies the standing defenses in-place from the current working
tree (see Standing Defenses below) instead of inferring regressions
from the missing history.

**Window-completeness note:** because the entire repo was repackaged
into `93e1ce2`, the strict `--since="7 days ago"` window is collapsed
to a single commit, and the prior weekly scans'
`2026-09-07` → `2026-10-05` interval is not represented in the local
checkout. The `droid-security-scan.yml` cron is `0 8 * * 1` (weekly
Monday), and the next scan will pick up any follow-up commits.

## Critical Findings

_None._

## High Findings

_None._

## Medium Findings

_None._

## Low Findings

_None._

## Observations (below severity threshold; tracked, not blocking)

### OBS-2026-10-12-01: Single-commit repository history

**Category:** Repudiation / process metadata
**Severity:** LOW (below threshold)
**Location:** `refs/heads/main` = `93e1ce2`; no parent reachable
**Status:** Tracked; no action this scan

`git rev-list --max-parents=0 HEAD` returns `93e1ce2`, and
`git rev-list --all --count` returns `1`. The only commit reachable
from the local working branch is a 2,615-file / 1,041,915-insertion
squash whose stated subject line ("fix(config): expose failed legacy
JSON sources for recovery") does not match the diff scope. The
`droid-security-scan.yml` workflow
(`.github/workflows/droid-security-scan.yml`) runs against
`refs/heads/main` and depends on a multi-commit history to compute
`git log --since="7 days ago"`. With a squashed single-commit
history, every weekly scan reduces to reviewing one commit, which
removes the per-PR diff context the scan was designed to surface.

**Why LOW:** the in-tree content of `93e1ce2` matches the prior
weekly scans' re-verified standing defenses (see Standing Defenses),
so the squashed tree is not a regression in defended surfaces.
The risk is that the squashed history drops the per-PR audit
trail for the `2026-08-31` → `2026-10-12` interval and any weekly
window that lands entirely inside the squashed history will only
review commit-message claims, not commit content.

**Recommended follow-up (not this scan's responsibility):**
restore the multi-commit history on `refs/heads/main` (e.g., rebase
the cumulative PR series on top of a true `origin/main` base), or
have the scheduled scan fetch a deeper window from upstream
(`fetch-depth: 250` or similar) and review the cumulative diff
even when the local working branch is squashed. The latter is
already what prior weekly reports recorded for shallow-clone
behavior and is the same documented release-surface shallow-clone
behavior the `2026-08-10` and `2026-08-17` reports noted.

### OBS-2026-10-12-02: No-Panic policy allowlist growth

**Category:** DoS / process metadata
**Severity:** LOW (below threshold; carried forward)
**Location:** `policy/no-panic-allowlist.toml` (416,962 insertions in
`93e1ce2`)
**Status:** Tracked; no action this scan

The `93e1ce2` diff grows `policy/no-panic-allowlist.toml` to
~417k lines, consistent with the standing practice of recording
per-callsite `unwrap` / `expect` suppressions as policy receipts
rather than code changes. The `2026-07-13` and `2026-07-27` reports
already noted that the no-panic allowlist is a policy receipt,
not a defended surface, and that the lints
`unwrap_used = "deny"`, `expect_used = "deny"`, and
`panic = "deny"` remain in force at the workspace level. This scan
re-verifies those lints are still set in `Cargo.toml` and that the
`no_panic` xtask gate remains the policy enforcement point.

### OBS-2026-10-12-03: Carried observations from prior reports

**Category:** Forwarded context
**Severity:** LOW (below threshold; carried forward)
**Status:** Forwarded, not new

- `OBS-002` (carried from `2026-06-29`): transitive
  `RUSTSEC-2020-0163` on `term_size` via `gix` / `tokei` chain is
  documented as an upstream limitation; not actionable in this
  workspace.
- `OBS-006` (carried from `2026-06-29`): branch-protection
  verification — the `2026-08-31` report already noted that the
  `Tokmd Rust Result` and `Codex Review Gate` status contexts are
  declared in `.github/settings.yml`, and that the checked-in
  settings file is not proof that public `main` is currently
  protected (the live protection state must be queried separately).
  Not re-verified this scan.

## Standing Defenses (re-verified from current working tree)

The prior weekly reports' standing-defense inventory was re-read
in-place against the working tree at `93e1ce2`. All defenses that
the prior scans marked as present remain present in the inspected
tree:

- **D-01 — Workspace lints.** `Cargo.toml` still sets
  `unsafe_code = "forbid"`, `unsafe_op_in_unsafe_fn = "deny"`,
  `unwrap_used = "deny"`, `expect_used = "deny"`,
  `get_unwrap = "deny"`, `unwrap_in_result = "deny"`,
  `panic_in_result_fn = "deny"`, `panic = "deny"`,
  `unreachable = "deny"`, `string_slice = "deny"`,
  `indexing_slicing = "deny"`, `out_of_bounds_indexing = "deny"`,
  plus the broader correctness / numerical / file-process / async
  lint set.
- **D-03 — `GIT_REPO_SHAPING_ENV` is `env_remove`'d before every git
  subprocess.** Verified in `crates/tokmd-git/src/command.rs`
  (`Command::new("git")` with `env_remove`), and in
  `crates/tokmd-scan/src/walk/git.rs`,
  `crates/tokmd-analysis/src/effort/delta.rs`,
  `crates/tokmd-core/src/context_git/mod.rs`. The test bodies in
  `crates/tokmd-git/tests/*.rs` consistently
  `env_remove("GIT_DIR")` and `env_remove("GIT_WORK_TREE")` before
  invoking git subprocesses.
- **D-04 — `env_base_ref_is_safe` still rejects empty, leading-`-`,
  whitespace, control, and backslash characters in
  `crates/tokmd-git/src/refs.rs`, paired with `--end-of-options`
  before positional refs.** Verified by re-reading the public
  surface; the unit tests in
  `crates/tokmd-git/src/refs.rs` (lines around 91) and the
  integration coverage in `crates/tokmd-git/tests/*.rs` continue
  to assert these rejection contracts.
- **D-05 — `BoundedPath::existing_relative` /
  `BoundedPath::existing_child` still enforce canonical under-root
  paths.** Re-verified at
  `crates/tokmd-scan/src/path/{bounded_path.rs,tests.rs}` and the
  call sites in `crates/tokmd-scan/src/walk/{git.rs,mod.rs}` and
  `crates/tokmd-analysis/src/effort/delta.rs`.
- **D-06 — `validate_in_memory_input_path` in
  `crates/tokmd-core/src/ffi/inputs.rs:106` still rejects the
  documented bypass set (empty, >4 KiB, control, leading `/` or
  `\`, Windows drive, `..`, all-`.` paths).** Re-verified at the
  declared call site `crates/tokmd-core/src/ffi/inputs.rs:66`.
- **D-07 — Strict FFI JSON parsing.** `parse.rs` (tokmd-core)
  still rejects type mismatches with typed `TokmdError` codes;
  no silent fallback to defaults.
- **D-09 / D-20 — Droid GitHub Actions remain SHA-pinned** with
  `ANTHROPIC_AUTH_TOKEN: ""` and `ANTHROPIC_BASE_URL: ""` in
  `.github/workflows/droid.yml`, `droid-review.yml`, and
  `droid-security-scan.yml`.
- **D-12 — `tokmd-format/src/redact/mod.rs` still uses BLAKE3**
  (`blake3::hash(cleaned.as_bytes()).to_hex().to_string()` at
  `crates/tokmd-format/src/redact/mod.rs:91`) with the
  extension allowlist and bare-hash fallback for untrusted
  compound suffixes.
- **D-13 — `DEFAULT_MAX_FILE_BYTES = 128 * 1024` in
  `crates/tokmd-analysis/src/complexity/mod.rs:42` and
  `crates/tokmd-analysis/src/halstead/mod.rs:10` is the per-file
  read ceiling**; `ContentLimits` enforces a total `max_bytes`
  ceiling on top of that.
- **D-19 — `action.yml` (the GitHub Action surface) still uses
  sha256 verification of the downloaded tokmd binary** against
  `checksums.txt`, and the `runtime: container` branch still
  requires a verification-gated tag from an explicit allowlist
  using an isolated `docker --config` directory.
- **D-21 / D-24 — `tokmd-cockpit/src/supply_chain.rs::parse_audit_output`
  still returns `Pending` on malformed JSON, never `Pass`**
  (`crates/tokmd-cockpit/src/supply_chain.rs:92` and
  `pending_supply_chain_gate` at the same module). The unit tests
  `parse_audit_output_fails_for_high_or_critical_vulnerabilities`,
  `parse_audit_output_warns_for_medium_vulnerabilities`, and
  `parse_audit_output_passes_when_no_vulnerabilities_are_listed`
  still cover the `Pending` / `Fail` / `Pass` decision matrix.
- **D-22 — `run_json_inner` still requires the top-level JSON to
  be an object** in `tokmd-core`'s FFI surface.

## Appendix

### Threat Model

- **Status:** Within freshness window (just barely).
- **Location:** `.factory/threat-model/threat-model.md`
- **Last Modified:** 2026-08-02 (71 days ago — within the 90-day
  window but approaching the renewal boundary).
- **Methodology:** STRIDE
- **Next review:** 2026-11-01 (90-day cadence) or upon architecture
  change. Because the threat model is 71 days old and the squashed
  history makes it harder to know whether the architecture actually
  changed in the lost interval, the next scheduled scan should
  re-validate the standing defenses and the trust-boundary list
  before letting the model roll over to 90+ days.
- **No regeneration this scan** — the file is within its normal
  freshness window. The next regeneration should fold in:
  - the `93e1ce2` legacy-JSON-failure exposure (this scan) which
    strengthens the Information Disclosure / Repudiation posture
    for `--show-config` and selected-profile recovery, and
  - the OBS-2026-10-12-01 single-commit-history observation so
    the next model iteration's scan-cadence guidance documents
    the shallow-history case.

### Scan Metadata

- **Strict window:** 2026-10-05 → 2026-10-12 — one commit
  reachable in the observed checkout: `93e1ce2`.
- **Commits reviewed:** 1 (`93e1ce2`).
- **Files in diff:** 2,615 (1,041,915 insertions; the entire
  working tree, because the commit is a root commit and the diff
  shows every file as `new file`).
- **Files in scope of the commit message:**
  `crates/tokmd/src/config.rs`,
  `crates/tokmd/src/config/explain.rs`,
  `crates/tokmd/src/lib.rs`,
  `crates/tokmd/src/cli/parser.rs`,
  `crates/tokmd/tests/config_resolution.rs`,
  `crates/tokmd/tests/show_config.rs` (read-only for context).
- **Files out of scope of the commit message but present in
  the diff:** the remaining ~2,609 files (the rest of the
  workspace). These could not be diffed against a parent (no
  parent exists), so this scan re-verifies the standing-defense
  surface from the current tree rather than reconstructing a
  regression list.
- **Window completeness:** Not independently proven. The local
  checkout has a single reachable commit on `main`, and
  `git log --since="7 days ago"` is therefore bounded to that
  one commit. A `git fetch --depth=50 origin main` (per the
  prior scans' documented release-surface shallow-clone behavior)
  would not help here, because the upstream `main` resolves to
  the same SHA.
- **Scan Duration:** ~10m (single-commit STRIDE review + standing
  defense re-verification across 11 named defenses + threat-model
  freshness check).
- **Skills Used:** commit-security-scan (manual, STRIDE),
  vulnerability-validation (manual, exploitability assessment of
  the captured `legacy_failure` propagation), security-review
  (manual, defense confirmation of standing inventory).
- **Manual Reviewers:** 1 (Droid scheduled security scan).
- **False Positive Filter:** applied — see Observations above
  for the LOW-tier items the scan surfaced but did not promote
  to findings.

### References

- [CWE Database](https://cwe.mitre.org/)
- [STRIDE Threat Model](https://docs.microsoft.com/en-us/azure/security/develop/threat-modeling-tool-threats)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [Rust Security Advisory Database](https://rustsec.org/)
- [CII Best Practices](https://www.bestpractices.dev/)
- Repository security policy: `SECURITY.md`
- Repository threat model: `.factory/threat-model/threat-model.md`
- Previous scans:
  `.factory/security/reports/security-report-2026-06-01.md`,
  `.factory/security/reports/security-report-2026-06-08.md`,
  `.factory/security/reports/security-report-2026-06-29.md`,
  `.factory/security/reports/security-report-2026-07-06.md`,
  `.factory/security/reports/security-report-2026-07-13.md`,
  `.factory/security/reports/security-report-2026-07-20.md`,
  `.factory/security/reports/security-report-2026-07-27.md`,
  `.factory/security/reports/security-report-2026-08-03.md`,
  `.factory/security/reports/security-report-2026-08-10.md`,
  `.factory/security/reports/security-report-2026-08-17.md`,
  `.factory/security/reports/security-report-2026-08-24.md`,
  `.factory/security/reports/security-report-2026-08-31.md`
