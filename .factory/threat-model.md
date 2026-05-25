# STRIDE Threat Model: tokmd

**Document Version:** 1.0  
**Date:** 2026-05-25  
**Tool:** tokmd v1.11.1 - Code inventory and analytics CLI/library  
**Author:** Security Review

---

## Executive Summary

tokmd is a Rust CLI tool and library that wraps the tokei library to generate code inventory receipts. It scans repositories, produces summaries in multiple formats (Markdown, TSV, JSON, JSONL, CSV), and provides analysis features including PR cockpit metrics and policy gates. This threat model covers the full attack surface including CLI interface, file system access, Git integration, FFI layer, output formats, and configuration files.

**Key Security Characteristics:**
- `#![forbid(unsafe_code)]` enforced throughout the codebase
- Schema versions for all receipt types to track changes
- BTreeMap for deterministic output ordering
- Path normalization to forward slashes
- GIL release during long operations in Python bindings
- Strict FFI safety invariants documented in bindings

---

## Threat Categories Overview

| Category | Description | Attack Surface |
|----------|-------------|----------------|
| **S**poofing | Impersonating someone or something else | FFI boundaries, output formats |
| **T**ampering | Modifying data or code | Configuration files, git integration |
| **R**epudiation | Denying having performed an action | Audit trails, receipts |
| **I**nformation Disclosure | Exposing sensitive information | File system access, output redaction |
| **D**enial of Service | Making the system unavailable | Input parsing, resource consumption |
| **E**levation of Privilege | Gaining capabilities without authorization | FFI layer, binding interfaces |

---

## 1. Spoofing Threats

### 1.1 FFI Boundary Impersonation

**Threat:** An attacker could craft malicious JSON arguments to the FFI layer to impersonate legitimate tokmd operations or bypass validation.

**Affected Components:**
- `tokmd-core/src/ffi/mod.rs` - JSON entrypoint `run_json(mode, args_json)`
- `tokmd-node/src/lib.rs` - Node.js `run_json()` and `run()` functions
- `tokmd-python/src/lib.rs` - Python `run_json()` and `run()` functions
- `tokmd-wasm/src/lib.rs` - WASM `run_json()` and `run()` functions

**Attack Scenario:**
```json
// Attacker sends malformed JSON to extract internal data structures
{
  "mode": "lang",
  "paths": [".env"],
  "files": true
}
```

**Mitigations in Place:**
- JSON validation at FFI boundary (`parse.rs`)
- Envelope contract with `ok`/`data`/`error` structure
- Error codes for invalid JSON, unknown modes
- WASM validates analyze preset restrictions

**Residual Risk:** MEDIUM - Complex JSON parsing at FFI boundary could have bypass paths

### 1.2 Output Format Spoofing

**Threat:** Attacker could craft receipt files that appear to come from a legitimate tokmd scan but contain falsified data.

**Attack Scenario:**
- Attacker creates a fake JSON receipt with modified line counts
- Downstream systems trust the receipt without re-verification

**Mitigations in Place:**
- Receipt includes `schema_version` and `generated_at_ms` timestamp
- Tool metadata (`name`, `version`) in envelope
- BLAKE3-based content hashing for redaction integrity

**Recommendations:**
- Add digital signature capability for receipts
- Implement receipt verification mode in CLI

---

## 2. Tampering Threats

### 2.1 Configuration File Tampering (.tokeignore)

**Threat:** Modification of `.tokeignore` or `tokei.toml` to exclude sensitive directories from scans, hiding code from inventory.

**Affected Components:**
- `crates/tokmd-scan/src/tokeignore/mod.rs` - Template generation and writing
- `crates/tokmd-settings/src/scan.rs` - Scan options and config handling

**Attack Scenario:**
1. Attacker modifies `.tokeignore` to exclude `secrets/` directory
2. tokmd scan doesn't report code in that directory
3. Audit/review process missing critical code

**Mitigations in Place:**
- `check-ignore` command explains why files are being ignored
- TOML config validation before use
- Explicit `--no-ignore` and `--no-ignore-parent` flags

**Residual Risk:** MEDIUM - User could unknowingly exclude sensitive areas

### 2.2 Git History Tampering

