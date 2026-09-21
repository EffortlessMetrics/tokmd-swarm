# Security Scan Report

**Generated:** 2026-09-21
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium
**Intended Window:** 2026-09-14 → 2026-09-21
**Observed Scope:** Checked-out commit `4241d83`; `git log --since="7 days ago"`
reported no other commits in this checkout for the intended window. The scan
does not independently prove window completeness: the appendix records only a
bounded `git fetch --depth=50 origin main`, so the zero-finding tally is limited
to the observed checkout and must not be read as a full-history claim.
The adjacent manual review covers the five commits named below since the
previous scheduled scan (`c8c3aa1` was the most recent commit recorded in
the 2026-08-31 report and remains the strict-window baseline reference).
Six other reachable commits in that interval have no commit-review record in
this report; see Scan Metadata.

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

**Summary:** `git log --since="7 days ago" --pretty=format:"%H %s"` returned
no commits in the observed checkout for the intended 2026-09-14 → 2026-09-21
window. The most recent commit on the working branch is
`4241d83 docs(security): record August 31 manual scan with bounded coverage
(#626)` from 2026-09-07 — seven days outside the strict 7-day window. The
manual review recorded five commits in the adjacent 2026-09-07 interval
between this scan and the prior scheduled scan, all of which are
STRIDE-positive or STRIDE-neutral for their respective categories.

Specifically:

- `7f0d675 ci(gate): run the required Cargo proof locked (#625)` — adds
  `--locked` to the required `Tokmd Rust Result` gate's outer `xtask`
  launchers and its build, Clippy, and test commands. The change lands a
  focused contract test
  `xtask/tests/required_gate_locked_commands_w108.rs` (382 lines) that
  proves the locked workflow shape, the locked gate task steps, and the
  documented guidance claim, and that the scanner has positive/negative
  controls. The xtask gate implementation
  (`xtask/src/tasks/gate.rs`) now passes `--locked` to `check`, `clippy`,
  and `test --no-run`, with `cargo fmt` retained as the documented positive
  control exception because fmt resolves no dependencies and rejects the
  flag. This is a STRIDE-positive Tampering / Elevation of Privilege
  reduction: missing or stale lock state can no longer be repaired
  silently before the inner checks run, and the locked claim is now bound
  to executable CI receipts instead of being guidance-only.

- `e3323ff chore(deps): bump the github-actions group across 1 directory
  with 3 updates (#629)` — dependabot bumps three GitHub Actions. The
  single SHA-pinned reference at
  `.github/workflows/ci.yml` (`actions/upload-artifact@043fb46d1a93c77aae656e7c1c1c64a875d1fc6a0a
  # v7.0.1`) is updated with a new SHA. Other tag-pinned
  `actions/upload-artifact@v7` references across the same workflow are
  bumped to `actions/upload-artifact@v7.0.1`. `docker/setup-qemu-action`
  and `cachix/install-nix-action` are already SHA-pinned and unchanged in
  this PR. This is STRIDE-neutral: the previously SHA-pinned reference
  keeps its pinning contract, and the mixed-pinning inventory recorded in
  OBS-003 of the prior report is preserved (the highest-privilege third-
  party surface remains SHA-pinned).

- `6c5aae9 chore(deps): bump the rust-minor-patch group across 1 directory
  with 14 updates (#628)` — Dependabot bumps thirteen Rust crates to their
  minor/patch versions in `Cargo.lock`. No `Cargo.toml`, no source code, no
  policy changes. STRIDE-neutral dependency maintenance.

- `65eabd7 docs(security): record August 24 manual review and advisory
  scope (#623)` and `4241d83 docs(security): record August 31 manual scan
  with bounded coverage (#626)` — historical weekly reports committed
  to `.factory/security/reports/`. STRIDE-neutral provenance artifacts.

The workspace-wide standing defenses were re-read in place and remain
present at the checked-out commit (see Standing Defenses table below). No
defenses were observed to have regressed.

The threat model at `.factory/threat-model/threat-model.md` is dated
2026-08-02 (last committed via `b4ff2c9 Merge publication import for
release alias promotion` on 2026-08-04) — 48 days old — still well
within the 90-day regeneration window. No regeneration this scan.

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
caller to opt in. Out of scope per `SECURITY.md`. No change in this scan's
commits.

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
Out of scope per `SECURITY.md`. No change in this scan's commits.

**Recommended action:** Track upstream `tokei` for a `term_size` removal.

