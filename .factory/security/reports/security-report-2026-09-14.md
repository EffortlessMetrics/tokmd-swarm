# Security Scan Report

**Generated:** 2026-09-14
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium
**Intended Window:** 2026-09-07 → 2026-09-14
**Observed Scope:** Checked-out commit `4241d83`; `git log --since="7 days ago"`
returned exactly one commit in this checkout for the intended window. The
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
intended 2026-09-07 → 2026-09-14 window:

```
4241d8385abdb6fa8992b382c750f1fa74bde191
Author: factory-droid[bot] <138933559+factory-droid[bot]@users.noreply.github.com>
Date:   Mon Sep 7 21:05:41 2026 -0400
Subject: docs(security): record August 31 manual scan with bounded coverage (#626)
```

The commit is a single-purpose reporting correction that re-states facts
about the prior `2026-08-31` manual review against the immutable inspected
source. Its claimed scope — "Files changed: 1 historical security report;
Composition: Documentation only; API / CLI / schema changes: None" —
is consistent with the diff in this checkout. The substantive change is a
single file (`.factory/security/reports/security-report-2026-08-31.md`)
whose additions are bounded: zero-finding tally preserved, adjacent-window
review subset disclosed, scanner line count corrected, locked-Cargo
guidance scoped to its governed command families, branch-protection and
action-pinning descriptions aligned with the inspected source, and the
handoff warning test claim bounded to its actual oracle.

The body of the commit explicitly distinguishes reporting correction from
security verdict: "the correction checks those reporting facts against the
immutable inspected source; it does not rerun the security scan or add
retrospective security verdicts." This is consistent with what the
diff shows — no source file outside `.factory/security/reports/` was
modified in the commit.

This is a **STRIDE-neutral** change across every category. The report
file is `docs/`-only Markdown and is not part of any trust boundary,
subprocess invocation, FFI surface, secret/env surface, CLI flag,
schema, dependency, or release surface. The corrections it carries
preserve the original manual reviewer's zero-finding tally for the
named subset of adjacent commits; they do not introduce, remove, or
weaken any defense.

| STRIDE | Direction | Why |
|--------|-----------|-----|
| Spoofing | N/A | No code or workflow change. |
| Tampering | N/A | No code or workflow change. |
| Repudiation | N/A | No code or workflow change. |
| Information Disclosure | N/A | No code or workflow change. The amendment corrects earlier over-broad wording (e.g., scoped locked-Cargo claim, bounded handoff warning oracle, accurate scanner line count); no new information is exposed. |
| Denial of Service | N/A | No code or workflow change. |
| Elevation of Privilege | N/A | No code or workflow change. |

The workspace-wide standing defenses were re-read in place at the
checked-out commit `4241d83` (see Standing Defenses table below). No
defenses were observed to have regressed, and no source file outside
`.factory/security/reports/security-report-2026-08-31.md` carries a
diff for this commit.

The threat model at `.factory/threat-model/threat-model.md` is dated
2026-08-02 — 43 days old — still well within the 90-day regeneration
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

The commit is a reporting correction; it does not introduce any new
observation. The carried observations below retain their identifiers from
the [August 31 report](security-report-2026-08-31.md) and the
[August 17 report](security-report-2026-08-17.md). The amendment aligns
the carried OBS-003 (mixed pinning) and OBS-006 (branch protection)
descriptions with the inspected source; it does not close them and does
not change the original manual review's prior classification.

- **OBS-002 (carried):** `RUSTSEC-2020-0163` (transitive `term_size`) is
  recorded as ignored in `deny.toml`. The advisory concerns the unmaintained
  `term_size` crate, transitive via `tokei`. The `home` crate vendored at
  `vendor/home-0.5.12` under `[patch.crates-io]` is an unrelated
  dependency-pinning note. Not in this commit's scope (the commit is
  docs-only).
