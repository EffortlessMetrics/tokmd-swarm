# Security Scan Report

**Generated:** 2026-10-26
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium

## Scope and Evidence Basis

**Intended Window:** 2026-10-19 → 2026-10-26

**Observed Scope:** The scan examined all 24 commits reachable from any ref
whose author date falls in the 7-day window, plus the checked-out `main`
(`46b8963 fix(ci): require exact qualification requests and share work
deadline (#737)`, dated 2026-10-17) as the baseline against which every
finding is stated.

**Important scope note:** `main` has not received a commit since
2026-10-17. All 24 in-window commits live on unmerged feature branches
(`origin/codex/*`, `origin/fix/*`, `origin/ci/*`, `origin/docs/*`,
`origin/sdlc/*`, `origin/dependabot/*`). The scan therefore reports each
finding with an explicit **state** relative to `main`:

- **Present on `main`** — the defect is live in the default branch today.
- **Fixed on an open branch** — the defect exists on `main` and a reviewed
  fix is already proposed in an open PR.

No automated vulnerability scanner, SAST tool, or dependency auditor was
run during this scan. All findings below are from manual review plus
targeted reproduction using the repository's own binaries and test
harnesses. This is an advisory finding set, not a security pass.

## Executive Summary

| Severity | Count | Auto-fixed | Manual Required |
|----------|-------|------------|-----------------|
| CRITICAL | 0     | 0          | 0               |
| HIGH     | 1     | 0          | 1               |
| MEDIUM   | 3     | 0          | 3               |
| LOW      | 0     | 0          | 0               |

**Total Findings:** 4
**Auto-fixed:** 0
**Manual Review Required:** 4

