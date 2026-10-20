//! Analysis formatting utilities for the tokmd CLI.
//!
//! This module provides the bridge between analysis receipts (from `tokmd-analysis-types`)
//! and formatted output. It uses the `tokmd_core::analysis_facade` to maintain tier
//! boundary compliance — the CLI (Tier 5) does not directly depend on Tier 3 crates.
//!
//! ## Architecture Note
//!
//! Per ADR-001, all analysis formatting goes through the tokmd-core facade rather
//! than directly importing the analysis renderer. This ensures the product layer
//! depends only on the facade (Tier 4) and contracts (Tier 0).

use std::io::{IsTerminal, Write};
use std::path::Path;

use crate::cli;
use anyhow::{Context, Result};
use tokmd_analysis as analysis;
use tokmd_analysis_types as analysis_types;
/// Re-exported from tokmd-core facade to maintain tier boundary compliance.
/// See ADR-001 for the architectural rationale.
use tokmd_core::analysis_facade::{RenderedOutput, render};

pub(crate) fn child_include_to_string(mode: tokmd_types::ChildIncludeMode) -> String {
    match mode {
        tokmd_types::ChildIncludeMode::Separate => "separate".to_string(),
        tokmd_types::ChildIncludeMode::ParentsOnly => "parents-only".to_string(),
    }
}

pub(crate) fn preset_to_string(preset: cli::AnalysisPreset) -> String {
    map_preset(preset).as_str().to_string()
}

pub(crate) fn format_to_string(format: tokmd_types::AnalysisFormat) -> String {
    match format {
        tokmd_types::AnalysisFormat::Md => "md".to_string(),
        tokmd_types::AnalysisFormat::Json => "json".to_string(),
        tokmd_types::AnalysisFormat::Jsonld => "jsonld".to_string(),
        tokmd_types::AnalysisFormat::Xml => "xml".to_string(),
        tokmd_types::AnalysisFormat::Svg => "svg".to_string(),
        tokmd_types::AnalysisFormat::Mermaid => "mermaid".to_string(),
        tokmd_types::AnalysisFormat::Obj => "obj".to_string(),
        tokmd_types::AnalysisFormat::Midi => "midi".to_string(),
        tokmd_types::AnalysisFormat::Tree => "tree".to_string(),
        tokmd_types::AnalysisFormat::Html => "html".to_string(),
    }
}

pub(crate) fn granularity_to_string(granularity: cli::ImportGranularity) -> String {
    match granularity {
        cli::ImportGranularity::Module => "module".to_string(),
        cli::ImportGranularity::File => "file".to_string(),
    }
}

pub(crate) fn map_preset(preset: cli::AnalysisPreset) -> analysis::AnalysisPreset {
    match preset {
        cli::AnalysisPreset::Receipt => analysis::AnalysisPreset::Receipt,
        cli::AnalysisPreset::Estimate => analysis::AnalysisPreset::Estimate,
        cli::AnalysisPreset::BunUb => analysis::AnalysisPreset::BunUb,
        cli::AnalysisPreset::Health => analysis::AnalysisPreset::Health,
        cli::AnalysisPreset::Risk => analysis::AnalysisPreset::Risk,
        cli::AnalysisPreset::Supply => analysis::AnalysisPreset::Supply,
        cli::AnalysisPreset::Architecture => analysis::AnalysisPreset::Architecture,
        cli::AnalysisPreset::Topics => analysis::AnalysisPreset::Topics,
        cli::AnalysisPreset::Security => analysis::AnalysisPreset::Security,
        cli::AnalysisPreset::Identity => analysis::AnalysisPreset::Identity,
        cli::AnalysisPreset::Git => analysis::AnalysisPreset::Git,
        cli::AnalysisPreset::Deep => analysis::AnalysisPreset::Deep,
        cli::AnalysisPreset::Fun => analysis::AnalysisPreset::Fun,
    }
}

pub(crate) fn map_granularity(granularity: cli::ImportGranularity) -> analysis::ImportGranularity {
    match granularity {
        cli::ImportGranularity::Module => analysis::ImportGranularity::Module,
        cli::ImportGranularity::File => analysis::ImportGranularity::File,
    }
}

