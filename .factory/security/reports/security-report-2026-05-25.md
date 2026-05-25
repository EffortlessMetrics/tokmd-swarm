# Security Scan Report

**Generated:** 2026-05-25
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium

## Executive Summary

| Severity | Count | Auto-fixed | Manual Required |
|----------|-------|------------|-----------------|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 0 | 0 | 0 |
| LOW | 0 | 0 | 0 |

**Total Findings:** 0
**Auto-fixed:** 0
**Manual Review Required:** 0

## Threat Model

- **Version:** 2026-05-25 (newly generated)
- **Location:** `.factory/threat-model.md`

### Threat Model Summary

The threat model identified the following attack surfaces and mitigations:

**Spoofing Threats (2):**
- FFI boundary impersonation via malicious JSON arguments
- Output format spoofing through fake receipts

**Tampering Threats (3):**
- `.tokeignore` configuration file tampering
- Git history manipulation affecting cockpit/diff analysis
- Output file tampering after generation

**Repudiation Threats (2):**
- Audit trail gaps (no cryptographic proof of scan timing)
- Receipt provenance issues (no binding to repo git hash)

**Information Disclosure Threats (5):**
- Path disclosure through unredacted output
- Sensitive file content (.env, keys) in scans
- Secrets in command-line arguments/metadata
- FFI data leakage via error messages
- WASM browser memory exposure

**Denial of Service Threats (5):**
- Path traversal via symbolic links
- Excessive resource consumption from large repos
- Git log DoS via massive commit history
- Malformed JSON/Input DoS at FFI boundary
- Circular symlink traps

**Elevation of Privilege Threats (5):**
- FFI escape to host system (mitigated by `#![forbid(unsafe_code)]`)
- Path injection for arbitrary file read
- Shell command injection via git refs
- Configuration injection via .tokeignore
- WASM sandboxing escape

**Key Mitigations in Place:**
- `#![forbid(unsafe_code)]` workspace-wide provides strong EoP protection
- Strict FFI safety patterns in Python/Node/WASM bindings (GIL release, never-panic guarantee)
- Path normalization and deterministic output are solid defensive measures
- Strict allowlist validation in workflow_dispatch inputs

## Scanned Files (Last 7 Days)

### GitHub Actions Workflows (21 files)
- `.github/workflows/badge-endpoints.yml`
- `.github/workflows/ci-policy.yml`
- `.github/workflows/ci.yml`
- `.github/workflows/cockpit.yml`
- `.github/workflows/coverage.yml`
- `.github/workflows/droid-review.yml`
- `.github/workflows/droid-security-scan.yml`
- `.github/workflows/droid.yml`
- `.github/workflows/em-routed-rust-small.yml`
- `.github/workflows/fuzz.yml`
- `.github/workflows/mutants.yml`
- `.github/workflows/nix-full.yml`
- `.github/workflows/nix-macos.yml`
- `.github/workflows/no-panic-policy.yml`
- `.github/workflows/pr-plan.yml`
- `.github/workflows/proof-executor.yml`
- `.github/workflows/proof-observation-collection.yml`
- `.github/workflows/release.yml`
- `.github/workflows/ripr.yml`
- `.github/workflows/sync-labels.yml`
- `.github/workflows/test-action.yml`

### CI Configuration
- `.cargo/config.toml`
- `.cargo/mutants.toml`

### Git Hooks
- `.githooks/pre-commit`
- `.githooks/pre-push`

### Agent Configuration
- `.claude/agents/author.yaml`
- `.claude/agents/ci.yaml`
- `.claude/agents/compat.yaml`
- `.claude/agents/critic.yaml`
- `.claude/agents/deps.yaml`
- `.claude/agents/docs.yaml`

## Security Analysis

### STRIDE Analysis Results

| STRIDE Category | Findings | Status |
|-----------------|----------|--------|
| Spoofing | 0 | Pass |
| Tampering | 0 | Pass |
| Repudiation | 0 | Pass |
| Information Disclosure | 0 | Pass |
| Denial of Service | 0 | Pass |
| Elevation of Privilege | 0 | Pass |

### Key Security Controls Verified

1. **Secrets Management:** All workflows use `${{ secrets.X }}` syntax for secret access
2. **Command Injection Prevention:** Arguments are passed as explicit parameters, not shell string interpolation
3. **Input Validation:** `workflow_dispatch` inputs are validated against explicit allowlists
4. **Safe Script Execution:** Git hooks use `set -euo pipefail` for safe shell execution
5. **Unsafe Code Prevention:** `#![forbid(unsafe_code)]` workspace-wide

## Critical Findings

None.

## High Findings

None.

## Medium Findings

None.

## Low Findings

None.

## Appendix

### Scan Metadata
- **Commits Scanned:** 1 (575e85a)
- **Files Changed:** 60+ configuration and CI files
- **Scan Duration:** ~5 minutes
- **Skills Used:** threat-model-generation, commit-security-scan, vulnerability-validation

### References
- [CWE Database](https://cwe.mitre.org/)
- [STRIDE Threat Model](https://docs.microsoft.com/en-us/azure/security/develop/threat-modeling-tool-threats)
- [tokmd-swarm Threat Model](./.factory/threat-model.md)