**Patch status:** No patches were applied by this scan. Three of the four
findings already have reviewed, CI-qualified fix PRs open against `main`
(#750, #758, #771); re-implementing them here would duplicate in-flight
remediation and race those PRs. The fourth finding (VULN-004) has **no**
open PR and no owning issue, and is reported for maintainer triage.

The scan reviewed 24 in-window commits covering 63 changed files and
found the in-window work to be, on balance, **security-positive**: 8 of the
changes are explicit hardening commits (strict UTF-8 path decoding, NUL-framed
path parsing, symlink containment, stdout write/flush error propagation,
Cargo lockfile pinning, cross-run target isolation).

### Findings summary

| ID | Title | Severity | STRIDE | State | Tracked by |
|----|-------|----------|--------|-------|------------|
| VULN-001 | Model layer collapses distinct source files into one receipt row | HIGH | Tampering | Present on `main` | #764 (issue) |
| VULN-002 | Git path discovery lossily aliases non-UTF-8 tracked filenames | MEDIUM | Tampering | Present on `main` | #757 / PR #758 |
| VULN-003 | Review-packet verifier certifies artifacts reached via a directory symlink | MEDIUM | Tampering | Present on `main` | PR #750 |
| VULN-004 | Review-packet writer emits artifacts outside the requested packet directory | MEDIUM | Tampering | Present on `main` | **Untracked** |

### Non-findings explicitly excluded

The following were examined and are **not** reported as findings:

- `action.yml:222` (`version="${{ inputs.version }}"`) interpolates a workflow
  input directly into a `run:` shell body rather than through `env:`. This
  is a GitHub Actions script-injection pattern. It is **pre-existing on
  `main`** (not introduced in this window), the `version` input is
  attacker-selectable only by the workflow author (not by a PR author),
  and the value reaches a `curl` URL and a `grep` regex rather than being
  executed as a command. Recorded below as OBS-010.
- `actions/upload-artifact` tag-vs-SHA pinning remains mixed across
  workflows. The in-window dependabot commit bumps tags without changing
  the existing mixed policy and preserves the one SHA-pinned reference.
  This is the carried OBS-003 from the 2026-10-19 scan, unchanged.
- The removed `cleanup_stale_scoped_dirs` / `find /tmp ... -exec rm -rf`
  cleanup was reviewed for a destructive-path-traversal bug. It is **not**
  exploitable: `label` was a hardcoded literal (`"gate-target"`), never
  user-derived, and the removal in `ff30e68` is a correctness improvement
  (directory age cannot distinguish an abandoned target from a live
  sibling run), not a security fix for a live defect.

---

## Critical Findings

*None.*

---

## High Findings

### VULN-001: Model layer collapses distinct source files into one receipt row

| Attribute | Value |
|-----------|-------|
| **Severity** | HIGH |
| **STRIDE Category** | Tampering |
| **CWE** | CWE-178 (Improper Handling of Case Sensitivity) / CWE-41 (Improper Resolution of Path Equivalence) |
| **File** | `crates/tokmd-model/src/lib.rs:96`, `crates/tokmd-model/src/rows.rs:206` |
| **Status** | Manual fix required — issue #764 open, no PR |

**Description:**
`normalize_path` rewrites every backslash in a path to a forward slash
before the path is used as a receipt row key:

```rust
pub fn normalize_path(path: &Path, strip_prefix: Option<&Path>) -> String {
    let s_cow = path.to_string_lossy();
    let s: Cow<str> = if s_cow.contains('\\') {
        Cow::Owned(s_cow.replace('\\', "/"))
    } else {
        s_cow
    };
```

On Unix, `\` is an ordinary filename byte, not a directory separator. Two
distinct files — `a\b.rs` (a single flat file whose name contains a
backslash) and `a/b.rs` (a file inside directory `a`) — therefore
normalize to the identical key `a/b.rs`. `insert_row` then **merges** the
two entries rather than replacing one:

```rust
let entry = map.entry(key).or_insert_with(|| (module, Agg::default()));
entry.1.code += stats.code;
// ...
```

The merged row reports the summed line counts under a single path. The
silently-absorbed file vanishes from the receipt, and the receipt still
declares `"status": "complete"` with an empty `warnings` array.

**Evidence (reproduced against the built `tokmd` binary):**

```
$ mkdir -p /tmp/merge_test2 && cd /tmp/merge_test2
$ python3 -c "open('a\\\\b.rs','w').write('fn x(){}\n'); \
              import os; os.makedirs('a'); open('a/b.rs','w').write('fn y(){}\nfn z(){}\n')"

$ ls -bR
.:
a
a\\b.rs          <- literal backslash, flat file

./a:
b.rs             <- real directory

$ tokmd export . --format json
  'a/b.rs' code= 3 lines= 3
  total rows: 1 status: complete
```

Two files on disk (`a\b.rs` with 1 code line, `a/b.rs` with 2 code lines)
produced **one** row reporting `code=3`. The `a\b.rs` file is absent. The
receipt claims `status: complete` and `warnings: []`.

The same collision reproduces in JSONL and CSV:

```
$ tokmd export . --format jsonl
{"type":"row","path":"a/b.rs","module":"a","lang":"Rust","kind":"parent","code":3,...}

$ tokmd export . --format csv
path,module,lang,kind,code,comments,blanks,lines,bytes,tokens
a/b.rs,a,Rust,parent,3,0,0,3,120,30
```

A second, independent collision exists on the lossy-decoding axis: two
distinct raw-byte filenames (`collision-\xff.rs`, `collision-\xfe.rs`) plus
a valid UTF-8 name containing the literal replacement character
(`collision-\u{fffd}.rs`) collapse to a single row `collision-�.rs`,
again with `status: complete` and no warning.

**Reachability:** Fully attacker-controlled. A repository author creates a
filename containing a backslash (or a raw non-UTF-8 byte) and commits it.
Every consumer of `tokmd export` — the CLI, the Python/Node/WASM bindings,
the GitHub Action, and the `Tokmd Rust Result` proof receipts — receives a
receipt whose row count is lower than the true file count while asserting
completeness.

**Impact:**
- **Completeness misrepresentation.** The receipt asserts
  `status: complete` while omitting source files. Downstream automation
  that gates on completeness (`ensure_complete_scan` in
  `crates/tokmd-scan/src/lib.rs:85`) cannot detect the loss, because
  completeness is asserted from tokei's read status, not from an
  independent enumeration of source identities.
- **Silent merge of distinct sources.** Merged line counts are attributed
  to one path. Reviewers and the `cockpit` risk signals see a single file
  that does not exist on disk, and the true file is invisible to
  review-priority ordering.
- **Undetectable tampering.** A reviewer comparing a receipt against the
  tree sees no mismatch, because the receipt is internally consistent —
  it simply omits a file. There is no warning to notice.

**Recommended Fix:**
Separate *source identity* from *display path*, as issue #764 specifies.
Concretely, the row key must be derived from a byte-preserving source
identity rather than from the display-normalized path, and the two must not
be interchangeable. If no existing public receipt schema can carry the
source losslessly, the receipt must emit an explicit `partial` /
`unsupported` state with a byte-safe diagnostic rather than a complete
inventory. Issue #764 already carries this contract and its acceptance
criteria; this finding confirms the severity and reproduces the defect
end-to-end against the current binary.

---

## Medium Findings

### VULN-002: Git path discovery lossily aliases non-UTF-8 tracked filenames

| Attribute | Value |
|-----------|-------|
| **Severity** | MEDIUM |
| **STRIDE Category** | Tampering |
| **CWE** | CWE-838 (Inappropriate Encoding for Output Context) |
| **File** | `crates/tokmd-scan/src/walk/git.rs:63` |
| **Status** | Manual fix required — PR #758 open, CI-qualified |

**Description:**
`git_ls_files` decodes NUL-framed Git output with a lossy conversion:

```rust
for part in bytes.split(|b| *b == 0) {
    if part.is_empty() {
        continue;
    }
    let s = String::from_utf8_lossy(part).to_string();
    files.push(PathBuf::from(s));
}
```

`String::from_utf8_lossy` replaces every invalid byte with U+FFFD. A tracked
file named `raw-\xff.png` and a distinct tracked file named
`raw-\u{fffd}.png` therefore decode to the identical string, and the
`BTreeSet<PathBuf>` in `PathSelector` deduplicates them into one entry.

**Evidence (reproduced against `tokmd-scan`):**

```
$ git ls-files -z (hex)
b'raw-\xef\xbf\xbd.png'   -> lossy: 'raw-\ufffd.png'
b'raw-\xff.png'           -> lossy: 'raw-\ufffd.png'

$ tokmd_scan::walk::list_files(root, None)
listed = ["raw-\ufffd.png"]
invalid_present    = true
replacement_present = true
listed_len = 1
```

Both `invalid_present` and `replacement_present` are simultaneously true
against a single returned entry, which proves the alias rather than a
mere omission. One of the two tracked files is silently dropped.

The lossy path also **resolves to a different file on disk**: reading
`raw-\u{fffd}.png` returns the legitimate file's bytes, not the
invalid-UTF-8 file's. The invalid-UTF-8 tracked file is never read and
never counted.

**Reachability:** A repository author commits a filename with invalid
UTF-8 bytes. `list_files` is reached from `tokmd analyze` with the `walk`
feature (`crates/tokmd-analysis/src/analysis/files.rs:129`) and from
`tokmd syntax` (`crates/tokmd/src/commands/syntax.rs:111`).

**Impact:** The analysis receipt under-reports the file inventory while
declaring `status: complete`. Combined with VULN-001 this is the same
lossiness class, but at the discovery layer, and it is the layer that the
repo's own issue #757 identified as the first repair in a two-step
sequence.

**Recommended Fix:** PR #758 already replaces the lossy decode with strict
`std::str::from_utf8` and an error carrying the offending raw bytes plus
rename-and-retry guidance, and adds negative/positive controls. Merge
PR #758 (it is CI-qualified and its body cites the same
"complete/1 image for two tracked images" reproduction). Note that PR #758
explicitly does **not** repair the model-layer collisions recorded as
VULN-001; issue #764 owns that half.

### VULN-003: Review-packet verifier certifies artifacts reached via a directory symlink

| Attribute | Value |
|-----------|-------|
| **Severity** | MEDIUM |
| **STRIDE Category** | Tampering |
| **CWE** | CWE-59 (Improper Link Resolution Before File Access) |
| **File** | `xtask/src/tasks/review_packet_check.rs:192` (on `main`) |
| **Status** | Manual fix required — PR #750 open, CI-qualified |

**Description:**
`validate_manifest_artifacts` checks each artifact with
`fs::symlink_metadata`, which does not follow a symlink at the final
component but **does** follow a symlinked intermediate directory:

```rust
let artifact_path = dir.join(&relative_path);
match fs::symlink_metadata(&artifact_path) {
    Ok(metadata) if metadata.file_type().is_file() => {
        verify_artifact_hash(&artifact_path, &display_path, artifact, &mut errors);
    }
```

`packet_relative_path` rejects `..` and absolute components, so the
lexical path stays inside the packet. But a manifest entry of
`proof/observation.json`, where `proof` is a symlink to an external
directory, passes the lexical check and the regular-file check, and the
BLAKE3 hash is then computed over a file that lives **outside** the packet
directory.

**Evidence (reproduced against the real filesystem semantics):**

```
$ ln -s /tmp/symtest/external /tmp/symtest/packet/proof
$ python3 -c "import os; \
    print('symlink_metadata follows intermediate dir symlink? ->', os.path.isfile(p)); \
    print('realpath escapes packet dir ->', os.path.realpath(p))"
symlink_metadata follows intermediate dir symlink? -> True
realpath escapes packet dir -> /tmp/symtest/external/observation.json
```

**Reachability:** The review packet directory is produced by `tokmd cockpit`
and verified by `cargo xtask review-packet-check`. The action.yml
`Verify Review Packet` step runs the verifier against the packet produced
in the same job. A party that can place a directory symlink inside the
packet directory before verification controls which bytes are certified.

**Impact:** The verifier receipt (`ok: true`, `hashes_verified: N`) can
certify evidence bytes that are not packet-local, defeating the purpose of
the manifest-hash contract that the hosted PR comment reports as
`Manifest hashes: valid`.

**Recommended Fix:** Merge PR #750. It resolves the packet root once,
canonicalizes each listed regular artifact, and rejects any artifact
resolving outside the packet root, while preserving internal directory
links and a symlinked packet root. Its regression test
`review_packet_rejects_proof_directory_symlink_escape` reproduces exactly
the condition shown above.

### VULN-004: Review-packet writer emits artifacts outside the requested packet directory

| Attribute | Value |
|-----------|-------|
| **Severity** | MEDIUM |
| **STRIDE Category** | Tampering |
| **CWE** | CWE-59 (Improper Link Resolution Before File Access) / CWE-22 (Path Traversal) |
| **File** | `crates/tokmd-cockpit/src/render/review_packet.rs:65`, `crates/tokmd-cockpit/src/render/review_packet.rs:107` |
| **Status** | Manual fix required — **no open PR or issue owns this** |

**Description:**
VULN-003 is the *verifier* side of the review packet. The *writer* side has
no equivalent containment check. `write_review_packet` creates the packet
directory and writes each artifact through it:

```rust
std::fs::create_dir_all(dir)?;
...
std::fs::write(dir.join("cockpit.json"), &cockpit_json)?;
...
if !proof_artifacts.is_empty() {
    std::fs::create_dir_all(dir.join("proof"))?;
    for artifact in &proof_artifacts {
        std::fs::write(dir.join(&artifact.path), &artifact.content)?;
    }
}
```

`create_dir_all` succeeds when the target already exists as a symlink to a
directory, and `fs::write` then follows that symlink. If the packet
directory path is itself a symlink — or contains a symlinked `proof`
subdirectory — every artifact is written **outside** the requested packet
directory.

**Evidence (reproduced end-to-end with the built `tokmd` binary):**

```
$ # A PR commits .tokmd/review as a symlink to an attacker-chosen directory
$ git ls-files -s .tokmd
120000 f3122a8bf1994371fa5d562716ae53acdfa92056 0   .tokmd/review

$ tokmd cockpit --base HEAD~1 --head HEAD --review-packet-dir .tokmd/review --format json
exit=0

$ ls -la /tmp/prtest_outside/
-rw-r--r-- 1 runner runner 1857 cockpit.json
-rw-r--r-- 1 runner runner  456 comment.md
-rw-r--r-- 1 runner runner 2537 evidence.json
-rw-r--r-- 1 runner runner 1660 manifest.json
-rw-r--r-- 1 runner runner  896 review-map.json
-rw-r--r-- 1 runner runner  896 review-map.md
```

All six packet artifacts were written to `/tmp/prtest_outside/`, entirely
outside the requested `--review-packet-dir .tokmd/review`. The command
exits 0 and reports success.

The same escape reproduces for a symlinked intermediate `proof`
subdirectory when proof evidence is supplied via `--proof-*`:

```
$ mkdir -p .tokmd/review && ln -s /tmp/wrt/outside2 .tokmd/review/proof
$ tokmd cockpit ... --review-packet-dir .tokmd/review
exit=0
```

**Reachability:** `.tokmd/` is **not** gitignored in this repository — only
`.tokmd/cache/` is (`.gitignore:34`) — and `git check-ignore` confirms
`.tokmd/review` is not ignored. Git tracks a symlink as mode `120000` and
restores it as a symlink on checkout. A pull-request author therefore
commits `.tokmd/review` as a symlink and the checkout presents it as a
symlink to any CI job that later runs `tokmd cockpit
--review-packet-dir .tokmd/review`.

This is the path `action.yml` uses by default: `review_packet_dir=".tokmd/review"`
(`action.yml:485`).

**Impact:**
- **Arbitrary-directory file planting.** The attacker chooses the symlink
  target. Six files with fixed names (`cockpit.json`, `comment.md`,
  `evidence.json`, `manifest.json`, `review-map.json`, `review-map.md`) are
  created at an attacker-chosen location. The names are fixed, so this is
  not arbitrary-filename overwrite, but it is arbitrary-directory write.
- **Content injection.** The written content embeds attacker-influenced
  data from the PR — changed file paths, line counts, module keys.
  `comment.md` is the file that `action.yml` posts to the PR via
  `thollander/actions-comment-pull-request` and copies into the uploaded
  artifact, so attacker-influenced content is published through a channel
  the reader attributes to the tool.
- **Asymmetric coverage.** PR #750 hardens the verifier so that it rejects
  out-of-root artifacts, but the writer can still *place* them out of root.
  Verifier hardening without writer hardening leaves the escape
  reachable, it simply moves the failure later in the flow.

**Reachability caveats (stated explicitly):**
- In `.github/workflows/test-action.yml` the `Clean generated files` step
  (`rm -rf ... .tokmd ...`) runs at phase 18, *before* the review-packet
  job at phase 21, so that workflow's committed-symlink path is currently
  neutralized by cleanup ordering. The published `action.yml` composite
  action has **no** equivalent cleanup step.
- The symlink target must be a writable directory on the runner.
- `comment.md` content is templated from PR-derived data rather than
  raw attacker text, so this is a controlled-content injection, not
  arbitrary Markdown.

**Recommended Fix:**
Add a containment check to the writer, symmetric with PR #750's verifier
check. Before writing, resolve the packet root and each artifact's parent
directory and reject any artifact whose resolved path falls outside the
resolved packet root. Prefer creating the packet directory with
`create_dir` (not `create_dir_all`) and failing when it already exists as
a symlink. A conservative option that also closes the `proof` subdirectory
variant is to refuse to write into any path component that is a symlink.
Record the same "stable filesystem paths" assumption that PR #750 documents
for the verifier.

Because no open PR or issue currently owns this seam, the scan recommends
opening a tracking issue with the reproduction above so the fix is
admitted through the repo's normal proof path.

---

## Low Findings

*None.* Findings below the `medium` threshold are recorded in the
Observations appendix.

---

## Observations (Below Threshold — Not Reported As Findings)

### OBS-010 (new): `action.yml` interpolates `inputs.version` directly into a shell body

`action.yml:222` uses `version="${{ inputs.version }}"` inside a `run:`
block rather than routing the value through `env:`. Every other input in
the same file is correctly routed through `env:` (lines 117-119, 313-326,
633). GitHub Actions expands `${{ }}` into the script text before the shell
sees it, so an input value containing shell metacharacters would be
interpreted as shell syntax.

Not reported as a finding because the value is authored by the workflow
author, not by a pull-request author; it reaches a `curl` URL and a `grep`
regular expression rather than a command position; and the defect is
pre-existing on `main` rather than introduced in this scan's window.
Recorded so the inconsistency with the file's own `env:` convention is
visible to the next maintainer touching this file. Recommended follow-up
(not urgent): route `inputs.version` through `env:` like its siblings and
validate it against `^v?[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$`
before use.

### OBS-011 (carried): GitHub Actions pinning is mixed (tag + SHA)

Unchanged from the 2026-10-19 scan (OBS-003). The in-window dependabot
commit `8b8abfd` bumps `actions/upload-artifact` from `v7.0.1` to `v7.0.2`
in tag-pinned positions and from `043fb46d…` to `cf430e03…` in the single
SHA-pinned position in `ci.yml`. It neither adds nor removes a
SHA-pinned reference, so it introduces no new pinning regression. The
custom `EffortlessMetrics/droid-action-safe` action and
`actions/checkout` remain SHA-pinned. Below the threshold; informational.

### OBS-012 (carried): `run_review_packet` documents a stable-filesystem assumption that the writer does not yet share

PR #750 adds "verification assumes paths are not being replaced
concurrently" to `docs/review-packet.md` and `docs/specs/review-packet.md`
for the verifier. VULN-004 shows the writer currently has no equivalent
constraint at all. When VULN-004 is remediated, the same documentation
paragraph should be extended to cover the writer's assumptions.

---

## Appendix

### Threat Model

- **Version:** 2026-08-02 (85 days old at scan date; within the 90-day
  regeneration window, so it was used as-is and not regenerated)
- **Location:** `.factory/threat-model/threat-model.md`

The threat model already anticipated the class that VULN-001 and VULN-002
fall into. Under Denial of Service it records "Pathological FFI JSON
payload" and under Elevation of Privilege it records the transitive
`RUSTSEC-2020-0163` advisory, both explicitly marked "Observed but not at
severity threshold for a finding". Its Standing Defenses list does not
include path-identity preservation, which is the gap VULN-001 and VULN-004
expose; the next threat-model refresh (due on or before 2026-11-01) should
record source-identity vs display-path separation as a standing defense.

### Scan Metadata

- **Commits Scanned:** 24 (in-window, across all refs) + `main` baseline
  `46b8963`
- **Unique Files Changed:** 63
- **Source Files Reviewed:** 21
- **Findings Reproduced:** 4 (all reproduced against repository-built
  binaries or the repository's own test harness, not inferred from source
  reading alone)
- **Scan Duration:** ~55m
- **Skills Used:** threat-model-generation (not invoked — existing threat
  model was current), commit-security-scan, vulnerability-validation
- **Automated scanners run:** none

### Files reviewed

Source (21):
`crates/tokmd-scan/src/walk/git.rs`,
`crates/tokmd/src/analysis_utils.rs`,
`crates/tokmd/src/commands/{badge,check_ignore,completions,diff}.rs`,
`crates/tokmd/src/commands/cockpit.rs`,
`crates/tokmd-cockpit/src/render/review_packet.rs`,
`crates/tokmd-cockpit/src/proof_evidence/{model,normalize}.rs`,
`crates/tokmd-model/src/{lib,rows}.rs`,
`xtask/src/tasks/{affected,build_guard,ci_plan,gate,review_packet_check}.rs`,
`.cargo/config.toml`, `ci/proof.toml`,
`policy/{cargo-command-surfaces,ci-lane-whitelist,ci-risk-packs}.toml`,
`action.yml`.

Workflows and scripts (18): all `.github/workflows/*.yml` touched in the
window, `scripts/ci_budget_test.py`, `scripts/ci_qualification_review_test.py`.

### Prior-scan carry-forward

All observations carried from the 2026-10-19 report (OBS-001 through
OBS-010 in that report) were re-checked against `main` and against the
in-window branches. OBS-001 (unbounded FFI JSON), OBS-002 (transitive
`RUSTSEC-2020-0163`), OBS-003 (mixed action pinning), OBS-004
(`web/runner` fetch origins), OBS-005 (`action.yml` install `curl | sh`
style download), OBS-006 (zero required approvals) and OBS-008 through
OBS-010 remain unchanged and are not re-listed here. OBS-007 (the
`taiki-e/install-action` SHA-vs-comment agreement) was re-checked and
remains unchanged: the in-window commits do not touch the typos lane.

### References

- [CWE Database](https://cwe.mitre.org/)
- [STRIDE Threat Model](https://docs.microsoft.com/en-us/azure/security/develop/threat-modeling-tool-threats)
- Issue [#757](https://github.com/EffortlessMetrics/tokmd-swarm/issues/757)
  — lossy Git discovery
- Issue [#764](https://github.com/EffortlessMetrics/tokmd-swarm/issues/764)
  — model-layer source path identity
- PR [#758](https://github.com/EffortlessMetrics/tokmd-swarm/pull/758) —
  lossy Git discovery fix
- PR [#750](https://github.com/EffortlessMetrics/tokmd-swarm/pull/750) —
  review-packet artifact containment
- PR [#771](https://github.com/EffortlessMetrics/tokmd-swarm/pull/771) —
  renamed-path retention in affected/CI selectors