- **OBS-003 (carried, with wording aligned in 4241d83):** Mixed GitHub
  Action pinning posture in `.github/workflows/*.yml`. The typos
  installer and Droid wrapper are SHA-pinned; other CI steps retain
  mutable references (`actions/upload-artifact@v7`,
  `actions/setup-node@v7`, `Swatinem/rust-cache@v2`, and
  `taiki-e/install-action@v2`). `actions/checkout` is SHA-pinned at
  `3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1` across the
  workflows that use it. The custom Droid action — the
  highest-privilege third-party surface — IS SHA-pinned. The
  `4241d83` correction makes this mixed-pinning inventory explicit in the
  carried August 31 report's standing-defense table; it does not change
  the underlying config.
- **OBS-006 (carried, with wording aligned in 4241d83):** Branch
  protection review requirements at the carried inspected commit
  (`c8c3aa1`) — `.github/settings.yml` configures
  `required_approving_review_count: 0` and
  `require_code_owner_reviews: false` for `main`. Live enforcement
  and per-PR execution of the required status context were not
  independently proven by this scan. The `4241d83` correction updates
  the carried description to match the inspected source; live state
  remains unverified.

The other observations from the August 17 report were not reassessed in
this focused docs-only scan. Their omission from the summary is not
closure or fresh confirmation of their original descriptions:

| Prior ID | Prior subject | Disposition in this report |
|---------|---------------|----------------------------|
| OBS-001 | FFI JSON payload size | Not reassessed; see prior report. |
| OBS-004 | Browser GitHub API base URL | Not reassessed; see prior report. |
| OBS-005 | Action binary download | Not reassessed; see prior report. |
| OBS-007 | Typos installer pin/comment correspondence | Not reassessed; see prior report. |
| OBS-008 | `cargo_command_surfaces` proof scope does not cover historical surfaces | Not reassessed; see prior report. |

The `4241d83` change adds no new observation. The amendment corrects
reporting wording on the carried observations above.

## Standing Defenses Re-read in the Inspected Tree

The following defenses were re-read during this scan. Presence in the
inspected tree is recorded here; this is the same defense inventory that
prior weekly scans have re-verified.

