# Spec: RIPR repository inventory profile

- Status: draft
- Metric definition version: 1
- Related proof scopes: `scan`, `model`
- Tracking: [tokmd#2863](https://github.com/EffortlessMetrics/tokmd/issues/2863), [ripr-swarm#2348](https://github.com/EffortlessMetrics/ripr-swarm/issues/2348)

## Contract

The `ripr_inventory_v1` profile measures the existing tokmd export and module
receipts on an exact RIPR checkout. It scans recognized file types under the
RIPR crate, VS Code extension, and xtask source roots. Hidden files are
excluded. The profile ignores ambient ignore files and tokei configuration.
The proof supplies the full tokmd configuration and its explicit exclusions.

| Category | Selection |
| --- | --- |
| Generated | Exclude `generated/` and `*.generated.rs` / `*.generated.ts`. |
| Test | Exclude `test/`, `tests/`, `*_test.rs`, `*_tests.rs`, `*.test.ts`, and `*.spec.ts`. Inline tests contribute to their containing file. |
| Fixture | Exclude `fixture/` and `fixtures/`. |
| Vendored | Exclude `vendor/`, `vendored/`, and `node_modules/`. |
| Build output | Exclude `target/`, `dist/`, and `out/`. |
| Remaining source-root inventory | Include recognized visible files under the three roots. |

Directory patterns match at any depth. These rules use paths and filenames.
Files in the chosen roots may have other conventions, including generated or
test content in otherwise selected files.

File identity uses the repository-relative forward-slash path and the tracked
blob ID. Module identity uses two components under `crates` and `editors`,
and the first component elsewhere. Keep both paths on a rename and record a
deletion as absence at head. An extraction can redistribute lines within the
same module with no aggregate line reduction. Architectural claims require
separate evidence.

| Metric ID | Unit | Definition |
| --- | --- | --- |
| `file.lines` | lines | Export parent row total, including code, comment, and blank lines. |
| `file.code` | lines | Export parent row code lines. |
| `module.lines` | lines | Sum of selected parent rows' line totals. Embedded languages contribute no separate rows. |
| `module.files` | files | Count of selected parent files grouped by module. |

Version 1 uses no measurement thresholds. RIPR's current `module-health`
counts newline characters in Rust and TypeScript files and applies a default
`>= 2000` line flag. It separately applies `>= 12` to a heuristic union of
implementation targets and public item prefixes. Those signals and thresholds
remain RIPR-specific. Tokmd's file lines and the RIPR newline count have
different definitions.

## Ownership at the RIPR source snapshot

| Decision or action | Owner | Source or disposition |
| --- | --- | --- |
| File and module inventory, size, and generic complexity, duplication, dependency, risk/effort, changed-surface and review-map observations | tokmd | This proof exercises inventory and size. The other families need their own definitions before comparison. |
| `module-health` responsibility heuristic and thresholds, capability counts, mutation and evidence semantics | RIPR | [Module health source](https://github.com/EffortlessMetrics/ripr-swarm/blob/bcc5e7d98de8c2976b32c45c547c5ece49b495aa/xtask/src/reports/module_health.rs) and [metrics implementation](https://github.com/EffortlessMetrics/ripr-swarm/blob/bcc5e7d98de8c2976b32c45c547c5ece49b495aa/xtask/src/main.rs#L7846). |
| `shape` formatting, tracked allowlist sorting, and its action report | RIPR-local mutation | The [shape implementation](https://github.com/EffortlessMetrics/ripr-swarm/blob/bcc5e7d98de8c2976b32c45c547c5ece49b495aa/xtask/src/main.rs#L2755) records formatting and sorting outcomes. |
| Merge/readiness policy | cockpitctl | Composition over evidence, as assigned in the RIPR program. |
| Workflow and orchestration decisions | Flow Studio | Orchestration authority, as assigned in the RIPR program. |

RIPR source was inspected at [bcc5e7d98de8](https://github.com/EffortlessMetrics/ripr-swarm/commit/bcc5e7d98de8c2976b32c45c547c5ece49b495aa).

## Inputs

Run from the tokmd workbench with a source-built tokmd binary and a clean,
disposable RIPR checkout at the specified exact remote head.

```bash
cargo build --locked -p tokmd
python3 scripts/ripr_metric_profile_proof.py target/debug/tokmd /path/to/ripr <full-head>
```

The selected roots are:

```text
crates/ripr/src
editors/vscode/src
xtask/src
```

## Outputs

The proof emits a deterministic JSON record with repository remote and HEAD,
tokmd version and binary SHA-256, profile and exclusion SHA-256, and selected
file and module identities.

## Proof Requirements

The proof verifies every tracked regular file under the roots against its Git
blob before and after the measurements. It rejects absent files even when
Git status reports a clean checkout, as exercised by a skip-worktree control.
The proof repeats both CLI calls after removing their generation
timestamps and verifies module aggregation against export. The two-commit
corpus checks category exclusions independently, an exact rename, deletion,
a file split, and expected repository-relative file and module names.

## Compatibility

The JSON is local proof output. Receipt implementation, full legacy inventory,
cross-platform execution, normalized parity, and cutover remain open.
