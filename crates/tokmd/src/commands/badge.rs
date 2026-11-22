use crate::cli;
use anyhow::{Context, Result};
use std::io::{self, Write};
use tokmd_analysis as analysis;
use tokmd_format::badge_svg;

use crate::analysis_utils;
use crate::export_bundle;

pub(crate) fn handle(args: cli::BadgeArgs, global: &cli::GlobalArgs) -> Result<()> {
    let metric = args.metric;
    let mut preset = args.preset.unwrap_or(cli::AnalysisPreset::Receipt);
    if metric == cli::BadgeMetric::Hotspot && args.preset.is_none() {
        preset = cli::AnalysisPreset::Risk;
    }
    let git_flag = if args.git {
        Some(true)
    } else if args.no_git {
        Some(false)
    } else if metric == cli::BadgeMetric::Hotspot {
        Some(true)
    } else {
        None
    };

    let bundle = export_bundle::load_export_from_inputs(&args.inputs, global)?;
    let source = tokmd_analysis_types::AnalysisSource {
        inputs: args
            .inputs
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        export_path: bundle.export_path.as_ref().map(|p| p.display().to_string()),
        base_receipt_path: bundle.export_path.as_ref().map(|p| p.display().to_string()),
        export_schema_version: bundle.meta.schema_version,
        export_generated_at_ms: bundle.meta.generated_at_ms,
        base_signature: None,
        module_roots: bundle.meta.module_roots.clone(),
        module_depth: bundle.meta.module_depth,
        children: analysis_utils::child_include_to_string(bundle.meta.children),
    };
    let args_meta = tokmd_analysis_types::AnalysisArgsMeta {
        preset: analysis_utils::preset_to_string(preset),
        format: "badge".to_string(),
        window_tokens: None,
        git: git_flag,
        max_files: None,
        max_bytes: None,
        max_file_bytes: None,
        max_commits: args.max_commits,
        max_commit_files: args.max_commit_files,
        import_granularity: "module".to_string(),
    };
    let request = analysis::AnalysisRequest {
        preset: analysis_utils::map_preset(preset),
        args: args_meta,
        limits: analysis::AnalysisLimits {
            max_files: None,
            max_bytes: None,
            max_file_bytes: None,
            max_commits: args.max_commits,
            max_commit_files: args.max_commit_files,
        },
        window_tokens: None,
        git: git_flag,
        import_granularity: analysis::ImportGranularity::Module,
        detail_functions: false,
        near_dup: false,
        near_dup_threshold: 0.80,
        near_dup_max_files: 2000,
        near_dup_scope: analysis::NearDupScope::Module,
        near_dup_max_pairs: None,
        near_dup_exclude: Vec::new(),
        effort: None,
    };
    let ctx = analysis::AnalysisContext {
        export: bundle.export,
        root: bundle.root,
        source,
    };
    let receipt = analysis::analyze(ctx, request)?;

    let value = match metric {
        cli::BadgeMetric::Lines => receipt
            .derived
            .as_ref()
            .map(|d| d.totals.lines.to_string())
            .unwrap_or_else(|| "0".to_string()),
        cli::BadgeMetric::Tokens => receipt
            .derived
            .as_ref()
            .map(|d| d.totals.tokens.to_string())
            .unwrap_or_else(|| "0".to_string()),
        cli::BadgeMetric::Bytes => receipt
            .derived
            .as_ref()
            .map(|d| d.totals.bytes.to_string())
            .unwrap_or_else(|| "0".to_string()),
        cli::BadgeMetric::Doc => receipt
            .derived
            .as_ref()
            .map(|d| format!("{:.1}%", d.doc_density.total.ratio * 100.0))
            .unwrap_or_else(|| "0%".to_string()),
        cli::BadgeMetric::Blank => receipt
            .derived
            .as_ref()
            .map(|d| format!("{:.1}%", d.whitespace.total.ratio * 100.0))
            .unwrap_or_else(|| "0%".to_string()),
        cli::BadgeMetric::Hotspot => receipt
            .git
            .as_ref()
            .and_then(|g| g.hotspots.first())
            .map(|h| h.score.to_string())
            .unwrap_or_else(|| "n/a".to_string()),
    };

    let label = badge_metric_label(metric);
    let svg = badge_svg(label, &value);

    if let Some(output) = args.output {
        std::fs::write(&output, svg)
            .with_context(|| format!("Failed to write badge to {}", output.display()))?;
    } else {
        write_stdout(&mut io::stdout().lock(), &svg)?;
    }

    Ok(())
}