**Threat:** Modification of git history could affect cockpit/diff analysis results.

**Affected Components:**
- `crates/tokmd-git/src/lib.rs` - Git history collection via `git log`
- `crates/tokmd-cockpit/src/` - PR metrics and evidence gates
- `crates/tokmd-analysis/` - Git hotspot analysis

**Attack Scenario:**
```
# Attacker rewrites history to hide malicious commits
git filter-branch --tree-filter 'rm -rf secrets/malware' HEAD
```

**Mitigations in Place:**
- Uses shell `git` command (not git2 crate) - relies on git integrity
- Two-dot vs three-dot range syntax documented and enforced
- `git_available()` check before operations

**Recommendations:**
- Add integrity hash of git objects to cockpit receipts
- Warn when history appears rewritten (non-linear history detection)

### 2.3 Output File Tampering

**Threat:** Attacker modifies scan output files after generation.

**Affected Components:**
- `crates/tokmd-format/src/` - Output file writing
- `crates/tokmd-gate/` - Policy evaluation on receipts

**Attack Scenario:**
- Attacker modifies `receipt.json` to change line counts
- Policy gate evaluation uses tampered data

**Mitigations in Place:**
- Schema version tracking for receipt format changes
- Deterministic output (BTreeMap ordering) enables comparison
- JSON schema validation (`docs/schema.json`)

**Recommendations:**
- Add content integrity hash to receipts
- Implement signed receipts option

---

## 3. Repudiation Threats

### 3.1 Audit Trail Gaps

**Threat:** No immutable record of who ran tokmd and when, making it difficult to prove when a scan occurred.

**Affected Components:**
- All CLI commands and library workflows
- Receipt metadata (`generated_at_ms`, `tool`)

**Attack Scenario:**
- Organization claims a scan was run earlier/later than actual
- No cryptographic proof of scan timestamp

**Mitigations in Place:**
- `generated_at_ms` timestamp in all receipts
- Tool metadata including version
- Deterministic output for reproducible receipts

**Residual Risk:** MEDIUM - Timestamps can be faked, no external timestamping

### 3.2 Receipt Provenance

**Threat:** Difficulty proving a receipt came from a specific repository at a specific commit.

**Affected Components:**
- `crates/tokmd-cockpit/` - PR cockpit review packets
- `crates/tokmd-format/` - Output rendering

**Attack Scenario:**
- Receipt is claimed to be from old commit (clean) but was generated from modified code
- No binding between receipt and git commit

**Mitigations in Place:**
- Cockpit includes `base` and `head` refs in evidence
- Receipt includes scan paths and settings
- File-level receipt includes path information

**Residual Risk:** MEDIUM - Receipt doesn't include repo git hash

---

## 4. Information Disclosure Threats

### 4.1 Path Disclosure Through Output

**Threat:** tokmd output could expose internal path structure, file names, or directory layout that should remain confidential.

**Affected Components:**
- All scan/receipt outputs (JSON, JSONL, CSV, Markdown)
- `crates/tokmd-format/src/redact/` - Path redaction module

**Attack Scenario:**
```
# Attacker learns about internal structure from tokmd output
tokmd export --format json | jq '.rows[].path'
# ["secrets/credentials.yaml", "internal/api-keys.txt", ...]
```

**Mitigations in Place:**
- `--redact` modes: `none`, `paths`, `all`
- `redact_path()` uses BLAKE3 short hash, preserves extension
- `short_hash()` for pattern redaction
- Cross-platform path normalization

**Residual Risk:** LOW - Redaction modes are comprehensive

### 4.2 Sensitive File Content Disclosure

**Threat:** tokmd could read and output content from sensitive files (`.env`, `*.key`, credentials).

**Affected Components:**
- `crates/tokmd-scan/` - File scanning via tokei
- `crates/tokmd-io-port/` - Host-abstracted file access
- Feature flag `content` - File content scanning (entropy, tags, hashing)

**Attack Scenario:**
- tokmd scans a directory containing `.env` files
- Without proper gitignore, these files are counted/included
- Content could be exposed in export or analysis outputs

