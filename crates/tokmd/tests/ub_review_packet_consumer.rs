//! Consumer-side contract smoke for the ub-review evidence packet status
//! taxonomy.
//!
//! The producer side (`tokmd evidence-packet` / `tokmd packet generate`) is
//! covered by `evidence_packet_integration.rs`, which runs the binary and
//! asserts the manifests it writes. This file instead exercises the
//! **consumer** contract documented in `docs/ub-review-integration.md`: given a
//! `sensors/tokmd/manifest.json`, an ub-review lane must read two independent
//! axes (packet `status` and per-artifact advisory state) and follow a fixed
//! trust order. These tests pin each documented taxonomy row against both the
//! published JSON schema (`schemas/evidence-packet.schema.json`) and the public
//! `tokmd_types::EvidencePacketManifest` type, so a drift in the schema, the
//! type, or the canonical examples is caught.
//!
//! Pure parse/validate/assert: no binary, git, or feature gate required.

use std::error::Error;

use serde_json::{Value, json};
use tokmd_types::{EVIDENCE_PACKET_SCHEMA, EvidencePacketManifest, EvidencePacketStatus};

type TestResult = Result<(), Box<dyn Error>>;

const SCHEMA_JSON: &str = include_str!("../schemas/evidence-packet.schema.json");

/// Validate a manifest value against the published evidence-packet schema,
/// returning the collected violations on failure instead of panicking.
fn validate_against_schema(manifest: &Value) -> TestResult {
    let schema: Value = serde_json::from_str(SCHEMA_JSON)?;
    let validator = jsonschema::validator_for(&schema)?;
    if validator.is_valid(manifest) {
        return Ok(());
    }
    let errors: Vec<String> = validator
        .iter_errors(manifest)
        .map(|err| format!("{err} at {}", err.instance_path()))
        .collect();
    Err(format!("manifest failed schema validation:\n{}", errors.join("\n")).into())
}

/// Round-trip a manifest value through the published schema and the public
/// consumer type, returning the typed manifest.
fn consume(manifest: &Value) -> Result<EvidencePacketManifest, Box<dyn Error>> {
    validate_against_schema(manifest)?;
    Ok(serde_json::from_value(manifest.clone())?)
}

/// Encodes the documented consumer gate: read `status` + `errors` first; a
/// `failed` status or any non-empty `errors` means the evidence is invalid and
/// must not be attached as a valid-looking packet (see the status taxonomy and
/// "what to trust first" sections of `docs/ub-review-integration.md`).
fn is_attachable(manifest: &EvidencePacketManifest) -> bool {
    manifest.status != EvidencePacketStatus::Failed && manifest.errors.is_empty()
}

fn base_artifacts(with_syntax: bool) -> Value {
    let mut artifacts = serde_json::Map::new();
    artifacts.insert("analyze_md".to_string(), json!("sensors/tokmd/analyze.md"));
    artifacts.insert(
        "analyze_json".to_string(),
        json!("sensors/tokmd/analyze.json"),
    );
    artifacts.insert("context_md".to_string(), json!("sensors/tokmd/context.md"));
    if with_syntax {
        artifacts.insert(
            "syntax_json".to_string(),
            json!("sensors/tokmd/syntax.json"),
        );
    }
    Value::Object(artifacts)
}

fn manifest_skeleton(status: &str, artifacts: Value, warnings: Value, errors: Value) -> Value {
    json!({
        "schema": EVIDENCE_PACKET_SCHEMA,
        "tokmd_version": "1.14.0",
        "preset": "bun-ub",
        "base": "origin/main",
        "head": "HEAD",
        "paths": ["src/runtime/api/MarkdownObject.rs"],
        "status": status,
        "artifacts": artifacts,
        "warnings": warnings,
        "errors": errors,
        "non_claims": [
            "bun-ub packages review evidence; it does not prove UB exists or is absent"
        ],
        "reproduce": [
            "tokmd packet generate --base origin/main --head HEAD src/runtime/api/MarkdownObject.rs"
        ],
    })
}

