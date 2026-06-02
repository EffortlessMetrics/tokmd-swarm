//! Proof evidence normalization, execution status, and freshness classification.

use std::path::{Path, PathBuf};

use tokmd_types::cockpit::CommitMatch;

use super::artifacts::ProofEvidenceArtifact;
use super::model::{
    NormalizedProofEvidence, ProofEvidenceAvailability, ProofEvidenceInput, ProofEvidenceKind,
    ProofExecutionStatus,
};
use super::status::{
    availability_for, availability_with_commit_match, coverage_availability,
    proof_run_entry_status, scope_status,
};

pub(crate) fn normalize_proof_evidence_inputs(
    inputs: &[ProofEvidenceInput],
    cockpit_base: Option<&str>,
    cockpit_head: Option<&str>,
) -> Vec<NormalizedProofEvidence> {
    inputs
        .iter()
        .flat_map(|input| {
            normalize_proof_evidence(
                &input.artifact,
                input.source_path.clone(),
                cockpit_base,
                cockpit_head,
            )
        })
        .collect()
}

pub(crate) fn normalize_proof_evidence(
    artifact: &ProofEvidenceArtifact,
    source_path: impl Into<PathBuf>,
    cockpit_base: Option<&str>,
    cockpit_head: Option<&str>,
) -> Vec<NormalizedProofEvidence> {
    let source_path = source_path.into();
    let source_ref = normalize_path_for_ref(&source_path);
    let commit_match = classify_commit_match(
        artifact_base(artifact),
        artifact.head(),
        cockpit_base,
        cockpit_head,
    );

    match artifact {
        ProofEvidenceArtifact::ProofRunSummary(summary) => summary
            .entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let execution_status = proof_run_entry_status(entry);
                let availability = availability_for(execution_status, commit_match);
                NormalizedProofEvidence {
                    source_path: source_path.clone(),
                    source_schema: summary.schema.clone(),
                    kind: ProofEvidenceKind::ProofRunSummary,
                    profile: Some(summary.profile.clone()),
                    scope: Some(entry.scope.clone()),
                    command: Some(entry.command.clone()),
                    required: entry.required,
                    advisory: !entry.required,
                    execution_status,
                    availability,
                    commit_match,
                    run_id: None,
                    run_attempt: None,
                    run_url: None,
                    workflow: None,
                    event_name: None,
                    ref_name: None,
                    artifact_refs: vec![format!("{source_ref}#/entries/{idx}")],
                }
            })
            .collect(),
        ProofEvidenceArtifact::ProofRunObservation(observation) => observation
            .scopes
            .iter()
            .enumerate()
            .map(|(idx, scope)| {
                let execution_status = scope_status(&scope.status, scope.exit_code);
                let availability = availability_for(execution_status, commit_match);
                let required = observation.counts.required_planned > 0;
                NormalizedProofEvidence {
                    source_path: source_path.clone(),
                    source_schema: observation.schema.clone(),
                    kind: ProofEvidenceKind::ProofRunObservation,
                    profile: Some(observation.profile.clone()),
                    scope: Some(scope.name.clone()),
                    command: Some(scope.command.clone()),
                    required,
                    advisory: !required,
                    execution_status,
                    availability,
                    commit_match,
                    run_id: None,
                    run_attempt: None,
                    run_url: None,
                    workflow: None,
                    event_name: None,
                    ref_name: None,
                    artifact_refs: vec![format!("{source_ref}#/scopes/{idx}")],
                }
            })
            .collect(),
        ProofEvidenceArtifact::ProofExecutorObservation(observation) => observation
            .scopes
            .iter()
            .enumerate()
            .map(|(idx, scope)| {
                let execution_status = scope_status(&scope.status, scope.exit_code);
                let availability = availability_for(execution_status, commit_match);
                NormalizedProofEvidence {
                    source_path: source_path.clone(),
                    source_schema: observation.schema.clone(),
                    kind: ProofEvidenceKind::ProofExecutorObservation,
                    profile: Some(observation.profile.clone()),
                    scope: Some(scope.name.clone()),
                    command: Some(scope.command.clone()),
                    required: observation.required,
                    advisory: !observation.required,
                    execution_status,
                    availability,
                    commit_match,
                    run_id: None,
                    run_attempt: None,
                    run_url: None,
                    workflow: None,
                    event_name: None,
                    ref_name: None,
                    artifact_refs: vec![format!("{source_ref}#/scopes/{idx}")],
                }
            })
            .collect(),
        ProofEvidenceArtifact::CoverageReceipt(receipt) => {
            let execution_status = if receipt.status.ok {
                ProofExecutionStatus::ExecutedPassed
            } else {
                ProofExecutionStatus::ExecutedFailed
            };
            let base_availability = coverage_availability(receipt);
            let availability = availability_with_commit_match(base_availability, commit_match);
            let artifact_refs = receipt
                .artifacts
                .iter()
                .enumerate()
                .map(|(idx, _)| format!("{source_ref}#/artifacts/{idx}"))
                .collect();

            vec![NormalizedProofEvidence {
                source_path,
                source_schema: receipt.schema.clone(),
                kind: ProofEvidenceKind::CoverageReceipt,
                profile: None,
                scope: Some(receipt.flag.clone()),
                command: None,
                required: false,
                advisory: true,
                execution_status,
                availability,
                commit_match,
                run_id: non_empty_string(receipt.github.run_id.as_deref()),
                run_attempt: non_empty_string(receipt.github.run_attempt.as_deref()),
                run_url: github_run_url(&receipt.repo, receipt.github.run_id.as_deref()),
                workflow: non_empty_string(Some(&receipt.workflow)),
                event_name: non_empty_string(receipt.github.event_name.as_deref()),
                ref_name: non_empty_string(receipt.github.ref_name.as_deref()),
                artifact_refs,
            }]
        }
        ProofEvidenceArtifact::ProofPackRoute(route) => {
            let availability =
                availability_with_commit_match(ProofEvidenceAvailability::Available, commit_match);
            vec![NormalizedProofEvidence {
                source_path,
                source_schema: route.schema.clone(),
                kind: ProofEvidenceKind::ProofPackRoute,
                profile: None,
                scope: Some("proof_pack_route".to_string()),
                command: Some(
                    "cargo xtask ci-plan --route-json-out target/ci/proof-pack-route.json"
                        .to_string(),
                ),
                required: false,
                advisory: true,
                execution_status: ProofExecutionStatus::Planned,
                availability,
                commit_match,
                run_id: None,
                run_attempt: None,
                run_url: None,
                workflow: None,
                event_name: None,
                ref_name: None,
                artifact_refs: vec![format!("{source_ref}#/summary")],
            }]
        }
    }
}