fn analysis_output_filename(format: tokmd_types::AnalysisFormat) -> &'static str {
    match format {
        tokmd_types::AnalysisFormat::Md => "analysis.md",
        tokmd_types::AnalysisFormat::Json => "analysis.json",
        tokmd_types::AnalysisFormat::Jsonld => "analysis.jsonld",
        tokmd_types::AnalysisFormat::Xml => "analysis.xml",
        tokmd_types::AnalysisFormat::Svg => "analysis.svg",
        tokmd_types::AnalysisFormat::Mermaid => "analysis.mmd",
        tokmd_types::AnalysisFormat::Obj => "analysis.obj",
        tokmd_types::AnalysisFormat::Midi => "analysis.mid",
        tokmd_types::AnalysisFormat::Tree => "analysis.tree.txt",
        tokmd_types::AnalysisFormat::Html => "analysis.html",
    }
}

pub(crate) fn write_analysis_output(
    receipt: &analysis_types::AnalysisReceipt,
    output_dir: &Path,
    format: tokmd_types::AnalysisFormat,
) -> Result<()> {
    let rendered = render(receipt, format)?;
    let out_path = output_dir.join(analysis_output_filename(format));
    match rendered {
        RenderedOutput::Text(text) => {
            std::fs::write(&out_path, text)?;
        }
        RenderedOutput::Binary(bytes) => {
            std::fs::write(&out_path, bytes)?;
        }
    }
    Ok(())
}

/// Render an analysis receipt and write it to an explicit file path.
///
/// Unlike [`write_analysis_output`], the caller controls the exact file name.
/// The packet orchestrator uses this to write `analyze.md` and `analyze.json`
/// from a single analysis pass.
pub(crate) fn write_analysis_to_path(
    receipt: &analysis_types::AnalysisReceipt,
    out_path: &Path,
    format: tokmd_types::AnalysisFormat,
) -> Result<()> {
    let rendered = render(receipt, format)?;
    match rendered {
        RenderedOutput::Text(text) => {
            std::fs::write(out_path, text)?;
        }
        RenderedOutput::Binary(bytes) => {
            std::fs::write(out_path, bytes)?;
        }
    }
    Ok(())
}

pub(crate) fn write_analysis_stdout(
    receipt: &analysis_types::AnalysisReceipt,
    format: tokmd_types::AnalysisFormat,
) -> Result<()> {
    let rendered = render(receipt, format)?;
    let mut stdout = std::io::stdout().lock();
    match rendered {
        RenderedOutput::Text(text) => {
            write_analysis_bytes(&mut stdout, text.as_bytes())?;
        }
        RenderedOutput::Binary(bytes) => {
            if stdout.is_terminal() {
                anyhow::bail!(
                    "Refusing to write binary output (format: {:?}) to a terminal to prevent rendering garbage characters. Please redirect stdout to a file (e.g., `> output.bin`) or specify an output directory.",
                    format
                );
            }
            write_analysis_bytes(&mut stdout, &bytes)?;
        }
    }
    Ok(())
}

