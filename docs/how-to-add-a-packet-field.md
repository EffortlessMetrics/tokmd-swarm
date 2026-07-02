# How To Add an Evidence Packet Field

Status: contributor extender guide for the `sensors/tokmd/manifest.json`
evidence packet.

Use this guide when you want to add a field to the PR evidence packet manifest
(`tokmd.evidence-packet/v1`). It explains where the contract actually lives, the
two kinds of field change, and the proof a contributor PR should run. It does
not restate the full field table; that lives in the
[evidence packet contract](evidence-packet.md).

For the broader "first useful contribution" flow, start from the
[contributor guide](contributor-guide.md).

## What "A Packet Field" Means

The evidence packet has three places a field can live:

| Field location | Example fields | Owner type |
| --- | --- | --- |
| Manifest top level | `preset`, `status`, `warnings`, `reproduce` | `EvidencePacketManifest` |
| `artifacts` object | `analyze_md`, `analyze_json`, `syntax_json` | `EvidencePacketArtifacts` |
| `review_priority` item | `rank`, `category`, `severity`, `score` | `EvidencePacketReviewPriorityItem` |

All three are serde structs in one DTO file. Pick the struct that matches the
meaning of your field instead of overloading an existing field.

## Where The Contract Lives

A packet field is owned by code, mirrored in docs and schema, and proven by
tests. Touch these together:

| Surface | Path | Role |
| --- | --- | --- |
| DTO struct | `crates/tokmd-types/src/evidence_packet.rs` | Declares the field and its serde behavior. |
| Manifest producer | `crates/tokmd/src/commands/evidence_packet.rs` | Populates the field in `build_manifest`. |
| Field reference | `docs/evidence-packet.md` | Human-facing field table and example. |
| JSON Schema | `docs/evidence-packet.schema.json` | Draft-07 validation for the manifest. |
| Workflow spec | `docs/specs/evidence-packet-workflow.md` | Normative contract; owns compatibility rules. |
| Integration test | `crates/tokmd/tests/evidence_packet_integration.rs` | End-to-end producer/verifier proof. |

The schema constant `EVIDENCE_PACKET_SCHEMA` (`tokmd.evidence-packet/v1`) and the
DTO roundtrip tests both live in the DTO file.

## Two Kinds Of Change

### A. Additive optional field (backward compatible)

This is the common, low-risk case. An optional field that defaults to absent
does not break existing consumers, and the JSON Schema already sets
`"additionalProperties": true`, so it validates without a schema bump.

Keep it backward compatible:

- make the field `Option<T>` or a collection that defaults to empty;
- annotate it with `#[serde(default, skip_serializing_if = "...")]` so absent
  values are not serialized and old manifests still deserialize;
- do not change the meaning of any existing required field;
- do not add the field to the schema `required` list.

The contract already permits this. From
[evidence-packet.md](evidence-packet.md): "Producers may add fields when they do
not change the meaning of required fields. Consumers should ignore unknown
fields and fail closed when required fields are missing."

### B. Required field, renamed field, or changed meaning (compatibility break)

Adding a required field, renaming one, removing one, or changing what an
existing field means is a contract change. The spec requires it in one PR:
update `docs/specs/evidence-packet-workflow.md`, update
`docs/evidence-packet.schema.json` (including its `required` list), and update
`crates/tokmd/tests/evidence_packet_integration.rs`. A break that old consumers
cannot ignore needs a new schema identifier (for example a `v2`), not a silent
edit to `tokmd.evidence-packet/v1`.

When in doubt, prefer shape A.

## Steps For An Additive Optional Field

1. Add the field to the matching struct in
   `crates/tokmd-types/src/evidence_packet.rs`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub source_tokmd_commit: Option<String>,
```

   (`source_tokmd_commit` is an illustrative example; choose a deterministic,
   meaningful name. Avoid wall-clock timestamps so manifests stay reproducible.)

2. Populate it in `build_manifest` in
   `crates/tokmd/src/commands/evidence_packet.rs`, alongside the other field
   assignments in the returned `EvidencePacketManifest`. Keep the value
   deterministic and derived from inputs already available to the producer.

3. Document it in the field table and example in `docs/evidence-packet.md` so
   consumers learn what it means.

4. Optionally add a `properties` entry in `docs/evidence-packet.schema.json`.
   Add it under `properties` only; do **not** add it to `required`, or you turn
   an additive change into a compatibility break.

5. Extend the DTO roundtrip test in
   `crates/tokmd-types/src/evidence_packet.rs` so the new field serializes and
   deserializes, and assert it in the manifest assertions in
   `crates/tokmd/tests/evidence_packet_integration.rs`.

## Compatibility Rules

From `docs/specs/evidence-packet-workflow.md`:

- Additive optional fields under `tokmd.evidence-packet/v1` keep existing
  consumers working; they must ignore unknown fields.
- Changes that alter required artifact names, status semantics, support-model
  boundaries, or verifier failure modes must update the spec, the schema when
  needed, and the integration test in the same PR.
- Do not change the meaning of `complete`, `partial`, or `failed`, or the set of
  required artifacts, without treating it as a contract change.

## Proof

Run the proof that matches the surfaces you touched. For an additive field that
touches the DTO, producer, docs, and schema:

```bash
cargo test -p tokmd-types evidence_packet
cargo test -p tokmd --test evidence_packet_integration
cargo xtask docs --check
cargo xtask doc-artifacts --check
cargo xtask affected --base origin/main --head HEAD --json-output target/proof/affected.json
cargo xtask proof --profile affected --base origin/main --head HEAD --plan --plan-json target/proof/proof-plan.json
git diff --check
```

State the claim boundary in the PR body: an additive packet field proves the new
field serializes, deserializes, and is produced deterministically. It does not
change packet status semantics, the required-artifact set, or what the packet
proves about safety, correctness, or merge readiness.

## Related Docs

- [Evidence packet contract](evidence-packet.md) — field reference and example
- [Evidence packet workflow spec](specs/evidence-packet-workflow.md) — normative
  contract and compatibility rules
- [Packet consumption guide](packet-consumption.md) — how consumers read the
  manifest and evidence states
- [Contributor guide](contributor-guide.md) — first-contribution flow and proof
  commands
