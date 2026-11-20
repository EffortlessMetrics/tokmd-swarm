use super::{Identity, LIMITS, Metrics, OMITTED, POLICY, Snapshot, Source, Symbol};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Serialize)]
struct Occurrence<'a> {
    path: &'a str,
    #[serde(flatten)]
    symbol: &'a Symbol,
}

#[derive(Serialize)]
struct Row<'a> {
    disposition: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    exact_nesting_observation: Option<&'static str>,
    baseline: Vec<Occurrence<'a>>,
    candidate: Vec<Occurrence<'a>>,
}

#[derive(Serialize)]
struct FileScope<'a> {
    path: &'a str,
    module: &'a str,
    baseline_status: &'a str,
    baseline_blob: &'a Option<String>,
    candidate_status: &'a str,
    candidate_blob: &'a Option<String>,
}

#[derive(Serialize)]
pub(super) struct Comparison<'a> {
    schema: &'static str,
    metric_policy: &'static str,
    mode: &'static str,
    package: &'a str,
    language: &'a str,
    baseline_commit: &'a str,
    candidate_commit: &'a str,
    thresholds: Metrics,
    outside_measurement: &'static str,
    files: Vec<FileScope<'a>>,
    rows: Vec<Row<'a>>,
}

type Pairs<'a, K> = BTreeMap<K, (Vec<Occurrence<'a>>, Vec<Occurrence<'a>>)>;

pub(super) fn comparison<'a>(base: &'a Snapshot, candidate: &'a Snapshot) -> Comparison<'a> {
    let mut report = Comparison {
        schema: "tokmd.rust_callable_comparison.v1",
        metric_policy: POLICY,
        mode: "advisory",
        package: base.package,
        language: base.language,
        baseline_commit: &base.commit,
        candidate_commit: &candidate.commit,
        thresholds: LIMITS,
        outside_measurement: OMITTED,
        files: Vec::new(),
        rows: Vec::new(),
    };
    let mut exact: Pairs<(&str, &Identity)> = BTreeMap::new();
    // Both snapshots are made from the same fixed, sorted path policy.
    for (old, new) in base.files.iter().zip(&candidate.files) {
        report.files.push(FileScope {
            path: old.path,
            module: old.module,
            baseline_status: old.status,
            baseline_blob: &old.blob,
            candidate_status: new.status,
            candidate_blob: &new.blob,
        });
        if old.status != "parsed" || new.status != "parsed" {
            report.rows.push(Row {
                exact_nesting_observation: None,
                disposition: match (old.status, new.status) {
                    ("parsed", _) => "lost_measured_file",
                    (_, "parsed") => "newly_measured_file",
                    _ => "unmeasured_file",
                },
                baseline: occurrences(old).collect(),
                candidate: occurrences(new).collect(),
            });
            continue;
        }
        for item in occurrences(old) {
            exact
                .entry((item.path, &item.symbol.id))
                .or_default()
                .0
                .push(item);
        }
        for item in occurrences(new) {
            exact
                .entry((item.path, &item.symbol.id))
                .or_default()
                .1
                .push(item);
        }
    }
    let mut structural: Pairs<&str> = BTreeMap::new();
    for (_, (baseline, candidate)) in exact {
        if baseline.len() > 1 || candidate.len() > 1 {
            report.rows.push(Row {
                exact_nesting_observation: None,
                disposition: "ambiguous_exact_identity",
                baseline,
                candidate,
            });
        } else if let ([old], [new]) = (baseline.as_slice(), candidate.as_slice()) {
            let observation = match (old.symbol.metrics.nesting, new.symbol.metrics.nesting) {
                (Some(_), None) => "lost_measurement",
                (None, Some(_)) => "newly_measured",
                (None, None) => "unmeasured",
                (Some(a), Some(b)) if b < a => "lower_observation",
                (Some(a), Some(b)) if a == b => "unchanged",
                (Some(_), Some(b)) if Some(b) > LIMITS.nesting => "worsened_over_threshold",
                _ => "increased_within_threshold",
            };
            report.rows.push(Row {
                exact_nesting_observation: Some(observation),
                disposition: "exact",
                baseline,
                candidate,
            });
        } else {
            for item in baseline {
                structural
                    .entry(&item.symbol.fingerprint)
                    .or_default()
                    .0
                    .push(item);
            }
            for item in candidate {
                structural
                    .entry(&item.symbol.fingerprint)
                    .or_default()
                    .1
                    .push(item);
            }
        }
    }
    for (_, (baseline, candidate)) in structural {
        let disposition = match (baseline.len(), candidate.len()) {
            (0, _) => "candidate_only",
            (_, 0) => "baseline_only_unresolved",
            (1, 1) => "possible_move_or_rename",
            _ => "ambiguous_structural",
        };
        report.rows.push(Row {
            exact_nesting_observation: None,
            disposition,
            baseline,
            candidate,
        });
    }
    report
}

fn occurrences(file: &Source) -> impl Iterator<Item = Occurrence<'_>> {
    file.symbols.iter().map(|symbol| Occurrence {
        path: file.path,
        symbol,
    })
}

impl Comparison<'_> {
    pub(super) fn print(&self) {
        println!(
            "Advisory {} {} -> {} ({})",
            self.package, self.baseline_commit, self.candidate_commit, self.metric_policy
        );
        println!("Thresholds: parameters <= 6 (excluding receiver), control nesting <= 4");
        println!("{}", self.outside_measurement);
        for file in &self.files {
            println!(
                "{}: {} -> {}",
                file.path, file.baseline_status, file.candidate_status
            );
        }
        for row in &self.rows {
            println!(
                "{} {}",
                row.disposition,
                row.exact_nesting_observation.unwrap_or("")
            );
            for (side, items) in [("baseline", &row.baseline), ("candidate", &row.candidate)] {
                for item in items {
                    let s = item.symbol;
                    let scope = s.id.owners.join(" / ");
                    println!(
                        "  {side} {}:{}-{} {} {} | parameters={} nesting={}",
                        item.path,
                        s.lines[0],
                        s.lines[1],
                        scope,
                        s.id.signature,
                        s.metrics.parameters,
                        s.metrics
                            .nesting
                            .map_or_else(|| "unmeasured".into(), |n| n.to_string())
                    );
                }
            }
        }
    }
}
