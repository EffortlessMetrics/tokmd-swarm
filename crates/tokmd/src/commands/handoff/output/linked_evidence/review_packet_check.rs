//! Review packet verifier receipt summary.

use serde_json::Value;

const MAX_VERIFIED_PROOF_ARTIFACTS: usize = 5;

pub(in crate::commands::handoff) struct ReviewPacketCheckSummary {
    pub(in crate::commands::handoff) ok: Option<bool>,
    pub(in crate::commands::handoff) artifact_count: Option<u64>,
    pub(in crate::commands::handoff) hashes_verified: Option<u64>,
    pub(in crate::commands::handoff) verified_proof_artifact_count: usize,
    pub(in crate::commands::handoff) first_verified_proof_artifacts: Vec<VerifiedProofArtifact>,
}

pub(in crate::commands::handoff) struct VerifiedProofArtifact {
    pub(in crate::commands::handoff) path: String,
    pub(in crate::commands::handoff) schema: Option<String>,
    pub(in crate::commands::handoff) media_type: Option<String>,
}

pub(super) fn summarize(value: &Value) -> ReviewPacketCheckSummary {
    let proof_artifacts = verified_proof_artifacts(value);
    let verified_proof_artifact_count = proof_artifacts.len();

    ReviewPacketCheckSummary {
        ok: value.get("ok").and_then(Value::as_bool),
        artifact_count: value.get("artifact_count").and_then(Value::as_u64),
        hashes_verified: value.get("hashes_verified").and_then(Value::as_u64),
        verified_proof_artifact_count,
        first_verified_proof_artifacts: proof_artifacts
            .into_iter()
            .take(MAX_VERIFIED_PROOF_ARTIFACTS)
            .collect(),
    }
}

pub(super) fn render(out: &mut String, check: &ReviewPacketCheckSummary) {
    out.push_str("- Review packet verifier:");
    if let Some(ok) = check.ok {
        out.push_str(&format!(" ok={ok}"));
    } else {
        out.push_str(" ok=unknown");
    }
    if let Some(artifact_count) = check.artifact_count {
        out.push_str(&format!(", artifacts={artifact_count}"));
    }
    if let Some(hashes_verified) = check.hashes_verified {
        out.push_str(&format!(", hashes_verified={hashes_verified}"));
    }
    out.push('\n');
    if check.verified_proof_artifact_count > 0 {
        out.push_str(&format!(
            "  - Verified packet-local proof artifact(s): {}\n",
            check.verified_proof_artifact_count
        ));
        for artifact in &check.first_verified_proof_artifacts {
            out.push_str("    - `");
            out.push_str(&artifact.path);
            out.push('`');
            let mut metadata = Vec::new();
            if let Some(schema) = &artifact.schema {
                metadata.push(schema.as_str());
            }
            if let Some(media_type) = &artifact.media_type {
                metadata.push(media_type.as_str());
            }
            if !metadata.is_empty() {
                out.push_str(" (");
                out.push_str(&metadata.join(", "));
                out.push(')');
            }
            out.push('\n');
        }
        if check.verified_proof_artifact_count > check.first_verified_proof_artifacts.len() {
            out.push_str(&format!(
                "    - ... {} more verified packet-local proof artifact(s); open the verifier receipt for the full list.\n",
                check.verified_proof_artifact_count - check.first_verified_proof_artifacts.len()
            ));
        }
        out.push_str("  - Verified packet-local proof artifacts are hash/inventory evidence, not proof execution.\n");
    }
}

fn verified_proof_artifacts(value: &Value) -> Vec<VerifiedProofArtifact> {
    value
        .get("artifacts")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(verified_proof_artifact)
        .collect()
}

fn verified_proof_artifact(value: &Value) -> Option<VerifiedProofArtifact> {
    let path = value.get("path").and_then(Value::as_str)?;
    if !path.starts_with("proof/") {
        return None;
    }

    Some(VerifiedProofArtifact {
        path: path.to_string(),
        schema: value
            .get("schema")
            .and_then(Value::as_str)
            .map(str::to_string),
        media_type: value
            .get("media_type")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_packet_check_summary_lists_verified_packet_local_proof_artifacts() {
        let value = serde_json::json!({
            "schema": "tokmd.review_packet_check.v1",
            "ok": true,
            "artifact_count": 7,
            "hashes_verified": 7,
            "artifacts": [
                {
                    "id": "cockpit",
                    "path": "cockpit.json",
                    "schema": "tokmd.cockpit_receipt.v3",
                    "media_type": "application/json",
                    "hash_algo": "blake3",
                    "hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                },
                {
                    "id": "proof-route",
                    "path": "proof/proof-pack-route.json",
                    "schema": "tokmd.proof_pack_route.v1",
                    "media_type": "application/json",
                    "hash_algo": "blake3",
                    "hash": "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
                }
            ]
        });

        let summary = summarize(&value);

        assert_eq!(summary.verified_proof_artifact_count, 1);
        assert_eq!(
            summary.first_verified_proof_artifacts[0].path,
            "proof/proof-pack-route.json"
        );
        assert_eq!(
            summary.first_verified_proof_artifacts[0].schema.as_deref(),
            Some("tokmd.proof_pack_route.v1")
        );
        assert_eq!(
            summary.first_verified_proof_artifacts[0]
                .media_type
                .as_deref(),
            Some("application/json")
        );

        let mut out = String::new();
        render(&mut out, &summary);

        assert!(out.contains("Review packet verifier: ok=true, artifacts=7"));
        assert!(out.contains("Verified packet-local proof artifact(s): 1"));
        assert!(out.contains(
            "`proof/proof-pack-route.json` (tokmd.proof_pack_route.v1, application/json)"
        ));
        assert!(out.contains("not proof execution"));
    }
}