**Mitigations in Place:**
- `.tokeignore` templates exclude common sensitive patterns
- Tokei handles gitignore semantics
- Content feature requires explicit `--features content`
- No content hashing without explicit feature

**Residual Risk:** MEDIUM - Depends on user configuration of ignore files

### 4.3 Secrets in Output Metadata

**Threat:** Command-line arguments, environment variables, or paths containing secrets could appear in output metadata or logs.

**Affected Components:**
- CLI argument parsing (`crates/tokmd/src/cli/`)
- Receipt metadata including scan configuration
- Error messages

**Attack Scenario:**
```
tokmd lang --paths /home/user/project --excluded "*.key"
# Secret path appears in receipt scan.paths
```

**Mitigations in Place:**
- No automatic logging of full arguments
- Error messages designed to not leak paths
- Receipt uses normalized paths

**Residual Risk:** MEDIUM - User must be careful with command arguments

### 4.4 FFI Data Leakage

**Threat:** Internal data structures could leak through FFI boundaries to calling applications.

**Affected Components:**
- `tokmd-core/src/ffi/` - JSON envelope extraction
- All language bindings (Python, Node, WASM)

**Attack Scenario:**
- Error handling in FFI exposes internal paths or data
- Exception contains stack trace with sensitive information

**Mitigations in Place:**
- `#![forbid(unsafe_code)]` in all crates
- Error translation at FFI boundaries
- Python: `TokmdError` exception type, no `.expect()` in production
- Node: Error mapped via `map_envelope_error()`
- WASM: `to_js_error()` for controlled error propagation

**Residual Risk:** LOW - Strict FFI safety patterns implemented

### 4.5 WASM Browser Memory Exposure

**Threat:** In browser contexts, tokmd-wasm output could be accessible to other JavaScript on the same page.

**Affected Components:**
- `crates/tokmd-wasm/` - Browser/WASM bindings

**Attack Scenario:**
- Malicious third-party script on same page reads tokmd output from memory

**Mitigations in Place:**
- WASM memory is isolated per module instance
- No shared state between instances
- Capabilities reported via `capabilities()` function

**Residual Risk:** LOW - Browser isolation provides protection

---

## 5. Denial of Service Threats

### 5.1 Path Traversal via Symbolic Links

**Threat:** Symbolic links could cause tokmd to traverse beyond intended scan boundaries, consuming excessive resources or exposing data.

**Affected Components:**
- `crates/tokmd-scan/` - File traversal via tokei
- `crates/tokmd-io-port/` - Abstracted file access

**Attack Scenario:**
```
# Attacker creates symlink to system directories
ln -s /etc ./link_to_etc
tokmd lang
# Scans /etc, exposing system configuration
```

**Mitigations in Place:**
- Tokei respects `.gitignore` and ignore patterns
- `--follow` not enabled by default
- Walk behavior controlled by settings

**Residual Risk:** MEDIUM - User must be careful with symlinks in scanned dirs

### 5.2 Excessive Resource Consumption

**Threat:** Maliciously crafted repositories with many small files or deep directory structures could cause excessive CPU/memory usage.

**Affected Components:**
- `crates/tokmd-scan/` - Core scanning
- `crates/tokmd-model/` - Aggregation
- All CLI commands

**Attack Scenario:**
```
# Attacker creates repo with 100,000 small files
for i in $(seq 1 100000); do echo "x" > file_$i.txt; done
tokmd lang
# System becomes unresponsive during scan
```

**Mitigations in Place:**
- `max_files`, `max_bytes` settings in analysis
- `max_commits` for git history limits
- No hard limits in core scan (delegated to tokei)
- `tokmd-gate` can enforce resource policies

**Recommendations:**
- Add `max_depth` setting for directory traversal
- Implement `max_files` for all scan modes, not just analysis

### 5.3 Git Log DoS via Large History

**Threat:** Repositories with extensive git history could cause tokei to hang or consume excessive resources during cockpit analysis.

**Affected Components:**
- `crates/tokmd-git/` - Git history collection
- `crates/tokmd-cockpit/` - PR metrics with git analysis
- `crates/tokmd-analysis/` - Git hotspot detection

**Attack Scenario:**
```
# Attacker creates repo with 1 million commits
tokmd cockpit --base main --head HEAD
# git log command runs for hours
```

