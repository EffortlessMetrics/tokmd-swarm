# Security Scan Report

**Generated:** 2026-08-17
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium
**Intended Window:** 2026-08-10 → 2026-08-17
**Observed Scope:** Checked-out commit `24d5a53`; `git log --since="7 days ago"`
reported no other commits in this checkout for the intended window. The scan
does not independently prove window completeness: the appendix records only a
bounded `git fetch --depth=20`, so the zero-finding tally is limited to the
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
`24d5a53 ci(typos): harden release asset install (#594)`, dated 2026-08-14.
This is itself a defensive STRIDE-positive CI hardening. The commit replaces
the prior mutable `crate-ci/typos@v1` install (which fetched a tool binary
from `github.com` releases with no checksum verification and a silent
fallback to `cargo-binstall` / `cargo install`) with a SHA-pinned
`taiki-e/install-action@91ddec75689c4c78665b598d188dc821c5a43e5c` (tagged
`# v2.85.9`) running with `tool: typos@1.49.0`, `checksum: true`, and
`fallback: none`, then runs `typos` as a separate step. The same commit
back-fills a contract test in `xtask/tests/proof_plan_w92.rs` that
structurally rejects drift: floating version refs, mutable `${{ … }}`
expressions (inputs, secrets, tokens, PR titles), disabled checksums,
cargo-binstall fallbacks, step reordering, comment-out drift, duplicated
install steps, write permissions, injected `env`, `permissions`, `if`,
`continue-on-error`, unmodeled step properties, and forged-script
mutations that try to trick the parser by adding look-alike text outside
the recognized step shape.

This is a **STRIDE-positive** change across the relevant categories:

| STRIDE | Direction | Why |
|--------|-----------|-----|
| Spoofing | Improved | Action pinned to `91ddec75689c4c78665b598d188dc821c5a43e5c`; tool pinned to `typos@1.49.0`. No mutable `@v1` floating tag remains for this lane. |
| Tampering | Improved | `checksum: true` enables SHA-256 verification of the downloaded `typos` binary against the manifest embedded in the pinned install-action. `fallback: none` removes the silent downgrade path to `cargo-binstall` / `cargo install`. The contract test additionally rejects any drift that disables these settings. |
| Repudiation | N/A | Out of scope for a spelling job. |
| Information Disclosure | N/A | Out of scope for a spelling job. |
| Denial of Service | Improved (incidentally) | The commit body documents that 6 of 38 (15.8%) recently observed Typos jobs failed at the download step (`No data received`, terminal 503, exit 8). The new installer has bounded retries, reducing that bootstrap failure mode. |
| Elevation of Privilege | Improved | Removes a mutable upstream supply-chain entry point; the contract test rejects any reintroduction of mutable values or write permissions. |

The workspace-wide standing defenses were re-read in place and remain
present at the checked-out commit (see Standing Defenses table below). No
defenses were observed to have regressed.

The threat model at `.factory/threat-model/threat-model.md` is dated
2026-08-02 — 15 days old — still well within the 90-day regeneration
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

The manual comparison recorded one new low-severity observation for the
`#594` typos install hardening (OBS-007 below), alongside the carried
`2026-08-10` observations. The hardening also *reduces* one carried observation
(OBS-003 below): the typos lane is now SHA-pinned where it was previously
tag-pinned (`crate-ci/typos@v1`), narrowing the residual mixed-pinning
gap. The remaining tag-pinned first-party actions are unchanged.

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
Out of scope per `SECURITY.md`. No change in the typos install hardening.

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
Other workflows pin third-party actions by tag. `actions/checkout`
is SHA-pinned at `3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1` across
the workflows that use it.

This week's `24d5a53` commit narrows the gap for the typos lane: the prior
mutable `crate-ci/typos@v1` reference in `.github/workflows/ci.yml::typos`
was replaced with a SHA-pinned
`taiki-e/install-action@91ddec75689c4c78665b598d188dc821c5a43e5c # v2.85.9`
plus an exact `tool: typos@1.49.0` and structural test coverage to
prevent regression.

**Why not a finding:**
- Tag-pinned first-party actions (`actions/*`) are a well-accepted practice
  with low residual risk; GitHub's own recommended baseline.
- The custom Droid action — the highest-privilege third-party surface — IS
  SHA-pinned.
- After `24d5a53`, the typos lane is also SHA-pinned with structural test
  enforcement; remaining tag-pinned examples include first-party actions and
  third-party tool installers such as `taiki-e/install-action@v2` used by
  coverage, CI, proof-executor, and release workflows.
- Below the `medium` severity threshold for this scan; flagged for the next
  threat-model refresh (target: 2026-11-01 or earlier if scope changes).

**Recommended action (optional, future):** Either update the threat model
to reflect the actual mixed-pinning policy with the typos lane listed as
the SHA-pinned exception, or convert all third-party tool-installer
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