### OBS-003 (carried, partially narrowed): GitHub Actions pinning is mixed (tag + SHA)

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
Other workflow references use a mixture of tags and SHAs. `actions/checkout`
is SHA-pinned at `3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1` across
the workflows that use it.

This scan's `e3323ff` (dependabot) updated the SHA-pinned
`actions/upload-artifact@043fb46d1a93c77aae656e7c1c1c64a875d1fc6a0a # v7.0.1`
in `ci.yml` line 439 (the single location with a SHA pin) and refreshed
other tag-pinned `actions/upload-artifact@v7` references in `ci.yml` to
`@v7.0.1`. The other SHA-pinned third-party actions that `e3323ff`
references — `docker/setup-qemu-action@1f40c72289eff860ee54a304f1438e3cff362e0a
# v4` in `release-candidate.yml` and
`cachix/install-nix-action@13d8dd58da0234aa297dedd986986ccb8e7f3e24 # v31`
in `release-consumer-smoke.yml` — were already SHA-pinned and remained
unchanged in this PR. After `e3323ff`, the highest-privilege third-party
surfaces remain SHA-pinned.

The previous scan's `24d5a53` commit narrowed the gap for the typos lane:
the prior mutable `crate-ci/typos@v1` reference in
`.github/workflows/ci.yml::typos` was replaced with a SHA-pinned
`taiki-e/install-action@91ddec75689c4c78665b598d188dc821c5a43e5c # v2.85.9`
plus an exact `tool: typos@1.49.0` and structural test coverage to
prevent regression.

**Why not a finding:**
- The original reviewer classified the remaining tag references below the
  scan's reporting threshold; this is a manual assessment, not proof of
  immutable dependency identity.
- The custom Droid action — the highest-privilege third-party surface — IS
  SHA-pinned.
- After `24d5a53`, the typos lane is also SHA-pinned with structural test
  enforcement. In the inspected `ci.yml`, tag references still include
  `actions/upload-artifact@v7.0.1` (refreshed by `e3323ff`),
  `actions/setup-node@v7`, `Swatinem/rust-cache@v2`, and
  `taiki-e/install-action@v2`; other workflow steps retain SHA pins. A pin
  in one job does not establish a repository-wide pinning contract.
- Below the `medium` severity threshold for this scan; flagged for the next
  threat-model refresh (target: 2026-11-01 or earlier if scope changes).

**Recommended action (optional, future):** Either update the threat model
to reflect the actual mixed-pinning policy with the typos lane and the
`actions/upload-artifact` SHA-pinned location listed as SHA-pinned
exceptions, or convert all third-party tool-installer actions to SHA-pinned
references and codify the rotation process in `.factory/rules/`.

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
  `textContent`; no use of `innerHTML`, `eval`, `new Function`, or
  `document.write`.
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

**Description:** At the checked-out commit `4241d83`, `.github/settings.yml`
configures `required_approving_review_count: 0` and
`require_code_owner_reviews: false` for `main`. The same historical file
declares only `Tokmd Rust Result` as a required status context and enables
`required_conversation_resolution: true`. Its comments require independent
agentic review in separate lanes as repo process while keeping bot review
statuses advisory; native human approval and CODEOWNERS review are
intentionally absent. This updates the carried OBS-006 description to the
inspected source; it does not claim live settings enforcement.

**Why not a finding:**
- The checked-in policy is narrow and explicit: `enforce_admins: false`,
  `allow_force_pushes: false`, `allow_deletions: false`, one required
  status context, and required conversation resolution.
- Live enforcement and per-PR execution of that context were not
  independently proven by this scan (`gh api repos/.../branches/main/protection`
  returns HTTP 403 for the integration token used by this scan).
- The checked-out tree's `.github/settings.yml` comments distinguish the
  independent agentic review process from native approval and status gates;
  the threat model's contradictory approval text remains pending refresh.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional, future):** When the maintainer count
grows, increase `required_approving_review_count` and re-enable
`require_code_owner_reviews`.

### OBS-007 (carried): `taiki-e/install-action` SHA must match `# v2.85.9` comment

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Spoofing / Tampering |
| **File** | `.github/workflows/ci.yml` (typos job) |
| **Status** | Not patched — process control |