| ID | Defense | Location | Verified |
|----|---------|----------|----------|
| D-01 | `unsafe_code = "forbid"` workspace lint | `Cargo.toml` (line 56) | ✓ |
| D-02 | `unwrap_used`, `expect_used`, `panic`, `unreachable`, `dbg_macro`, `todo`, `unimplemented` lints denied | `Cargo.toml` (lines 65–71) | ✓ |
| D-03 | Git subprocess env isolation (`GIT_REPO_SHAPING_ENV`) | `crates/tokmd-git/src/command.rs` (lines 9, 35, 54), `crates/tokmd/src/git_support.rs`, `crates/tokmd-scan/src/walk/git.rs` | ✓ |
| D-04 | Git ref validation (`env_base_ref_is_safe` + `--end-of-options`) | `crates/tokmd-git/src/refs.rs` | ✓ |
| D-05 | Bounded path canonicalization under root | `crates/tokmd-scan/src/path/bounded_path.rs` (`existing_relative`, `ensure_under_root`) | ✓ |
| D-06 | FFI in-memory input path validation | `crates/tokmd-core/src/ffi/inputs.rs` (`MAX_IN_MEMORY_INPUT_PATH_BYTES = 4096`, line 12) | ✓ |
| D-07 | Strict JSON parsing with type validation | `crates/tokmd-core/src/ffi/parse.rs` | ✓ |
| D-08 | Per-family schema versioning (`SCHEMA_VERSION=2`, `COCKPIT_SCHEMA_VERSION=3`, `HANDOFF_SCHEMA_VERSION=5`, `CONTEXT_SCHEMA_VERSION=4`, `CONTEXT_BUNDLE_SCHEMA_VERSION=2`, `ANALYSIS_SCHEMA_VERSION=9`) | `crates/tokmd-types/src/lib.rs`, `cockpit.rs`, `context.rs`, `crates/tokmd-analysis-types/src/lib.rs` | ✓ |
| D-09 | Mixed action references: custom Droid action and typos installer are SHA-pinned; `ci.yml` also retains `actions/upload-artifact@v7`, `actions/setup-node@v7`, `Swatinem/rust-cache@v2`, and `taiki-e/install-action@v2` | `.github/workflows/droid*.yml`, `.github/workflows/ci.yml` | inspected mixed references |
| D-10 | Branch-protection settings for `main` are present (status checks required, no force-push, no deletions); live enforcement and per-PR execution were not independently proven by this scan | `.github/settings.yml` | configured; live enforcement unverified |
| D-11 | `cargo-deny` advisory + license allowlist | `deny.toml` (`RUSTSEC-2020-0163` ignore for transitive `term_size` via `tokei`) | ✓ |
| D-12 | BLAKE3 redaction with extension allowlist | `crates/tokmd-format/src/redact/mod.rs` (lines 53, 91, 101), `extensions.rs` | ✓ |
| D-13 | Content reads bounded by `ContentLimits` (`DEFAULT_MAX_FILE_BYTES = 128 KiB`) | `crates/tokmd-analysis/src/content/mod.rs` (lines 17, 27, 35) | ✓ |
| D-14 | PyO3 FFI invariants (no panic, GIL release, error translation) | `crates/tokmd-python/src/lib.rs` | ✓ |
| D-15 | WASM uses `MemFs` (no host fs) | `crates/tokmd-wasm/` | ✓ |
| D-16 | `web/runner` browser runner uses `textContent` (no `innerHTML`/`eval`/`new Function`/`document.write`) | `web/runner/main.js` | ✓ |
| D-17 | `web/runner` token stored in `sessionStorage` (not `localStorage`) | `web/runner/auth.js` | ✓ |
| D-18 | `web/runner` worker protocol allowlists modes & presets | `web/runner/messages.js` | ✓ |
| D-19 | Composite action installs tokmd with sha256 checksum verification; verification-gated tag allowlist for `runtime: container` | `action.yml` (lines 258–279) | ✓ |
| D-20 | Custom Droid action SHA-pinned across all Droid workflows; explicit `ANTHROPIC_AUTH_TOKEN: ""` / `ANTHROPIC_BASE_URL: ""` to block ambient fallback | `.github/workflows/droid*.yml` | ✓ |
| D-21 | `cargo audit` invoked with structured `--json` output, malformed JSON treated as Pending | `crates/tokmd-cockpit/src/supply_chain.rs` (lines 29, 36) | ✓ |
| D-22 | `run_json` top-level JSON must be an object (strict shape check) | `crates/tokmd-core/src/ffi/mod.rs::run_json_inner` | ✓ |
| D-23 | Author DAG import via true-merge commits (no force-push of publication history) | repository topology | not verifiable from this shallow clone |
| D-24 | `supply_chain` gate explicitly tolerates missing `cargo audit` binary by returning `Pending`, never `Pass` (per `pending_supply_chain_gate` constructor) | `crates/tokmd-cockpit/src/supply_chain.rs` | ✓ |
| D-25 | `Command::new("cargo")` and `Command::new("git")` invocations use `arg()` (not shell) and `current_dir` for path control, no `sh -c` / `bash -c` | `crates/tokmd-cockpit/src/gates/contracts.rs`, `tokmd-cockpit/src/supply_chain.rs`, `tokmd-git/src/command.rs`, `crates/tokmd-scan/src/walk/git.rs` | ✓ |
| D-26 | Typos lane install contract: SHA-pinned action, pinned tool version, `checksum: true`, `fallback: none`, plus a structural drift/fork rejection test | `.github/workflows/ci.yml::typos` (line 919 pins `taiki-e/install-action@91ddec75689c4c78665b598d188dc821c5a43e5c # v2.85.9`), `xtask/tests/proof_plan_w92.rs::typos_install_contract_is_immutable_verified_and_fail_closed` | ✓ |
| D-27 | `cargo_command_surfaces` adoption guard: closed-world inventory in `policy/cargo-command-surfaces.toml` (`schema_version = 1`), deterministic tracked-file scanner in `xtask/tests/cargo_command_surfaces_w104.rs`, routed through `ci/proof.toml::cargo_command_surfaces` proof scope; scanner is text-only and never executes guidance | `policy/cargo-command-surfaces.toml`, `xtask/tests/cargo_command_surfaces_w104.rs`, `ci/proof.toml` | ✓ |
| D-28 | Canonical governed Cargo `build`, `check`, `test`, `clippy`, `run`, and `install` guidance uses `--locked`, with explicit "this source install is reproducible only to the committed lock" framing and tracked under `tokmd-swarm#604` / `depguard#21` / `depguard#22` / `depguard#24`; formatter aliases, xtask, and other ungoverned subcommands are outside this claim | `AGENTS.md`, `agents/shared/repo.md` | ✓ |