fn write_analysis_bytes(out: &mut impl Write, bytes: &[u8]) -> Result<()> {
    out.write_all(bytes)
        .context("Failed to write analysis output to stdout")?;
    // Short SVG and binary formats can remain buffered without a newline.
    // Rust's process-shutdown flush discards errors, so finish explicitly.
    out.flush()
        .context("Failed to flush analysis output to stdout")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct Writer {
        bytes: Vec<u8>,
        fail_at: Option<usize>,
        fail_flush: bool,
        flushes: usize,
    }

    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_at == Some(self.bytes.len()) {
                return Err(io::Error::from_raw_os_error(28));
            }
            let count = bytes.len().min(1);
            self.bytes.extend(bytes.iter().take(count));
            Ok(count)
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

    fn writer(fail_at: Option<usize>, fail_flush: bool) -> Writer {
        Writer {
            bytes: Vec::new(),
            fail_at,
            fail_flush,
            flushes: 0,
        }
    }

    #[test]
    fn stdout_short_writes_preserve_binary_bytes_and_flush() -> Result<()> {
        let mut out = writer(None, false);
        write_analysis_bytes(&mut out, b"MThd\0\xff")?;
        assert_eq!(out.bytes, b"MThd\0\xff");
        assert_eq!(out.flushes, 1);
        Ok(())
    }

    #[test]
    fn stdout_write_failures_preserve_native_cause() -> Result<()> {
        for offset in [0, 2, 5] {
            let mut out = writer(Some(offset), false);
            let error = write_analysis_bytes(&mut out, b"<svg/>")
                .err()
                .context("expected write failure")?;
            assert_eq!(
                error.to_string(),
                "Failed to write analysis output to stdout"
            );
            assert_eq!(
                error
                    .downcast_ref::<io::Error>()
                    .context("I/O cause")?
                    .raw_os_error(),
                Some(28)
            );
            assert_eq!(out.bytes.len(), offset);
            assert_eq!(out.flushes, 0);
        }
        Ok(())
    }

    #[test]
    fn stdout_flush_failure_follows_complete_write() -> Result<()> {
        let mut out = writer(None, true);
        let error = write_analysis_bytes(&mut out, b"<svg/>")
            .err()
            .context("expected flush failure")?;
        assert_eq!(
            error.to_string(),
            "Failed to flush analysis output to stdout"
        );
        assert_eq!(
            error
                .downcast_ref::<io::Error>()
                .context("I/O cause")?
                .to_string(),
            "flush rejected"
        );
        assert_eq!(out.bytes, b"<svg/>");
        assert_eq!(out.flushes, 1);
        Ok(())
    }

    #[test]
    fn test_child_include_to_string_separate() {
        assert_eq!(
            child_include_to_string(tokmd_types::ChildIncludeMode::Separate),
            "separate"
        );
    }

    #[test]
    fn test_child_include_to_string_parents_only() {
        assert_eq!(
            child_include_to_string(tokmd_types::ChildIncludeMode::ParentsOnly),
            "parents-only"
        );
    }

    #[test]
    fn test_preset_to_string_all_variants() {
        assert_eq!(preset_to_string(cli::AnalysisPreset::Receipt), "receipt");
        assert_eq!(preset_to_string(cli::AnalysisPreset::Estimate), "estimate");
        assert_eq!(preset_to_string(cli::AnalysisPreset::BunUb), "bun-ub");
        assert_eq!(preset_to_string(cli::AnalysisPreset::Health), "health");
        assert_eq!(preset_to_string(cli::AnalysisPreset::Risk), "risk");
        assert_eq!(preset_to_string(cli::AnalysisPreset::Supply), "supply");
        assert_eq!(
            preset_to_string(cli::AnalysisPreset::Architecture),
            "architecture"
        );
        assert_eq!(preset_to_string(cli::AnalysisPreset::Topics), "topics");
        assert_eq!(preset_to_string(cli::AnalysisPreset::Security), "security");
        assert_eq!(preset_to_string(cli::AnalysisPreset::Identity), "identity");
        assert_eq!(preset_to_string(cli::AnalysisPreset::Git), "git");
        assert_eq!(preset_to_string(cli::AnalysisPreset::Deep), "deep");
        assert_eq!(preset_to_string(cli::AnalysisPreset::Fun), "fun");
    }

    #[test]
    fn test_format_to_string_all_variants() {
        assert_eq!(format_to_string(tokmd_types::AnalysisFormat::Md), "md");
        assert_eq!(format_to_string(tokmd_types::AnalysisFormat::Json), "json");
        assert_eq!(
            format_to_string(tokmd_types::AnalysisFormat::Jsonld),
            "jsonld"
        );
        assert_eq!(format_to_string(tokmd_types::AnalysisFormat::Xml), "xml");
        assert_eq!(format_to_string(tokmd_types::AnalysisFormat::Svg), "svg");
        assert_eq!(
            format_to_string(tokmd_types::AnalysisFormat::Mermaid),
            "mermaid"
        );
        assert_eq!(format_to_string(tokmd_types::AnalysisFormat::Obj), "obj");
        assert_eq!(format_to_string(tokmd_types::AnalysisFormat::Midi), "midi");
        assert_eq!(format_to_string(tokmd_types::AnalysisFormat::Tree), "tree");
        assert_eq!(format_to_string(tokmd_types::AnalysisFormat::Html), "html");
    }

    #[test]
    fn test_granularity_to_string() {
        assert_eq!(
            granularity_to_string(cli::ImportGranularity::Module),
            "module"
        );
        assert_eq!(granularity_to_string(cli::ImportGranularity::File), "file");
    }

    #[test]
    fn test_map_preset_all_variants() {
        assert!(matches!(
            map_preset(cli::AnalysisPreset::Receipt),
            analysis::AnalysisPreset::Receipt
        ));
        assert!(matches!(
            map_preset(cli::AnalysisPreset::Estimate),
            analysis::AnalysisPreset::Estimate
        ));
        assert!(matches!(
            map_preset(cli::AnalysisPreset::BunUb),
            analysis::AnalysisPreset::BunUb
        ));
        assert!(matches!(
            map_preset(cli::AnalysisPreset::Health),
            analysis::AnalysisPreset::Health
        ));
        assert!(matches!(
            map_preset(cli::AnalysisPreset::Risk),
            analysis::AnalysisPreset::Risk
        ));
        assert!(matches!(
            map_preset(cli::AnalysisPreset::Supply),
            analysis::AnalysisPreset::Supply
        ));
        assert!(matches!(
            map_preset(cli::AnalysisPreset::Architecture),
            analysis::AnalysisPreset::Architecture
        ));
        assert!(matches!(
            map_preset(cli::AnalysisPreset::Topics),
            analysis::AnalysisPreset::Topics
        ));
        assert!(matches!(
            map_preset(cli::AnalysisPreset::Security),
            analysis::AnalysisPreset::Security
        ));
        assert!(matches!(
            map_preset(cli::AnalysisPreset::Identity),
            analysis::AnalysisPreset::Identity
        ));
        assert!(matches!(
            map_preset(cli::AnalysisPreset::Git),
            analysis::AnalysisPreset::Git
        ));
        assert!(matches!(
            map_preset(cli::AnalysisPreset::Deep),
            analysis::AnalysisPreset::Deep
        ));
        assert!(matches!(
            map_preset(cli::AnalysisPreset::Fun),
            analysis::AnalysisPreset::Fun
        ));
    }

    #[test]
    fn test_map_granularity() {
        assert!(matches!(
            map_granularity(cli::ImportGranularity::Module),
            analysis::ImportGranularity::Module
        ));
        assert!(matches!(
            map_granularity(cli::ImportGranularity::File),
            analysis::ImportGranularity::File
        ));
    }

    #[test]
    fn test_analysis_output_filename() {
        assert_eq!(
            analysis_output_filename(tokmd_types::AnalysisFormat::Md),
            "analysis.md"
        );
        assert_eq!(
            analysis_output_filename(tokmd_types::AnalysisFormat::Json),
            "analysis.json"
        );
        assert_eq!(
            analysis_output_filename(tokmd_types::AnalysisFormat::Jsonld),
            "analysis.jsonld"
        );
        assert_eq!(
            analysis_output_filename(tokmd_types::AnalysisFormat::Xml),
            "analysis.xml"
        );
        assert_eq!(
            analysis_output_filename(tokmd_types::AnalysisFormat::Svg),
            "analysis.svg"
        );
        assert_eq!(
            analysis_output_filename(tokmd_types::AnalysisFormat::Mermaid),
            "analysis.mmd"
        );
        assert_eq!(
            analysis_output_filename(tokmd_types::AnalysisFormat::Obj),
            "analysis.obj"
        );
        assert_eq!(
            analysis_output_filename(tokmd_types::AnalysisFormat::Midi),
            "analysis.mid"
        );
        assert_eq!(
            analysis_output_filename(tokmd_types::AnalysisFormat::Tree),
            "analysis.tree.txt"
        );
        assert_eq!(
            analysis_output_filename(tokmd_types::AnalysisFormat::Html),
            "analysis.html"
        );
    }
}