**Mitigations in Place:**
- `collect_history()` has `max_commits` parameter
- `max_commit_files` for commit file limit
- `get_added_lines()` uses `--unified=0` for minimal output

**Residual Risk:** MEDIUM - `max_commits` not enforced by default

### 5.4 Malformed JSON/Input DoS

**Threat:** Malformed JSON passed to FFI could cause parsing loops or memory exhaustion.

**Affected Components:**
- `tokmd-core/src/ffi/parse.rs` - JSON argument parsing
- `tokmd-envelope/` - Envelope parsing and validation
- All language bindings

**Attack Scenario:**
```json
{
  "inputs": [
    {"path": "x", "text": "AAAAAAAAAAAA..."}  // 10MB of text
  ]
}
```

**Mitigations in Place:**
- WASM validates preset restrictions before processing
- Python/Node use runtime JSON parsing (native error handling)
- Envelope validation in `tokmd-envelope`

**Residual Risk:** MEDIUM - Large inputs bypass some checks

### 5.5 Circular Symlink Traps

**Threat:** Circular symlink directories could cause infinite loops.

**Affected Components:**
- File traversal in `crates/tokmd-scan/`
- `crates/tokmd-io-port/`

**Attack Scenario:**
```
mkdir dir
ln -s ../dir dir/loop
tokmd lang  # Hangs
```

**Mitigations in Place:**
- Tokei handles traversal with built-in cycle detection
- `ignore` crate used for file walking (handles symlinks)
- No explicit loop detection in tokmd code

**Residual Risk:** LOW - Tokei handles this gracefully

---

## 6. Elevation of Privilege Threats

### 6.1 FFI Escape to Host System

**Threat:** A vulnerability in the FFI layer could allow escape to the host system with the privileges of the tokmd process.

**Affected Components:**
- `tokmd-core/src/ffi/` - Core FFI implementation
- `tokmd-python/` - PyO3 bindings
- `tokmd-node/` - napi-rs bindings
- `tokmd-wasm/` - wasm-bindgen bindings

**Attack Scenario:**
- Malicious JSON input causes buffer overflow in Rust code
- Attacker gains shell access via compromised process

**Mitigations in Place:**
- `#![forbid(unsafe_code)]` enforced in all crates
- `#![deny(clippy::all)]` in binding crates
- PyO3: "Never Panic Guarantee" - no `.expect()` in production
- No `unsafe` blocks in FFI code
- GIL properly managed in Python bindings

**Residual Risk:** VERY LOW - Rust safety guarantees + no unsafe code

### 6.2 Arbitrary File Read via Path Injection

**Threat:** Path injection through command-line arguments could allow reading files outside intended scope.

**Affected Components:**
- CLI parsing in `crates/tokmd/src/cli/`
- Settings in `crates/tokmd-settings/`
- Scan options in `crates/tokmd-scan/`

**Attack Scenario:**
```
tokmd lang --paths "../../../etc/passwd"
# Attempts to read sensitive files
```

**Mitigations in Place:**
- Tokei handles path resolution
- `.gitignore` semantics prevent arbitrary access
- Path normalization to forward slashes
- Deterministic path handling throughout

**Residual Risk:** LOW - Tokei paths are resolved, not arbitrary file read

### 6.3 Shell Command Injection via Git Integration

**Threat:** Git integration could be vulnerable to command injection if git refs are not properly validated.

**Affected Components:**
- `crates/tokmd-git/src/command.rs` - Git command execution
- `crates/tokmd-git/src/lib.rs` - Git history collection
- Cockpit, diff, and analysis workflows

**Attack Scenario:**
```rust
// If base/head refs contain shell metacharacters
tokmd cockpit --base "; rm -rf /" --head HEAD
```

**Mitigations in Place:**
- Git refs passed as arguments, not interpolated into shell strings
- `git_cmd()` wrapper uses explicit argument passing
- `rev_exists()` validation before use
- `resolve_base_ref()` for ref resolution

**Residual Risk:** LOW - Arguments are passed safely

### 6.4 Configuration Injection via .tokeignore

**Threat:** Malicious `.tokeignore` could contain patterns that cause unexpected behavior.