**Description:** At the checked-out commit `24d5a53`, `.github/settings.yml`
configures `required_approving_review_count: 0` and
`require_code_owner_reviews: false` for `main`. The same historical file
declares `Tokmd Rust Result` and `Codex Review Gate` as status contexts and
describes native human approval and CODEOWNERS review as intentionally
absent (per the in-line comment recorded in the inspected file).

**Why not a finding:**
- The checked-in policy is narrow and explicit: `enforce_admins: false`,
  `allow_force_pushes: false`, `allow_deletions: false`, and two declared
  status contexts.
- Live enforcement and per-PR execution of those contexts were not
  independently proven by this scan.
- The checked-out tree's `.github/settings.yml` comment records this as
  a deliberate operational choice ("Codex is the exact-head reviewer for this
  single-maintainer workflow"); the threat model is stale on this point and
  its contradictory approval text is explicitly pending refresh.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional, future):** When the maintainer count
grows, increase `required_approving_review_count` and re-enable
`require_code_owner_reviews`.


### OBS-007 (new): `taiki-e/install-action` SHA must match `# v2.85.9` comment

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
| D-08 | Per-family schema versioning (`SCHEMA_VERSION=2`, `COCKPIT_SCHEMA_VERSION=3`, `HANDOFF_SCHEMA_VERSION=5`, `CONTEXT_SCHEMA_VERSION=4`, `CONTEXT_BUNDLE_SCHEMA_VERSION=2`) | `crates/tokmd-types/src/` | ✓ |
| D-09 | SHA-pinned Droid-related actions; tag-pinned first-party actions; **typos lane SHA-pinned after `24d5a53`** | `.github/workflows/droid*.yml` (SHA), `ci.yml::typos` (SHA after `24d5a53`) | ✓ |
| D-10 | Branch-protection settings for `main` are present (status checks required, no force-push, no deletions); live enforcement and per-PR execution were not independently proven by this scan | `.github/settings.yml` | configured; live enforcement unverified |
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
| D-23 | Author DAG import via true-merge commits (no force-push of publication history) | repository topology | not verifiable from this shallow clone |
| D-24 | `supply_chain` gate explicitly tolerates missing `cargo audit` binary by returning `Pending`, never `Pass` (per `pending_supply_chain_gate` constructor) | `crates/tokmd-cockpit/src/supply_chain.rs` | ✓ |
| D-25 | `Command::new("cargo")` and `Command::new("git")` invocations use `arg()` (not shell) and `current_dir` for path control, no `sh -c` / `bash -c` | `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd-cockpit/src/gates/contracts.rs`, `crates/tokmd-git/src/command.rs`, `crates/tokmd-scan/src/walk/git.rs` | ✓ |
| D-26 (new) | Typos lane install contract: SHA-pinned action, pinned tool version, `checksum: true`, `fallback: none`, plus a structural drift/fork rejection test | `.github/workflows/ci.yml::typos`, `xtask/tests/proof_plan_w92.rs::typos_install_contract_is_immutable_verified_and_fail_closed` | ✓ |


## Appendix

### Threat Model

- **Status:** Within freshness window. The OBS-003 mixed-pinning note
  remains a known pending refresh item; the typos lane hardening in this
  scan's commit narrows the gap without closing it.
- **Location:** `.factory/threat-model/threat-model.md`
- **Last Modified:** 2026-08-02 (15 days ago — well within 90-day window)
- **Methodology:** STRIDE
- **Next review:** 2026-11-01 (90-day cadence) or upon architecture change
- **No regeneration this scan** — the file is within its normal freshness
  window. The next regeneration should fold in the typos-lane SHA-pinning
  update from `24d5a53`.

### Scan Metadata

- **Known commit reviewed:** `24d5a53fe360383693ad63a814b2a16606753f84 ci(typos):
  harden release asset install (#594)`, 2026-08-14, GPG-signed
- **Window completeness:** Not independently proven. `git log --since="7 days ago"
  --pretty=format:"%H %s"` returned exactly one commit (`24d5a53`) in the
  observed checkout, but the recorded `git fetch --depth=20 origin main` is
  bounded and cannot establish full-history completeness.
- **Files in scope:** 4 (the diff against the parent is +443/-3, approximately
  97% tests by changed-line composition), scoped to
  `.github/workflows/ci.yml`, `docs/ci/inventory.md`,
  `policy/ci-lane-whitelist.toml`, `xtask/tests/proof_plan_w92.rs`). The
  full surface was previously reviewed under the `2026-06-29` true-merge
  baseline; this scan re-verified all security-critical modules in place.
- **Scan Duration:** ~5m (focused diff review + defense re-verification)
- **Skills Used:** commit-security-scan (manual, STRIDE),
  vulnerability-validation (manual, exploitability assessment), security-review
  (manual, defense confirmation)
- **Manual Reviewers:** 1 (Droid scheduled security scan)
- **False Positive Filter:** applied — see Observations above