**Description:** The typos job introduced by `24d5a53` pins the install
action with both a SHA (`91ddec75689c4c78665b598d188dc821c5a43e5c`) and a
human-readable comment (`# v2.85.9`). The contract test in
`xtask/tests/proof_plan_w92.rs::typos_install_contract_is_immutable_verified_and_fail_closed`
verifies the SHA pin, the exact `with:` values, and the no-fallback
setting, but does not verify the trailing `# v2.85.9` comment matches the
SHA. If the SHA is ever rotated without updating the comment, the
comment becomes misleading documentation rather than a mismatched
security control.

**Why not a finding:**
- The SHA itself is what GitHub resolves and what the contract test
  enforces. The comment is a human-readable annotation for reviewers.
- Rotating the SHA without updating the comment is a documented CI
  review checklist item, not a security control failure.
- The contract test fails closed if the SHA, version, checksum, or
  fallback settings drift in any direction.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional, future):** Add a separate test that
fetches the tag→commit mapping for the documented `taiki-e/install-action`
release tag (e.g., via a pinned witness JSON committed under
`fixtures/` or via `github.event.repository.default_branch`'s
`refs/tags/v2.85.9^{commit}`) and asserts the comment and the SHA agree.
This converts the comment from a review aid into a tested invariant.

### OBS-008 (carried): `cargo_command_surfaces` proof scope does not cover `--locked` regression in historical surfaces

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Tampering / Elevation of Privilege |
| **File** | `xtask/tests/cargo_command_surfaces_w104.rs`, `policy/cargo-command-surfaces.toml` |
| **Status** | Not patched — intentional scope boundary |

**Description:** The `f3cfd24` adoption guard correctly classifies
`docs/examples` and `.factory/security/reports` as `historical` surfaces
("may preserve historical command output") and routes only the
`cargo_command_surfaces` scope through `ci/proof.toml`. The guard's
classification verdict is `NotProven` for these surfaces, which means a
future `--locked` regression in a preserved historical example will not
fail the new guard's CI lane. The `7f0d675` adoption guard for the
required gate has been added and bounds the canonical guidance; the
historical surfaces remain explicitly out of scope by policy.

**Why not a finding:**
- The policy file's `mode = "historical"` declaration is the explicit
  policy intent: historical surfaces preserve intentional command text
  (e.g., reproduce-the-issue recipes) and the guard's `NotProven` verdict
  is the contract for that scope.
- The canonical agent guidance (`AGENTS.md`, `agents/shared/repo.md`)
  IS routed through `cargo_command_surfaces` (`paths` list) AND through
  `agent_guidance_docs` AND through the new `required_gate_locked_commands`
  scope (`xtask/tests/required_gate_locked_commands_w108.rs`); a `--locked`
  regression on the canonical lanes fails all three proof scopes.
- The guard does not execute commands; it only classifies visible
  whitespace-delimited text. There is no execution surface added.
- Below the `medium` severity threshold; this is informational about
  the policy's intentional scope boundary.

**Recommended action (optional, future):** If maintainers want
historical-surface `--locked` regressions to fail CI, add a follow-up
scope (e.g. `cargo_command_surfaces_historical`) with an allowlist of
path globs whose historical surfaces still must conform to the locked
contract. This is a policy decision, not a security gap.

### OBS-009 (new): Mixed-pinning inventory for `actions/upload-artifact` is partial

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Spoofing / Tampering |
| **File** | `.github/workflows/ci.yml` |
| **Status** | Not patched — dependabot behavior |

**Description:** `e3323ff` updates the single SHA-pinned
`actions/upload-artifact@043fb46d1a93c77aae656e7c1c1c64a875d1fc6a0a # v7.0.1`
in `ci.yml` (line 439) to a new SHA. Other tag-pinned references to
`actions/upload-artifact@v7` across `ci.yml` are bumped to `@v7.0.1` but
remain tag-pinned. Dependabot treats the two pinning strategies as a
single group and updates them with a single PR, but it does not convert
tag-pinned references to SHA pins; that would require a separate, manual
sweep of the workflow to find each tag reference, look up its SHA, and
edit it.

**Why not a finding:**
- This is a Dependabot behavior, not a vulnerability. The single
  SHA-pinned location in `ci.yml` is updated correctly. The tag-pinned
  references are pinned to a major-version tag (`@v7`), so the runner
  still resolves to a specific upstream commit, not a mutable ref.
- Below the `medium` severity threshold; informational only. Carried
  alongside OBS-003.

**Recommended action (optional, future):** Convert the remaining
tag-pinned `actions/upload-artifact@v7.0.1` references in `ci.yml` to
SHA-pinned references with `# v7.0.1` comments. This is the same
recommendation as OBS-003.