## Scan Coverage Matrix

The coverage below applies to the `4241d83` reporting correction and the
standing defenses. The single substantive file change in the strict window
is the August 31 report itself; no production, test, workflow, schema, or
release-surface file carries a diff in this commit.

| Area | Files reviewed | Findings |
|------|----------------|----------|
| Reporting correction | `.factory/security/reports/security-report-2026-08-31.md` | 0 |
| Git subprocess isolation | `crates/tokmd-git/src/command.rs`, `crates/tokmd-git/src/refs.rs`, `crates/tokmd/src/git_support.rs`, `crates/tokmd-scan/src/walk/git.rs` | 0 (unchanged) |
| FFI inputs | `crates/tokmd-core/src/ffi/mod.rs`, `inputs.rs`, `parse.rs` | 0 (unchanged) |
| Path handling | `crates/tokmd-scan/src/path/bounded_path.rs`, `crates/tokmd-scan/src/exclude/mod.rs` | 0 (unchanged) |
| File content reads | `crates/tokmd-analysis/src/content/mod.rs` (limits), `crates/tokmd-io-port/src/` | 0 (unchanged) |
| Redaction / hashing | `crates/tokmd-format/src/redact/mod.rs`, `extensions.rs` | 0 (unchanged) |
| Subprocess audit/semver | `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd-cockpit/src/gates/contracts.rs` | 0 (unchanged) |
| GitHub workflows | `.github/workflows/*.yml` (29 files), `.github/settings.yml`, `action.yml` | 0 (unchanged) |
| Build / lint | `Cargo.toml`, `deny.toml`, `clippy.toml`, `.cargo/config.toml` | 0 (unchanged) |
| Githooks | `.githooks/pre-commit`, `.githooks/pre-push`, `.claude/hooks/format-rust.sh` | 0 (unchanged) |
| Web runner (browser) | `web/runner/main.js`, `worker.js`, `auth.js`, `messages.js`, `runtime.js`, `ingest.js` | 0 (unchanged) |
| Threat model | `.factory/threat-model/threat-model.md` | unchanged |

## Commit-level Analysis

The strict intended 2026-09-07 → 2026-09-14 window contains exactly one
commit in the observed checkout:

```
4241d8385abdb6fa8992b382c750f1fa74bde191
Author: factory-droid[bot] <138933559+factory-droid[bot]@users.noreply.github.com>
Date:   Mon Sep 7 21:05:41 2026 -0400
Subject: docs(security): record August 31 manual scan with bounded coverage (#626)
```

- **Type:** Single-commit reporting correction. Body claims: "Files
  changed: 1 historical security report; Composition: Documentation only;
  API / CLI / schema changes: None." Reviewed-and-confirmed by this scan.
- **Surface:** 1 file (`.factory/security/reports/security-report-2026-08-31.md`).
- **Net code change:** None. The only diff in this commit is the August 31
  report, which is documentation of a prior manual review.