fn artifact_base(artifact: &ProofEvidenceArtifact) -> Option<&str> {
    match artifact {
        ProofEvidenceArtifact::ProofRunSummary(artifact) => Some(&artifact.base),
        ProofEvidenceArtifact::ProofRunObservation(artifact) => Some(&artifact.base),
        ProofEvidenceArtifact::ProofExecutorObservation(artifact) => Some(&artifact.base),
        ProofEvidenceArtifact::CoverageReceipt(_) => None,
        ProofEvidenceArtifact::ProofPackRoute(artifact) => Some(&artifact.base),
    }
}

fn classify_commit_match(
    artifact_base: Option<&str>,
    artifact_head: Option<&str>,
    cockpit_base: Option<&str>,
    cockpit_head: Option<&str>,
) -> CommitMatch {
    let artifact_head = non_empty(artifact_head);
    let cockpit_head = non_empty(cockpit_head);

    match (artifact_head, cockpit_head) {
        (Some(artifact_head), Some(cockpit_head))
            if commitish_ref(artifact_head) && commitish_ref(cockpit_head) =>
        {
            if artifact_head == cockpit_head {
                CommitMatch::Exact
            } else {
                CommitMatch::Stale
            }
        }
        _ if non_empty(artifact_base).is_some()
            || artifact_head.is_some()
            || non_empty(cockpit_base).is_some()
            || cockpit_head.is_some() =>
        {
            CommitMatch::Partial
        }
        _ => CommitMatch::Unknown,
    }
}