## Standing Defenses Re-read in the Inspected Tree

The following defenses were re-read during this scan. Presence in the
inspected tree is recorded here; this is the same defense inventory that
prior weekly scans have re-verified.

| ID | Defense | Location | Verified |
|----|---------|----------|----------|
| D-01 | `unsafe_code = "forbid"` workspace lint | `Cargo.toml` | ✓ |
| D-02 | `unwrap_used`, `expect_used`, `panic`, `unreachable`, `dbg_macro`, `todo`, `unimplemented` lints denied | `Cargo.toml` | ✓ |
| D-03 | Git subprocess env isolation (`GIT_REPO_SHAPING_ENV`) | `crates/tokmd-git/src/command.rs`, `crates/tokmd/src/git_support.rs`, `crates/tokmd-scan/src/walk/git.rs` | ✓ |
| D-04 | Git ref validation (`env_base_ref_is_safe` + `--end-of-options`) | `crates/tokmd-git/src/refs.rs` | ✓ |
| D-05 | Bounded path canonicalization under root | `crates/tokmd-scan/src/path/bounded_path.rs` | ✓ |
| D-06 | FFI in-memory input path validation | `crates/tokmd-core/src/ffi/inputs.rs` (line: `MAX_IN_MEMORY_INPUT_PATH_BYTES = 4096`) | ✓ |
| D-07 | Strict JSON parsing with type validation | `crates/tokmd-core/src/ffi/parse.rs` | ✓ |
| D-08 | Per-family schema versioning (`SCHEMA_VERSION=2`, `COCKPIT_SCHEMA_VERSION=3`, `HANDOFF_SCHEMA_VERSION=5`, `CONTEXT_SCHEMA_VERSION=4`, `CONTEXT_BUNDLE_SCHEMA_VERSION=2`) | `crates/tokmd-types/src/lib.rs`, `cockpit.rs`, `context.rs` | ✓ |
| D-09 | Mixed action references: custom Droid action and typos installer are SHA-pinned; `ci.yml` also retains `actions/upload-artifact@v7.0.1` (single SHA-pinned reference at line 439 refreshed by `e3323ff`), `actions/setup-node@v7`, `Swatinem/rust-cache@v2`, and `taiki-e/install-action@v2` | `.github/workflows/droid*.yml`, `.github/workflows/ci.yml` | inspected mixed references |
| D-10 | Branch-protection settings for `main` are present (status checks required, no force-push, no deletions); live enforcement and per-PR execution were not independently proven by this scan (`gh api` returns HTTP 403 for the integration token) | `.github/settings.yml` | configured; live enforcement unverified |
| D-11 | `cargo-deny` advisory + license allowlist | `deny.toml` (`RUSTSEC-2020-0163` ignore for transitive `term_size` via `tokei`) | ✓ |
| D-12 | BLAKE3 redaction with extension allowlist | `crates/tokmd-format/src/redact/mod.rs`, `extensions.rs` | ✓ |
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
| D-23 | Author DAG import via true-merge commits (no force-push of publication history) | repository topology | not verifiable from this shallow clone |
| D-24 | `supply_chain` gate explicitly tolerates missing `cargo audit` binary by returning `Pending`, never `Pass` (per `pending_supply_chain_gate` constructor) | `crates/tokmd-cockpit/src/supply_chain.rs` | ✓ |
| D-25 | `Command::new("cargo")` and `Command::new("git")` invocations use `arg()` (not shell) and `current_dir` for path control, no `sh -c` / `bash -c` | `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd-cockpit/src/gates/contracts.rs`, `tokmd-git/src/command.rs`, `crates/tokmd-scan/src/walk/git.rs` | ✓ |
| D-26 | Typos lane install contract: SHA-pinned action, pinned tool version, `checksum: true`, `fallback: none`, plus a structural drift/fork rejection test | `.github/workflows/ci.yml::typos`, `xtask/tests/proof_plan_w92.rs::typos_install_contract_is_immutable_verified_and_fail_closed` | ✓ |
| D-27 | `cargo_command_surfaces` adoption guard: closed-world inventory in `policy/cargo-command-surfaces.toml` (`schema_version = 1`), deterministic tracked-file scanner in `xtask/tests/cargo_command_surfaces_w104.rs`, routed through `ci/proof.toml::cargo_command_surfaces` proof scope; scanner is text-only and never executes guidance | `policy/cargo-command-surfaces.toml`, `xtask/tests/cargo_command_surfaces_w104.rs`, `ci/proof.toml` | ✓ |
| D-28 | Canonical governed Cargo `build`, `check`, `test`, `clippy`, `run`, and `install` guidance uses `--locked`, with explicit "this source install is reproducible only to the committed lock" framing and tracked under `tokmd-swarm#604` / `depguard#21` / `depguard#22` / `depguard#24`; formatter aliases, xtask, and other ungoverned subcommands are outside this claim | `AGENTS.md`, `agents/shared/repo.md` | ✓ |
| D-29 (new) | Required-gate locked-command contract: every governed Cargo invocation in the `tokmd-rust-result` job body carries `--locked` (or `--frozen`); xtask gate task passes `--locked` to `check`, `clippy`, and `test --no-run`, with `cargo fmt` retained as the documented positive control exception; bounded by `xtask/tests/required_gate_locked_commands_w108.rs` (382 lines) and `xtask/tests/cargo_command_surfaces_w104.rs` (582 lines) | `.github/workflows/ci.yml::tokmd-rust-result`, `xtask/src/tasks/gate.rs`, `xtask/tests/required_gate_locked_commands_w108.rs`, `xtask/tests/cargo_command_surfaces_w104.rs` | ✓ |