fn write_stdout(out: &mut impl Write, svg: &str) -> Result<()> {
    out.write_all(svg.as_bytes())
        .context("Failed to write badge SVG to stdout")?;
    // SVG has no final LF; buffered output errors must be observed before exit.
    out.flush().context("Failed to flush badge SVG to stdout")
}

fn badge_metric_label(metric: cli::BadgeMetric) -> &'static str {
    match metric {
        cli::BadgeMetric::Lines => "lines",
        cli::BadgeMetric::Tokens => "tokens",
        cli::BadgeMetric::Bytes => "bytes",
        cli::BadgeMetric::Doc => "doc",
        cli::BadgeMetric::Blank => "blank",
        cli::BadgeMetric::Hotspot => "hotspot",
    }
}

#[cfg(test)]
mod output_tests {
    use super::write_stdout;
    use anyhow::{Context, Result};
    use std::io::{self, Write};

    const SVG: &str = "<svg>badge</svg>";

    #[derive(Default)]
    struct Writer {
        bytes: Vec<u8>,
        fail_at: Option<(usize, io::ErrorKind)>,
        fail_flush: bool,
        flushes: usize,
    }

    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some((offset, kind)) = self.fail_at
                && offset == self.bytes.len()
            {
                self.fail_at = None;
                return Err(io::Error::new(kind, "write rejected"));
            }
            let Some(&byte) = bytes.first() else {
                return Ok(0);
            };
            self.bytes.push(byte);
            Ok(1) // Forces write_all to handle successful short writes.
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if self.fail_flush {
                Err(io::Error::other("flush rejected"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn short_writes_preserve_exact_svg_and_flush() -> Result<()> {
        let mut out = Writer::default();
        write_stdout(&mut out, SVG)?;
        assert_eq!(out.bytes, SVG.as_bytes());
        assert_eq!(out.flushes, 1);
        Ok(())
    }

    #[test]
    fn initial_partial_and_last_byte_errors_preserve_native_cause() -> Result<()> {
        for kind in [io::ErrorKind::BrokenPipe, io::ErrorKind::StorageFull] {
            for offset in [0, 4, SVG.len() - 1] {
                let mut out = Writer {
                    fail_at: Some((offset, kind)),
                    ..Writer::default()
                };
                let err = write_stdout(&mut out, SVG)
                    .err()
                    .context("expected write failure")?;
                assert_eq!(err.to_string(), "Failed to write badge SVG to stdout");
                let cause = err
                    .downcast_ref::<io::Error>()
                    .context("native I/O cause")?;
                assert_eq!(cause.kind(), kind);
                assert_eq!(cause.to_string(), "write rejected");
                assert_eq!(crate::exit_code(&err), 1);
                assert_eq!(Some(out.bytes.as_slice()), SVG.as_bytes().get(..offset));
                assert_eq!(out.flushes, 0);
            }
        }
        Ok(())
    }

    #[test]
    fn flush_error_is_returned_after_complete_svg() -> Result<()> {
        let mut out = Writer {
            fail_flush: true,
            ..Writer::default()
        };
        let err = write_stdout(&mut out, SVG)
            .err()
            .context("expected flush failure")?;
        assert_eq!(err.to_string(), "Failed to flush badge SVG to stdout");
        let cause = err
            .downcast_ref::<io::Error>()
            .context("native I/O cause")?;
        assert_eq!(cause.kind(), io::ErrorKind::Other);
        assert_eq!(cause.to_string(), "flush rejected");
        assert_eq!(crate::exit_code(&err), 1);
        assert_eq!(out.bytes, SVG.as_bytes());
        assert_eq!(out.flushes, 1);
        Ok(())
    }
}