fn commitish_ref(value: &str) -> bool {
    let value = value.trim();
    value.len() >= 6 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn non_empty_string(value: Option<&str>) -> Option<String> {
    non_empty(value).map(ToOwned::to_owned)
}

fn github_run_url(repo: &str, run_id: Option<&str>) -> Option<String> {
    let repo = non_empty(Some(repo))?;
    let run_id = non_empty(run_id)?;

    if !valid_github_repo(repo) || !run_id.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    Some(format!("https://github.com/{repo}/actions/runs/{run_id}"))
}

fn valid_github_repo(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let Some(owner) = parts.next() else {
        return false;
    };
    let Some(name) = parts.next() else {
        return false;
    };

    parts.next().is_none() && valid_github_segment(owner) && valid_github_segment(name)
}

fn valid_github_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn normalize_path_for_ref(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tokmd_types::cockpit::CommitMatch;

    use super::*;
    use crate::proof_evidence::fixtures::{
        coverage_receipt_artifact, proof_executor_observation_artifact, proof_pack_route_artifact,
        proof_run_observation_artifact, proof_run_summary_artifact,
    };
    use crate::proof_evidence::model::ProofEvidenceAvailability;

    fn single_evidence(
        artifact: &ProofEvidenceArtifact,
        source_path: &str,
        cockpit_head: Option<&str>,
    ) -> NormalizedProofEvidence {
        let mut evidence = normalize_proof_evidence(
            artifact,
            PathBuf::from(source_path),
            Some("origin/main"),
            cockpit_head,
        );
        assert_eq!(evidence.len(), 1);
        evidence.pop().expect("normalized evidence")
    }

    #[test]
    fn normalizes_proof_run_summary_as_required_exact_evidence() {
        let artifact = proof_run_summary_artifact("abc123");
        let evidence = single_evidence(&artifact, "proof-run-summary.json", Some("abc123"));

        assert_eq!(evidence.kind, ProofEvidenceKind::ProofRunSummary);
        assert_eq!(evidence.profile.as_deref(), Some("fast"));
        assert_eq!(evidence.scope.as_deref(), Some("tokmd_cockpit"));
        assert_eq!(
            evidence.command.as_deref(),
            Some("cargo test -p tokmd-cockpit")
        );
        assert!(evidence.required);
        assert!(!evidence.advisory);
        assert_eq!(
            evidence.execution_status,
            ProofExecutionStatus::ExecutedPassed
        );
        assert_eq!(evidence.availability, ProofEvidenceAvailability::Available);
        assert_eq!(evidence.commit_match, CommitMatch::Exact);
    }

    #[test]
    fn normalizes_proof_run_observation_scope_as_required_evidence() {
        let artifact = proof_run_observation_artifact("abc123");
        let evidence = single_evidence(&artifact, "proof-run-observation.json", Some("abc123"));

        assert_eq!(evidence.kind, ProofEvidenceKind::ProofRunObservation);
        assert_eq!(evidence.scope.as_deref(), Some("tokmd_cockpit"));
        assert!(evidence.required);
        assert_eq!(
            evidence.execution_status,
            ProofExecutionStatus::ExecutedPassed
        );
        assert_eq!(evidence.availability, ProofEvidenceAvailability::Available);
    }

    #[test]
    fn symbolic_proof_observation_head_is_partial_not_exact() {
        let artifact = proof_run_observation_artifact("HEAD");
        let evidence = single_evidence(&artifact, "proof-run-observation.json", Some("HEAD"));

        assert_eq!(evidence.kind, ProofEvidenceKind::ProofRunObservation);
        assert_eq!(
            evidence.execution_status,
            ProofExecutionStatus::ExecutedPassed
        );
        assert_eq!(evidence.commit_match, CommitMatch::Partial);
        assert_eq!(evidence.availability, ProofEvidenceAvailability::Degraded);
    }

    #[test]
    fn normalizes_executor_dry_run_as_advisory_skipped_evidence() {
        let artifact = proof_executor_observation_artifact("abc123");
        let evidence = single_evidence(
            &artifact,
            "proof/proof-executor-observation.json",
            Some("abc123"),
        );

        assert_eq!(evidence.kind, ProofEvidenceKind::ProofExecutorObservation);
        assert_eq!(evidence.profile.as_deref(), Some("affected"));
        assert_eq!(evidence.scope.as_deref(), Some("tokmd_cockpit"));
        assert!(!evidence.required);
        assert!(evidence.advisory);
        assert_eq!(evidence.execution_status, ProofExecutionStatus::DryRun);
        assert_eq!(evidence.availability, ProofEvidenceAvailability::Skipped);
        assert_eq!(
            evidence.artifact_refs,
            vec!["proof/proof-executor-observation.json#/scopes/0"]
        );
    }

    #[test]
    fn normalizes_coverage_receipt_as_advisory_artifact_evidence() {
        let artifact = coverage_receipt_artifact("abc123", true, true);
        let evidence = single_evidence(&artifact, "proof/coverage-receipt.json", Some("abc123"));

        assert_eq!(evidence.kind, ProofEvidenceKind::CoverageReceipt);
        assert_eq!(evidence.scope.as_deref(), Some("tokmd_cockpit"));
        assert!(!evidence.required);
        assert!(evidence.advisory);
        assert_eq!(
            evidence.execution_status,
            ProofExecutionStatus::ExecutedPassed
        );
        assert_eq!(evidence.availability, ProofEvidenceAvailability::Available);
        assert_eq!(
            evidence.artifact_refs,
            vec!["proof/coverage-receipt.json#/artifacts/0"]
        );
        assert_eq!(evidence.run_id.as_deref(), Some("12345"));
        assert_eq!(evidence.run_attempt.as_deref(), Some("1"));
        assert_eq!(
            evidence.run_url.as_deref(),
            Some("https://github.com/EffortlessMetrics/tokmd/actions/runs/12345")
        );
        assert_eq!(evidence.workflow.as_deref(), Some("Coverage"));
        assert_eq!(evidence.event_name.as_deref(), Some("pull_request"));
        assert_eq!(evidence.ref_name.as_deref(), Some("feature"));
    }

    #[test]
    fn normalizes_proof_pack_route_as_advisory_planned_routing_evidence() {
        let artifact = proof_pack_route_artifact("abc1234");
        let evidence = single_evidence(&artifact, "proof/proof-pack-route.json", Some("abc1234"));

        assert_eq!(evidence.kind, ProofEvidenceKind::ProofPackRoute);
        assert_eq!(evidence.scope.as_deref(), Some("proof_pack_route"));
        assert!(!evidence.required);
        assert!(evidence.advisory);
        assert_eq!(evidence.execution_status, ProofExecutionStatus::Planned);
        assert_eq!(evidence.availability, ProofEvidenceAvailability::Available);
        assert_eq!(evidence.commit_match, CommitMatch::Exact);
        assert_eq!(
            evidence.command.as_deref(),
            Some("cargo xtask ci-plan --route-json-out target/ci/proof-pack-route.json")
        );
        assert_eq!(
            evidence.artifact_refs,
            vec!["proof/proof-pack-route.json#/summary"]
        );
    }

    #[test]
    fn symbolic_proof_pack_route_head_is_partial_not_exact() {
        let artifact = proof_pack_route_artifact("HEAD");
        let evidence = single_evidence(&artifact, "proof/proof-pack-route.json", Some("HEAD"));

        assert_eq!(evidence.kind, ProofEvidenceKind::ProofPackRoute);
        assert_eq!(evidence.execution_status, ProofExecutionStatus::Planned);
        assert_eq!(evidence.commit_match, CommitMatch::Partial);
        assert_eq!(evidence.availability, ProofEvidenceAvailability::Degraded);
    }

    #[test]
    fn stale_commit_marks_otherwise_available_evidence_stale() {
        let artifact = coverage_receipt_artifact("abc123", true, true);
        let evidence = single_evidence(&artifact, "coverage-receipt.json", Some("def456"));

        assert_eq!(evidence.commit_match, CommitMatch::Stale);
        assert_eq!(evidence.availability, ProofEvidenceAvailability::Stale);
    }

    #[test]
    fn unknown_commit_does_not_become_available_evidence() {
        let artifact = coverage_receipt_artifact("", true, true);
        let mut evidence = normalize_proof_evidence(
            &artifact,
            PathBuf::from("proof/coverage-receipt.json"),
            None,
            None,
        );
        assert_eq!(evidence.len(), 1);
        let evidence = evidence.pop().expect("normalized evidence");

        assert_eq!(evidence.commit_match, CommitMatch::Unknown);
        assert_eq!(evidence.availability, ProofEvidenceAvailability::Degraded);
    }

    #[test]
    fn github_run_url_requires_safe_repo_and_numeric_run_id() {
        assert_eq!(
            github_run_url("EffortlessMetrics/tokmd", Some("12345")).as_deref(),
            Some("https://github.com/EffortlessMetrics/tokmd/actions/runs/12345")
        );
        assert_eq!(github_run_url("EffortlessMetrics", Some("12345")), None);
        assert_eq!(
            github_run_url("EffortlessMetrics/tokmd", Some("run-12345")),
            None
        );
        assert_eq!(
            github_run_url("EffortlessMetrics/tokmd?x=1", Some("12345")),
            None
        );
    }
}