### Scan Coverage Matrix

The coverage below applies to the five commits reviewed for context
(`6c5aae9`, `e3323ff`, `7f0d675`, `65eabd7`, `4241d83`) and the standing
defenses the original reviewer recorded as re-read. It does not establish
commit-level coverage of the six omitted adjacent commits listed in Scan
Metadata.

| Area | Files reviewed | Findings |
|------|----------------|----------|
| Required-gate locked-command contract | `.github/workflows/ci.yml::tokmd-rust-result`, `xtask/src/tasks/gate.rs`, `xtask/src/tasks/ci_gate_contract.rs`, `xtask/tests/required_gate_locked_commands_w108.rs`, `xtask/tests/cargo_command_surfaces_w104.rs`, `xtask/tests/affected_w91.rs` | 0 |
| Locked-gate adoption docs | `AGENTS.md`, `agents/shared/repo.md`, `docs/ci/default-pr-gate.md`, `docs/testing.md`, `policy/ci-lane-whitelist.toml`, `ci/proof.toml` | 0 |
| Dependabot GitHub Actions bump | `.github/workflows/*.yml` (17 files; SHA pin refresh at `ci.yml` line 439) | 0 |
| Dependabot Rust crates bump | `Cargo.lock` (no source or `Cargo.toml` changes) | 0 |
| Adjacent security report commits | `.factory/security/reports/security-report-2026-08-24.md`, `.factory/security/reports/security-report-2026-08-31.md` | 0 |
| Git subprocess isolation | `crates/tokmd-git/src/command.rs`, `crates/tokmd-git/src/refs.rs`, `crates/tokmd/src/git_support.rs`, `crates/tokmd-scan/src/walk/git.rs` | 0 |
| FFI inputs | `crates/tokmd-core/src/ffi/mod.rs`, `inputs.rs`, `parse.rs` | 0 |
| Path handling | `crates/tokmd-scan/src/path/bounded_path.rs`, `crates/tokmd-scan/src/exclude/mod.rs` | 0 |
| File content reads | `crates/tokmd-analysis/src/content/mod.rs` (limits), `crates/tokmd-io-port/src/` | 0 |
| Redaction / hashing | `crates/tokmd-format/src/redact/mod.rs`, `extensions.rs` | 0 |
| Subprocess audit/semver | `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd-cockpit/src/gates/contracts.rs` | 0 |
| GitHub workflows | `.github/workflows/*.yml` (29 files), `.github/settings.yml`, `action.yml` | 0 |
| Build / lint | `Cargo.toml`, `Cargo.lock`, `deny.toml`, `clippy.toml`, `.cargo/config.toml` | 0 |
| Githooks | `.githooks/pre-commit`, `.githooks/pre-push`, `.claude/hooks/format-rust.sh` | 0 |
| Web runner (browser) | `web/runner/main.js`, `worker.js`, `auth.js`, `messages.js`, `runtime.js`, `ingest.js` | 0 |
| Threat model | `.factory/threat-model/threat-model.md` | unchanged (within 90-day window) |

### Commit-level Analysis

The strict intended 2026-09-14 → 2026-09-21 window contains zero commits in
the observed checkout. The recorded adjacent manual review covers five
selected commits from 2026-09-07. The most recent reviewed commit is:

```
4241d8385abdb6fa8992b382c750f1fa74bde191
Author: Steven Zimmerman, CPA <15812269+EffortlessSteven@users.noreply.github.com>
Date:   Mon Sep 7 21:05:41 2026 -0400
Subject: docs(security): record August 31 manual scan with bounded coverage (#626)
```

- **Type:** Documentation-only commit. Body claims: "Historical weekly
  report committed to `.factory/security/reports/`."
- **Surface:** 1 file (`+782/0`).
- **Net code change:** Adds the 2026-08-31 weekly security report.
- **STRIDE analysis:** STRIDE-neutral provenance artifact.

The other four commits in the recorded adjacent review subset are summarized
below. The manual review assessed these named commits as adding no
new trust boundaries, new subprocess invocations, new secret/env surfaces,
new CLI flags, or new dependency surface (the `6c5aae9` Dependabot
commit only bumps existing `Cargo.lock` entries); all are STRIDE-positive
or STRIDE-neutral for their respective categories.

```
65eabd7967f3c967925ff35bad3f573a9edc3cac  2026-09-07  docs(security): record August 24 manual review and advisory scope (#623)
7f0d675d6b2b3009c5fc4bd0ccf26a5a9fd2e200  2026-09-07  ci(gate): run the required Cargo proof locked (#625)
e3323ff875b981b99b5cfc7440c1c5f6f642d830  2026-09-07  chore(deps): bump the github-actions group across 1 directory with 3 updates (#629)
6c5aae9e03e077c5d6f4ba362b1ba660847d9cc6  2026-09-07  chore(deps): bump the rust-minor-patch group across 1 directory with 14 updates (#628)
```

- **`7f0d675 ci(gate): run the required Cargo proof locked (#625)`** —
  `+473/-32` across `.github/workflows/ci.yml`, `AGENTS.md`,
  `agents/shared/repo.md`, `ci/proof.toml`,
  `docs/ci/default-pr-gate.md`, `docs/testing.md`,
  `policy/ci-lane-whitelist.toml`, `xtask/src/tasks/ci_gate_contract.rs`,
  `xtask/src/tasks/gate.rs`, `xtask/tests/affected_w91.rs`,
  `xtask/tests/required_gate_locked_commands_w108.rs` (new file, 382 lines).
  Adds `--locked` to the required `Tokmd Rust Result` gate's outer `xtask`
  launchers (`cargo --locked xtask gate --check`,
  `cargo --locked xtask proof-policy --check`) and to its build/test
  commands (`cargo test --locked --all-features`,
  `cargo test --locked -p xtask --all-features`). Adds matching `--locked`
  flags in `xtask/src/tasks/gate.rs` to the `check`, `clippy`, and
  `test --no-run` steps, with `cargo fmt` retained as the documented
  positive control exception (fmt resolves no dependencies and rejects
  the flag). Lands `xtask/tests/required_gate_locked_commands_w108.rs`
  (382 lines) as a focused contract test: it parses the
  `tokmd-rust-result` job body from `ci.yml`, asserts all required
  commands are present, asserts no `cargo xtask` launcher is unlocked
  anywhere in the job (including receipt/summary strings), and asserts
  that the xtask gate task's three dependency-resolving steps all pass
  `--locked` while the `fmt` step stays unlocked. It also includes
  positive and negative scanner controls (e.g.,
  `cargo test --locked` accepted, `cargo test --all-features` rejected,
  `cargo xtask gate --check --locked` rejected because `--locked` after
  the alias reaches the xtask binary not Cargo). Updates the `affected_w91`
  fixture and `ci/proof.toml` to route the new contract test. STRIDE-
  positive Tampering / Elevation of Privilege reduction: missing or
  stale lock state can no longer be repaired silently before the inner
  checks run, and the locked claim is now bound to executable CI
  receipts instead of being guidance-only.

- **`e3323ff chore(deps): bump the github-actions group across 1 directory
  with 3 updates (#629)`** — `+29/-29` across 17 files in
  `.github/workflows/`. Dependabot bumps:
  - `actions/upload-artifact` from 4.6.2 to 7.0.1. Single SHA-pinned
    reference at `ci.yml` line 439 is updated to the new SHA
    `043fb46d1a93c77aae656e7c1c1c64a875d1fc6a0a # v7.0.1`. Other
    tag-pinned references to `@v7` in `ci.yml` are refreshed to
    `@v7.0.1`.
  - `docker/setup-qemu-action` from 4.2.0 to 4.3.0. Already SHA-pinned
    at `1f40c72289eff860ee54a304f1438e3cff362e0a # v4` in
    `release-candidate.yml`. No change to the pinned reference.
  - `cachix/install-nix-action` from v6 to v31. Already SHA-pinned at
    `13d8dd58da0234aa297dedd986986ccb8e7f3e24 # v31` in
    `release-consumer-smoke.yml`. No change to the pinned reference.
  STRIDE-neutral: the previously SHA-pinned reference keeps its pinning
  contract, and the mixed-pinning inventory (OBS-003) is preserved.