- **Diff scope relative to the immutable inspected source
  `c8c3aa1987aeac40d5397936ec84519a82f8993a`:** the report's
  `Reporting correction (2026-09-07)` section records the immutable source,
  commands, counts, and evidence boundary used to verify its claims. The
  amendment preserves the original scan date (`2026-08-31`), reviewer
  count (1), duration (~5m), manual methodology, and advisory
  zero-finding tally from the original report. It explicitly disclaims
  retrospective security verdicts: "The original scan date, duration,
  reviewer count, and manual methodology above remain historical
  provenance."
- **What changed in the report, by topic:**
  - **Reviewed subset:** explicitly disclosed as ten of sixteen adjacent
    commits; six omitted adjacent commits named without a security verdict.
    The omission is recorded as a coverage gap, not a zero-finding result.
  - **Scanner line count:** corrected to 582 lines including blanks at the
    inspected source.
  - **Locked-Cargo guidance scope:** narrowed to the documented governed
    `build`, `check`, `test`, `clippy`, `run`, and `install` command
    families. Formatter aliases, xtask commands, and other ungoverned
    subcommands remain outside the claim.
  - **Branch-protection description:** aligned with the inspected
    `.github/settings.yml` source at `c8c3aa1` (only `Tokmd Rust Result`
    is a required status context; live enforcement was not independently
    proven).
  - **Action-pinning inventory:** made explicit that `ci.yml` retains
    tag-pinned references (`actions/upload-artifact@v7`,
    `actions/setup-node@v7`, `Swatinem/rust-cache@v2`,
    `taiki-e/install-action@v2`) alongside the SHA-pinned Droid and
    typos installer lanes; `actions/checkout` SHA-pinned at
    `3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1`.
  - **Handoff warning test oracle:** scoped to the test's specific
    contract (the `hotspots unavailable: git history skipped` warning
    prefix and `null` hotspots), not to all warning variants the producer
    can emit.
  - **Carried observations:** OBS-003 wording aligned with inspected
    source; OBS-006 wording aligned with inspected source; OBS-007
    description preserved; OBS-008 description preserved (the
    `cargo_command_surfaces` historical-scope boundary is policy intent,
    not a security gap).
- **STRIDE analysis:** STRIDE-neutral. The file is `docs/`-only Markdown
  under `.factory/security/reports/` and is not part of any trust boundary,
  subprocess invocation, FFI surface, secret/env surface, CLI flag,
  schema, dependency, or release surface. The corrections tighten prior
  wording to match the inspected source rather than alter defenses.

**Security-critical files re-read in place:**
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
- `crates/tokmd-types/src/lib.rs`,
  `crates/tokmd-types/src/cockpit.rs`,
  `crates/tokmd-types/src/context.rs`,
  `crates/tokmd-analysis-types/src/lib.rs` —
  schema versioning unchanged (`SCHEMA_VERSION=2`,
  `COCKPIT_SCHEMA_VERSION=3`, `HANDOFF_SCHEMA_VERSION=5`,
  `CONTEXT_SCHEMA_VERSION=4`, `CONTEXT_BUNDLE_SCHEMA_VERSION=2`,
  `ANALYSIS_SCHEMA_VERSION=9`).
- `policy/cargo-command-surfaces.toml` — closed-world inventory
  (`schema_version = 1`) with explicit classification per candidate
  root, no execution surface.
- `xtask/tests/cargo_command_surfaces_w104.rs` — deterministic
  tracked-file scanner; recognizes short global cargo options;
  treats unknown pre-command options as `NotProven`; stops scanning
  after `--`; verifies lock-preservation only for the parsed
  cargo-prefix token span.
- `xtask/tests/affected_w91.rs` — updated affected-scope fixture for
  the `cargo_command_surfaces` scope.
- `ci/proof.toml` — routes the `cargo_command_surfaces` proof scope.
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
- `.github/settings.yml` — only `Tokmd Rust Result` is a required status
  context for `main`; `required_conversation_resolution: true`;
  `allow_force_pushes: false`; `allow_deletions: false`. This records the
  inspected file; live enforcement at scan time and current
  branch-protection state were not independently proven by this scan.