#[test]
fn complete_packet_is_attachable_with_skipped_optional_signal() -> TestResult {
    // Row 1: required artifacts exist, errors empty -> attach and review.
    // No `syntax_json` is the "skipped" advisory state (optional signal not
    // requested), which must keep the packet `complete`, not a failure.
    let manifest = manifest_skeleton("complete", base_artifacts(false), json!([]), json!([]));
    let packet = consume(&manifest)?;

    assert_eq!(packet.status, EvidencePacketStatus::Complete);
    assert!(
        packet.errors.is_empty(),
        "complete packet must carry no errors"
    );
    assert!(
        !packet.artifacts.analyze_md.is_empty()
            && !packet.artifacts.analyze_json.is_empty()
            && !packet.artifacts.context_md.is_empty(),
        "required artifact references must be present"
    );
    assert!(
        packet.artifacts.syntax_json.is_none(),
        "skipped optional signal: syntax_json absent"
    );
    assert!(
        is_attachable(&packet),
        "complete packet is valid review evidence"
    );
    Ok(())
}

#[test]
fn partial_packet_surfaces_advisory_missing_as_named_limit() -> TestResult {
    // Row 2 + advisory-missing: an optional signal was requested for this run
    // but is absent, so the packet degrades to `partial` with a named warning.
    // It stays attachable (exit 0); the limit is named, not a failure.
    let warnings = json!(["optional artifact syntax_json missing for this run"]);
    let manifest = manifest_skeleton("partial", base_artifacts(true), warnings, json!([]));
    let packet = consume(&manifest)?;

    assert_eq!(packet.status, EvidencePacketStatus::Partial);
    assert!(
        packet.errors.is_empty(),
        "partial is a bounded limit, not invalid evidence"
    );
    assert!(
        !packet.warnings.is_empty(),
        "partial status must be explained by at least one named warning"
    );
    assert!(
        packet
            .warnings
            .iter()
            .any(|w| w.contains("syntax_json missing")),
        "advisory-missing must name the absent optional artifact: {:?}",
        packet.warnings
    );
    assert!(
        is_attachable(&packet),
        "partial packet is still attachable evidence"
    );
    Ok(())
}

#[test]
fn failed_packet_is_rejected_not_attached() -> TestResult {
    // Row 3: a required artifact is missing (or refs/parse failed). The manifest
    // is still written (schema-valid) for inspection, but the consumer must NOT
    // attach a valid-looking packet: status `failed` and non-empty `errors`.
    let errors = json!(["required artifact analyze_md missing: sensors/tokmd/analyze.md"]);
    let manifest = manifest_skeleton("failed", base_artifacts(false), json!([]), errors);
    let packet = consume(&manifest)?;

    assert_eq!(packet.status, EvidencePacketStatus::Failed);
    assert!(
        !packet.errors.is_empty(),
        "failed status must record at least one error for the consumer"
    );
    assert!(
        !is_attachable(&packet),
        "failed packet must be rejected, regenerated, or omitted"
    );
    Ok(())
}

#[test]
fn trust_order_rejects_any_nonempty_errors_regardless_of_status() -> TestResult {
    // "Read the manifest top-down ... stop and reject early if an upstream field
    // invalidates the packet." Any non-empty `errors` invalidates the evidence
    // even if a (malformed) producer left `status` more optimistic.
    let errors = json!(["analyze.json could not be parsed"]);
    let manifest = manifest_skeleton("complete", base_artifacts(false), json!([]), errors);
    let packet = consume(&manifest)?;

    assert!(
        !is_attachable(&packet),
        "non-empty errors must fail the validity gate before status is trusted"
    );
    Ok(())
}

#[test]
fn consumer_confirms_schema_identity_before_interpreting_fields() -> TestResult {
    // Trust-order step 2: confirm `schema` + `tokmd_version` before reading the
    // rest. The published schema pins `schema` to the v1 const; a manifest with
    // a foreign schema id must not validate as a v1 evidence packet.
    let valid = manifest_skeleton("complete", base_artifacts(false), json!([]), json!([]));
    let packet = consume(&valid)?;
    assert_eq!(packet.schema, EVIDENCE_PACKET_SCHEMA);
    assert!(
        !packet.tokmd_version.is_empty(),
        "tokmd_version must be present"
    );

    let mut foreign = valid;
    let foreign_obj = foreign
        .as_object_mut()
        .ok_or("manifest skeleton must be a JSON object")?;
    foreign_obj.insert("schema".to_string(), json!("tokmd.evidence-packet/v2"));
    assert!(
        validate_against_schema(&foreign).is_err(),
        "a non-v1 schema id must be rejected by the v1 schema gate"
    );
    Ok(())
}