- **`6c5aae9 chore(deps): bump the rust-minor-patch group across 1 directory
  with 14 updates (#628)`** — `Cargo.lock` only (binary update). Dependabot
  bumps thirteen Rust crates to their minor/patch versions. No `Cargo.toml`,
  no source code, no policy changes. STRIDE-neutral dependency maintenance.

- **`65eabd7 docs(security): record August 24 manual review and advisory
  scope (#623)`** — `+311/0` to
  `.factory/security/reports/security-report-2026-08-24.md`. Historical
  weekly report committed to `.factory/security/reports/`. STRIDE-neutral
  provenance artifact.

**Security-critical files re-read in place at `4241d83`:**
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
- `xtask/tests/cargo_command_surfaces_w104.rs` — 582 lines; deterministic
  tracked-file scanner; recognizes short global cargo options; treats
  unknown pre-command options as `NotProven`; stops scanning after
  `--`; verifies lock-preservation only for the parsed cargo-prefix
  token span.
- `xtask/tests/required_gate_locked_commands_w108.rs` — 382 lines;
  required-gate locked-command contract for the `tokmd-rust-result`
  job. Bounded parser extracts the job body without leaking into
  adjacent platform lanes. The scanner covers short global cargo
  options, value-taking options (`-C`, `-Z`, `--target`,
  `--target-dir`, `--manifest-path`, etc.), the alias case (`xtask`
  expands to `run -p xtask --`), and bare-word option values that
  must not pose as the subcommand. Positive and negative controls are
  inline in the test.
- `xtask/src/tasks/gate.rs` — `STEPS` table now passes `--locked` to
  `check`, `clippy`, and `test --no-run`. The `fmt` step is the
  documented positive control exception.
- `xtask/src/tasks/ci_gate_contract.rs` — required marker set was
  refreshed by `7f0d675` to accept both `cargo xtask gate --check` and
  `cargo --locked xtask gate --check` (matched without the
  `cargo ` prefix). No forbidden-marker drift.
- `xtask/tests/affected_w91.rs` — updated affected-scope fixture for
  the new `required_gate_locked_commands` scope.
- `ci/proof.toml` — routes the new `required_gate_locked_commands`
  proof scope (in addition to existing `cargo_command_surfaces`).
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
- `.github/workflows/ci.yml::tokmd-rust-result` — every governed Cargo
  invocation now carries `--locked`; receipt and summary strings
  agree with the executable commands (defense D-29).
- `.github/workflows/release-candidate.yml` —
  `docker/setup-qemu-action@1f40c72289eff860ee54a304f1438e3cff362e0a
  # v4` (SHA-pinned).
- `.github/workflows/release-consumer-smoke.yml` —
  `cachix/install-nix-action@13d8dd58da0234aa297dedd986986ccb8e7f3e24
  # v31` (SHA-pinned).
- At `4241d83`, `.github/settings.yml` — only `Tokmd Rust Result` is a
  required status context for `main`; `required_conversation_resolution:
  true`; `allow_force_pushes: false`; `allow_deletions: false`. This records the
  inspected historical file; live enforcement at scan time and current
  branch-protection state were not independently proven (HTTP 403).
- `deny.toml` — `RUSTSEC-2020-0163` ignore for transitive `term_size`
  unchanged; license allowlist unchanged.
- `AGENTS.md` and `agents/shared/repo.md` — documented governed Cargo
  `build`, `check`, `test`, `clippy`, `run`, and `install` commands pass
  `--locked`; source-install framing explicit. This does not cover formatter
  aliases, xtask commands, or other ungoverned subcommands.
- `docs/ci/default-pr-gate.md`, `docs/testing.md`,
  `policy/ci-lane-whitelist.toml` — refreshed by `7f0d675` to record
  the locked-command contract and its adoption boundary.