- `deny.toml` — `RUSTSEC-2020-0163` ignore for transitive `term_size`
  unchanged; license allowlist unchanged.
- `AGENTS.md` and `agents/shared/repo.md` — documented governed Cargo
  `build`, `check`, `test`, `clippy`, `run`, and `install` commands pass
  `--locked`; source-install framing explicit. This does not cover formatter
  aliases, xtask commands, or other ungoverned subcommands.

**The manual review recorded no security findings at or above the `medium`
threshold for `4241d83`. The change is itself a STRIDE-neutral reporting
correction scoped entirely to one historical security report under
`.factory/security/reports/`. No production, CLI, schema, CI, release,
dependency, or security surface is altered.**

## Patches Generated

No patches were generated this scan (no findings at or above `medium`).

## Appendix

### Threat Model

- **Status:** Within freshness window. The carried observations
  (OBS-002 transitive `term_size`, OBS-003 mixed action pinning, OBS-006
  branch protection) remain pending items for the next regeneration; none
  were touched in the `4241d83` docs-only window.
- **Location:** `.factory/threat-model/threat-model.md`
- **Last Modified:** 2026-08-02 (43 days ago — well within 90-day window)
- **Methodology:** STRIDE
- **Next review:** 2026-11-01 (90-day cadence) or upon architecture change
- **No regeneration this scan** — the file is within its normal freshness
  window. The next regeneration should fold in the typos-lane SHA-pinning
  update from `24d5a53` (carried OBS-003) and the `#604` locked-Cargo-
  command adoption guard (`f3cfd24` + `c2f77f0` + `7d192f0`); both are
  recorded as known pending items by the carried August 31 report.

### Scan Metadata

- **Strict window:** 2026-09-07 → 2026-09-14 — exactly one commit in the
  observed checkout (`4241d83`).
- **Known commit reviewed (most recent on working branch):**
  `4241d8385abdb6fa8992b382c750f1fa74bde191 docs(security): record August 31
  manual scan with bounded coverage (#626)`, 2026-09-07.
- **Window completeness:** Not independently proven. `git log --since="7
  days ago" --pretty=format:"%H %s"` returned exactly one commit
  (`4241d83`) in the observed checkout, but no `git fetch` was executed
  in this report PR and the local checkout cannot establish
  full-history completeness on its own. The previous scan report
  (2026-08-31) recorded a bounded `git fetch --depth=50 origin main`;
  that fetch is not refreshed by this docs-only commit and this scan
  does not rerun it.
- **Files in scope:** 1 substantive file (the August 31 reporting
  correction). The full surface was previously reviewed under the
  `2026-06-29` true-merge baseline and re-verified in subsequent weekly
  scans; this scan re-verified all security-critical modules in place.
- **Diff scope evidence:** `git show 4241d83 -- .factory/security/reports/security-report-2026-08-31.md --numstat`
  yields `782 0` — that is, `782` insertions and `0` deletions against the
  parent for the only file path present in the commit's diff. Because the
  working tree is a shallow clone (single commit reachable), every other
  file appears as `A` relative to the empty root in `git diff-tree`; the
  `git show -- <path>` filter restricts the diff to the report file only.
  No other source file is in the commit's diff.
- **Scan Duration:** ~5m (focused diff review + defense re-verification)
- **Skills Used:** commit-security-scan (manual, STRIDE),
  vulnerability-validation (manual, exploitability assessment),
  security-review (manual, defense confirmation)
- **Manual Reviewers:** 1 (Droid scheduled security scan)
- **False Positive Filter:** applied — see Observations above

### Next Scan

The next scheduled security scan runs Monday, 2026-09-21 via
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
  `.factory/security/reports/security-report-2026-08-17.md`,
  `.factory/security/reports/security-report-2026-08-24.md`,
  `.factory/security/reports/security-report-2026-08-31.md`