**Affected Components:**
- `crates/tokmd-scan/src/tokeignore/` - Template handling
- Tokei's ignore file parsing

**Attack Scenario:**
```
# Malicious .tokeignore
**/secrets/**
../../../etc/**
```

**Mitigations in Place:**
- Tokei handles ignore parsing safely
- `check-ignore` command reveals ignored patterns
- User controls .tokeignore creation

**Residual Risk:** LOW - User controls configuration files

### 6.5 WASM Sandboxing Escape

**Threat:** Browser/worker environment could be compromised through WASM vulnerability.

**Affected Components:**
- `crates/tokmd-wasm/` - WASM bindings

**Attack Scenario:**
- WASM runtime vulnerability allows memory access outside sandbox
- Malicious code reads browser cookies, localStorage

**Mitigations in Place:**
- `#![forbid(unsafe_code)]` in WASM crate
- wasm-bindgen provides safe JS interop
- No `web-sys` or `js-sys` in narrow WASM mode

**Residual Risk:** VERY LOW - WASM provides strong isolation

---

## Attack Surface Summary

| Component | S | T | R | I | D | E | Risk Level |
|-----------|---|---|---|---|---|---|-------------|
| CLI Interface | ✓ | | | | ✓ | | MEDIUM |
| File System Access | | ✓ | | ✓ | ✓ | ✓ | MEDIUM |
| Git Integration | | ✓ | ✓ | | ✓ | | MEDIUM |
| FFI Layer (Python) | ✓ | | | ✓ | | ✓ | LOW |
| FFI Layer (Node) | ✓ | | | ✓ | | ✓ | LOW |
| FFI Layer (WASM) | ✓ | | | ✓ | | ✓ | LOW |
| Output Formats | ✓ | ✓ | | ✓ | | | MEDIUM |
| Configuration Files | | ✓ | | | | | LOW |

---

## Security Controls Matrix

| Threat | Control | Implementation | Effectiveness |
|--------|---------|----------------|--------------|
| Path Disclosure | Redaction | `tokmd-format::redact` module | HIGH |
| FFI Escape | Unsafe Code Ban | `#![forbid(unsafe_code)]` | VERY HIGH |
| Python Crash | Never Panic | `?` operator + TokmdError | HIGH |
| Git Injection | Safe Argument Passing | `git_cmd()` wrapper | HIGH |
| DoS via Files | Resource Limits | `max_files`, `max_bytes` settings | MEDIUM |
| Output Tampering | Schema Versioning | Separate versions per receipt type | MEDIUM |
| Config Tampering | Validation | `check-ignore` command | MEDIUM |
| WASM Escape | Sandboxing | WASM memory isolation | VERY HIGH |

---

## Recommendations

### High Priority

1. **Add receipt signing** - Implement optional digital signatures for receipts to ensure provenance
2. **Enforce max_files in all scan modes** - Currently only enforced in analysis preset
3. **Add repo git hash to receipts** - Bind receipt to specific repository state
4. **Implement git history integrity check** - Detect rewritten history in cockpit receipts

### Medium Priority

5. **Add `--max-depth` flag** - Limit directory traversal depth
6. **Add warning for non-linear git history** - Detect force-pushes in cockpit
7. **Document security considerations for CI usage** - Guidance on running tokmd in untrusted contexts
8. **Add audit log for scan operations** - Immutable record of who/when/what

### Low Priority

9. **Consider ChaCha20-Poly1305 for receipt signatures** - Authenticated encryption for receipts
10. **Add external timestamp authority support** - RFC 3161 timestamps for receipts
11. **Document FFI safety guarantees per binding** - Formal security documentation for bindings

---

## Conclusion

tokmd demonstrates good security posture through:
- `#![forbid(unsafe_code)]` enforced workspace-wide
- Strict FFI safety patterns in Python/Node/WASM bindings
- Path normalization and deterministic output
- Comprehensive redaction modes
- Git integration using safe argument passing

Primary residual risks center on:
- User-controlled configuration files (.tokeignore)
- Potential DoS via maliciously structured repositories
- No cryptographic proof of receipt provenance

The threat model should be reviewed quarterly or after any significant architectural change.