**The manual review recorded no security findings at or above the `medium`
threshold for the five named reviewed commits and recorded standing-defense
inspection. No finding tally is established for the six omitted adjacent
commits. Based on the reviewed source commits and the new
`required_gate_locked_commands` adoption guard, the change set is
STRIDE-positive across Tampering, Information Disclosure (incidentally,
via tighter fail-closed contracts on locked commands), and Elevation of
Privilege.**

### Patches Generated

No patches were generated this scan (no findings at or above `medium`).

### Next Scan

The next scheduled security scan runs Monday, 2026-09-28 via
`.github/workflows/droid-security-scan.yml` (cron `0 8 * * 1`).

## Appendix

### Threat Model

- **Status:** Within freshness window.
- **Location:** `.factory/threat-model/threat-model.md`
- **Last Modified:** 2026-08-04 (commit `b4ff2c9 Merge publication
  import for release alias promotion`); file content timestamp
  2026-08-02 (48 days ago — well within 90-day window)
- **Methodology:** STRIDE
- **Next review:** 2026-11-01 (90-day cadence) or upon architecture change
- **No regeneration this scan** — the file is within its normal freshness
  window. The next regeneration should fold in the typos-lane SHA-pinning
  update from `24d5a53` (carried OBS-003), the `#604` locked-Cargo-
  command adoption guard from `f3cfd24` + `c2f77f0` + `7d192f0`
  (already covered by prior scan), and the new `7f0d675`
  required-gate locked-command adoption guard from this scan
  (which extends the prior guard from guidance-only to workflow+gate-
  task+test binding).

### Scan Metadata

- **Strict window:** 2026-09-14 → 2026-09-21 — zero commits in the
  observed checkout.
- **Adjacent review subset:** 2026-09-07 — five named commits
  (`6c5aae9`, `e3323ff`, `7f0d675`, `65eabd7`, `4241d83`) recorded as
  reviewed for context.
- **Known commit reviewed (most recent on working branch):**
  `4241d8385abdb6fa8992b382c750f1fa74bde191 docs(security): record
  August 31 manual scan with bounded coverage (#626)`, 2026-09-07.
- **Window completeness:** Not independently proven. `git log --since="7
  days ago" --pretty=format:"%H %s"` returned zero commits in the
  observed checkout, but the recorded `git fetch --depth=50 origin main`
  is bounded and cannot establish full-history completeness.
- **Branch-protection live read:** `gh api
  repos/EffortlessMetrics/tokmd-swarm/branches/main/protection` returns
  HTTP 403 for the integration token used by this scan. Live enforcement
  state was not independently proven; the checked-in
  `.github/settings.yml` configuration is recorded above.
- **Files changed by the five named adjacent commits:** 28 unique paths
  across `.github/workflows/bindings-parity.yml`,
  `.github/workflows/ci-policy.yml`, `.github/workflows/ci.yml`,
  `.github/workflows/clippy-exceptions-policy.yml`,
  `.github/workflows/cockpit.yml`, `.github/workflows/coverage.yml`,
  `.github/workflows/fuzz.yml`, `.github/workflows/ghcr-container-smoke.yml`,
  `.github/workflows/mutants.yml`, `.github/workflows/no-panic-policy.yml`,
  `.github/workflows/pr-plan.yml`, `.github/workflows/proof-executor.yml`,
  `.github/workflows/proof-observation-collection.yml`,
  `.github/workflows/release-candidate.yml`,
  `.github/workflows/release-consumer-smoke.yml`,
  `.github/workflows/release.yml`, `.github/workflows/ripr.yml`,
  `.factory/security/reports/security-report-2026-08-24.md`,
  `.factory/security/reports/security-report-2026-08-31.md`,
  `AGENTS.md`, `agents/shared/repo.md`, `ci/proof.toml`,
  `docs/ci/default-pr-gate.md`, `docs/testing.md`,
  `policy/ci-lane-whitelist.toml`, `Cargo.lock`,
  `xtask/src/tasks/ci_gate_contract.rs`, `xtask/src/tasks/gate.rs`,
  `xtask/tests/affected_w91.rs`,
  `xtask/tests/required_gate_locked_commands_w108.rs`.
- **Scan Duration:** ~5m (focused diff review + defense re-verification)
- **Skills Used:** commit-security-scan (manual, STRIDE),
  vulnerability-validation (manual, exploitability assessment), security-review
  (manual, defense confirmation)
- **Manual Reviewers:** 1 (Droid scheduled security scan)
- **False Positive Filter:** applied — see Observations above

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