### Scan Coverage Matrix

The coverage below applies to the `24d5a53` typos install hardening and the
re-verified standing defenses.

| Area | Files reviewed | Findings |
|------|----------------|----------|
| CI workflow typos lane hardening | `.github/workflows/ci.yml::typos` | 0 |
| CI policy / lane whitelist update | `policy/ci-lane-whitelist.toml::typos` | 0 |
| CI inventory docs update | `docs/ci/inventory.md` | 0 |
| Drift/fork rejection test | `xtask/tests/proof_plan_w92.rs::typos_install_contract_is_immutable_verified_and_fail_closed` | 0 |
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

### Commit-level Analysis

The only commit reviewed for the intended 2026-08-10 → 2026-08-17 window was:

```
24d5a53fe360383693ad63a814b2a16606753f84
Author: Steven Zimmerman, CPA <15812269+EffortlessSteven@users.noreply.github.com>
Date:   Fri Aug 14 17:00:25 2026 -0400
Subject: ci(typos): harden release asset install (#594)
```

- **Type:** GPG-signed single-commit CI workflow hardening.
- **Surface:** 4 files (`+443/-3`, approximately 97% tests by changed-line
  composition).
- **Net code change in the commit body:**
  - `.github/workflows/ci.yml::typos`: replaces
    `- uses: crate-ci/typos@v1` with a SHA-pinned
    `taiki-e/install-action@91ddec75689c4c78665b598d188dc821c5a43e5c # v2.85.9`
    running with `tool: typos@1.49.0`, `checksum: true`, `fallback: none`,
    followed by a separate `run: typos` step. The `actions/checkout`
    SHA pin (`3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1`) and
    `persist-credentials: false` are preserved.
  - `docs/ci/inventory.md`: row description for the typos lane updated to
    "Pinned checksum-verified typos 1.49.0 install with bounded retries
    and no fallback, then `typos`."
  - `policy/ci-lane-whitelist.toml`: `proof_obligation` updated to
    "Install checksum-verified typos 1.49.0 with the pinned
    bounded-retry, fail-closed installer, then run typos." All other
    fields (tier, owner, evidence, allowed_triggers) unchanged.
  - `xtask/tests/proof_plan_w92.rs`: new test
    `typos_install_contract_is_immutable_verified_and_fail_closed`
    parses the typos job structure and asserts the contract, then mutates
    the YAML to assert the contract rejects:
    - Floating action reference (`taiki-e/install-action@v2`).
    - Floating tool version (`tool: typos@1.49`).
    - Disabled checksum (`checksum: false`).
    - Enabled fallback (`fallback: cargo-binstall`).
    - Altered run command (`run: typos --version`).
    - Mutable tool values (`${{ inputs.typos_version }}`,
      `${{ github.event.pull_request.title }}`, `${{ github.token }}`).
    - Mutable fallback values (`${{ secrets.FALLBACK }}`).
    - Omitted `checksum: true` or `fallback: none` lines.
    - Comment-out drift on `checksum: true`.
    - Duplicated install block.
    - Reordered run-then-install.
    - Injected extra step (`run: echo injected`).
    - Forged scripts that embed look-alike install text outside the
      recognized step shape.
    - Injected job-level `env:` or `permissions: write-all`.
    - Workflow-level `contents: write`.
    - Injected step-level `env: TOKEN: ${{ github.token }}`.
    - Injected root-level `TOKEN: ${{ secrets.TOKEN }}`.
    - Job-level `if: false` or `continue-on-error: true`.
    - Step-level `continue-on-error` or unmodeled step properties such
      as `shell: bash -c 'echo injected'`.
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
  - `.github/workflows/ci.yml::typos` — SHA-pinned installer
    (`taiki-e/install-action@91ddec75689c4c78665b598d188dc821c5a43e5c
    # v2.85.9`) with `checksum: true` and `fallback: none`, then
    `run: typos`. Enforced by
    `xtask/tests/proof_plan_w92.rs::typos_install_contract_is_immutable_verified_and_fail_closed`.
  - At `24d5a53`, `.github/settings.yml` — `Tokmd Rust Result` and
    `Codex Review Gate` declared as status contexts for `main`;
    `allow_force_pushes: false`; `allow_deletions: false`. This records the
    inspected historical file; live enforcement at scan time and current
    branch-protection state were not independently proven.
  - `deny.toml` — `RUSTSEC-2020-0163` ignore for transitive `term_size`
    unchanged; license allowlist unchanged.

**The manual review recorded no security findings at or above the `medium`
threshold for `24d5a53`. The change is itself a STRIDE-positive hardening
across Spoofing, Tampering, Denial of Service (incidentally), and Elevation
of Privilege.**

### Patches Generated

No patches were generated this scan (no findings at or above `medium`).

### Next Scan

The next scheduled security scan runs Monday, 2026-08-24 via
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
  `.factory/security/reports/security-report-2026-08-10.md`
