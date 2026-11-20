//! Bundle text rendering helpers for context and handoff output.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use tokmd_types::{ContextFileRow, InclusionPolicy};

use crate::cli;

mod head_tail;

#[cfg(test)]
#[path = "render/head_tail_bounds.rs"]
mod head_tail_bounds;

/// A writer wrapper that counts bytes written.
pub(crate) struct CountingWriter<W: Write> {
    inner: W,
    bytes: u64,
}

impl<W: Write> CountingWriter<W> {
    pub(crate) fn new(inner: W) -> Self {
        Self { inner, bytes: 0 }
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.bytes += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Format list output as a markdown table.
pub(crate) fn format_list_output(
    selected: &[ContextFileRow],
    budget: usize,
    used_tokens: usize,
    utilization: f64,
    strategy: cli::ContextStrategy,
) -> String {
    let mut out = String::new();
    out.push_str("# Context Pack\n\n");
    out.push_str(&format!("Budget: {} tokens\n", budget));
    out.push_str(&format!(
        "Used: {} tokens ({:.1}%)\n",
        used_tokens, utilization
    ));
    out.push_str(&format!("Files: {}\n", selected.len()));
    out.push_str(&format!("Strategy: {:?}\n\n", strategy));
    out.push_str("|Path|Module|Lang|Used|Tokens|Policy|Code|\n");
    out.push_str("|---|---|---|---:|---:|---|---:|\n");
    for file in selected {
        let used = file.effective_tokens.unwrap_or(file.tokens);
        let policy = list_policy_label(file);
        out.push_str(&format!(
            "|{}|{}|{}|{}|{}|{}|{}|\n",
            file.path, file.module, file.lang, used, file.tokens, policy, file.code
        ));
    }
    out
}

fn list_policy_label(file: &ContextFileRow) -> &str {
    if let Some(reason) = file.policy_reason.as_deref() {
        return reason;
    }

    match file.policy {
        InclusionPolicy::Full => "full",
        InclusionPolicy::HeadTail => "head+tail",
        InclusionPolicy::Summary => "summary",
        InclusionPolicy::Skip => "skipped",
    }
}

/// Write bundle output (concatenated file contents) directly to a writer.
///
/// Streams file content to avoid loading the entire bundle into memory and
/// dispatches based on file inclusion policy (Full / HeadTail / Skip).
pub(crate) fn write_context_bundle_output<W: Write>(
    w: &mut W,
    selected: &[ContextFileRow],
    compress: bool,
    input_paths: &[PathBuf],
) -> anyhow::Result<()> {
    write_bundle_output(w, selected, compress, FullRead::Stream, input_paths)
}

/// Handoff's existing uncompressed Full policy validates UTF-8. Context's
/// uncompressed Full policy streams raw bytes. Other policies share one path.
pub(crate) fn write_handoff_bundle_output<W: Write>(
    w: &mut W,
    selected: &[ContextFileRow],
    compress: bool,
    input_paths: &[PathBuf],
) -> anyhow::Result<()> {
    write_bundle_output(w, selected, compress, FullRead::Utf8, input_paths)
}

#[derive(Clone, Copy)]
enum FullRead {
    Stream,
    Utf8,
}

fn write_bundle_output<W: Write>(
    w: &mut W,
    selected: &[ContextFileRow],
    compress: bool,
    full_read: FullRead,
    input_paths: &[PathBuf],
) -> anyhow::Result<()> {
    for file in selected {
        let path = bundle_source_path(&file.path, input_paths);
        // Full and HeadTail must open/read directly: exists() hides IO errors
        // and can silently omit selected inputs. Summary/Skip need no read.
        match file.policy {
            InclusionPolicy::Full => {
                writeln!(w, "// === {} ===", file.path)?;

                if compress {
                    let f = File::open(&path)
                        .with_context(|| format!("Failed to open file: {}", path.display()))?;
                    let reader = BufReader::new(f);
                    for line in reader.lines() {
                        let line = line
                            .with_context(|| format!("Failed to read file: {}", path.display()))?;
                        if !line.trim().is_empty() {
                            writeln!(w, "{line}")?;
                        }
                    }
                    writeln!(w)?;
                } else if matches!(full_read, FullRead::Utf8) {
                    let content = std::fs::read_to_string(&path)
                        .with_context(|| format!("Failed to read file: {}", path.display()))?;
                    w.write_all(content.as_bytes())?;
                    if !content.ends_with('\n') {
                        writeln!(w)?;
                    }
                    writeln!(w)?;
                } else {
                    let mut f = File::open(&path)
                        .with_context(|| format!("Failed to open file: {}", path.display()))?;
                    let mut buf = [0u8; 16 * 1024];
                    let mut last: Option<u8> = None;
                    loop {
                        let n = f
                            .read(&mut buf)
                            .with_context(|| format!("Failed to read file: {}", path.display()))?;
                        if n == 0 {
                            break;
                        }
                        last = Some(buf[n - 1]);
                        w.write_all(&buf[..n])?;
                    }
                    if last != Some(b'\n') {
                        w.write_all(b"\n")?;
                    }
                    w.write_all(b"\n")?;
                }
            }
            InclusionPolicy::HeadTail => {
                writeln!(w, "// === {} ===", file.path)?;
                write_head_tail(w, &path, file, compress)?;
                writeln!(w)?;
            }
            InclusionPolicy::Summary | InclusionPolicy::Skip => {
                writeln!(
                    w,
                    "// === {} [skipped: {}] ===",
                    file.path,
                    file.policy_reason.as_deref().unwrap_or("policy")
                )?;
                writeln!(w)?;
            }
        }
    }
    Ok(())
}

/// Model receipts normalize away a Unix leading slash. Recover the physical
/// path from an explicitly absolute scan root, without probing existence or
/// changing the advertised path. Relative invocations retain their usual path.
fn bundle_source_path(selected_path: &str, input_paths: &[PathBuf]) -> PathBuf {
    let path = Path::new(selected_path);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    input_paths
        .iter()
        .filter(|root| root.is_absolute())
        .filter_map(|root| {
            let normalized_root = tokmd_model::normalize_path(root, None);
            path.strip_prefix(&normalized_root).ok().map(|relative| {
                let resolved = if relative.as_os_str().is_empty() {
                    root.clone()
                } else {
                    root.join(relative)
                };
                (root, resolved)
            })
        })
        .max_by_key(|(root, _)| root.components().count())
        .map_or_else(|| path.to_path_buf(), |(_, resolved)| resolved)
}

/// Invalidate a previous completion marker before rewriting any payload.
/// This is not an atomic generation replacement or crash-safety guarantee.
pub(crate) fn remove_completion_marker(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("Failed to remove completion marker: {}", path.display())),
    }
}

/// Write a bounded head/tail excerpt, validating the complete input as UTF-8.
///
/// The retained-source allowance is four bytes per effective token. Path
/// headers and the final bundle separator belong to the caller. The body adds
/// at most one fixed-format omission marker and two fragment-final newlines;
/// these framing bytes are counted in the existing output audit. This is an
/// excerpt safety bound, not exact tokenization or a global output-size cap.
pub(crate) fn write_head_tail<W: Write>(
    w: &mut W,
    path: &Path,
    file: &ContextFileRow,
    compress: bool,
) -> anyhow::Result<()> {
    let mut input =
        File::open(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let allowance = file
        .effective_tokens
        .unwrap_or(file.tokens)
        .saturating_mul(4);
    // Capture first so malformed or unreadable omitted content is still a
    // required-read failure. A failed writer may leave counted partial bytes.
    let excerpt = head_tail::capture(&mut input, allowance)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    head_tail::write(w, excerpt, file, compress, allowance)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context_row(policy: InclusionPolicy) -> ContextFileRow {
        ContextFileRow {
            path: "src/big.rs".to_string(),
            module: "src".to_string(),
            lang: "Rust".to_string(),
            tokens: 31_753,
            code: 2_370,
            lines: 3_128,
            bytes: 127_012,
            value: 750,
            rank_reason: "test".to_string(),
            policy,
            effective_tokens: None,
            policy_reason: None,
            classifications: Vec::new(),
        }
    }

    #[test]
    fn list_output_shows_effective_tokens_and_policy_reason() {
        let mut row = context_row(InclusionPolicy::HeadTail);
        row.effective_tokens = Some(750);
        row.policy_reason =
            Some("file exceeds cap (31753 > 750 tokens); head+tail included".to_string());

        let output = format_list_output(&[row], 5_000, 750, 15.0, cli::ContextStrategy::Greedy);

        assert!(output.contains("|Path|Module|Lang|Used|Tokens|Policy|Code|"));
        assert!(output.contains(
            "|src/big.rs|src|Rust|750|31753|file exceeds cap (31753 > 750 tokens); head+tail included|2370|"
        ));
    }

    #[test]
    fn list_output_labels_full_policy_when_tokens_are_uncharged() {
        let row = context_row(InclusionPolicy::Full);

        let output = format_list_output(
            std::slice::from_ref(&row),
            50_000,
            row.tokens,
            63.5,
            cli::ContextStrategy::Greedy,
        );

        assert!(output.contains("|src/big.rs|src|Rust|31753|31753|full|2370|"));
    }

    fn recovery_row(path: &Path) -> ContextFileRow {
        let mut row = context_row(InclusionPolicy::HeadTail);
        row.path = path.display().to_string();
        row.tokens = 10;
        row.effective_tokens = Some(4);
        row.lines = 10;
        row.code = 10;
        row.bytes = 40;
        row
    }

    fn missing_head_tail_can_be_created_and_retried(directory: &str) -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let parent = dir.path().join(directory);
        let path = parent.join("input.rs");
        let row = recovery_row(&path);
        anyhow::ensure!(!path.exists(), "missing renderer fixture already exists");
        let mut output = Vec::new();
        let error = match write_head_tail(&mut output, &path, &row, false) {
            Ok(()) => anyhow::bail!("missing head-tail file unexpectedly rendered"),
            Err(error) => error,
        };
        let cause = error.downcast_ref::<std::io::Error>().ok_or_else(|| {
            anyhow::anyhow!("renderer discarded the original IO cause: {error:#}")
        })?;
        anyhow::ensure!(
            cause.kind() == std::io::ErrorKind::NotFound,
            "wrong missing renderer IO kind: {:?}",
            cause.kind()
        );
        anyhow::ensure!(output.is_empty(), "failed renderer emitted content");
        let rendered = crate::format_error(&error);
        anyhow::ensure!(
            rendered.starts_with(&format!("Error: Failed to read {}:", path.display())),
            "missing selected renderer path context: {rendered}"
        );
        let hints = rendered
            .lines()
            .filter(|line| line.starts_with("- "))
            .collect::<Vec<_>>();
        anyhow::ensure!(
            hints
                == vec![
                    "- Verify the input path exists and is readable.",
                    "- Use an absolute path to avoid working-directory confusion.",
                ],
            "wrong renderer recovery for {directory}: {hints:?}"
        );
        std::fs::create_dir(&parent)?;
        std::fs::write(
            &path,
            "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
        )?;
        let mut retry = Vec::new();
        write_head_tail(&mut retry, &path, &row, false)?;
        let retry = std::str::from_utf8(&retry)?;
        anyhow::ensure!(
            retry == "one\ntwo\nthre\n// ... [33 bytes omitted] ...\nten\n",
            "wrong recovered head-tail output: {retry:?}"
        );
        Ok(())
    }

    #[test]
    fn head_tail_missing_rate_limit_path_preserves_io_and_recovers() -> anyhow::Result<()> {
        missing_head_tail_can_be_created_and_retried("rate_limit")
    }

    #[test]
    fn head_tail_missing_timeout_path_preserves_io_and_recovers() -> anyhow::Result<()> {
        missing_head_tail_can_be_created_and_retried("timeout")
    }

    #[test]
    fn head_tail_missing_base_ref_path_preserves_io_and_recovers() -> anyhow::Result<()> {
        missing_head_tail_can_be_created_and_retried("base ref")
    }

    #[test]
    fn head_tail_invalid_utf8_preserves_invalid_data_cause() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("invalid_utf8.rs");
        std::fs::write(&path, [0xff])?;
        let row = recovery_row(&path);
        let mut output = Vec::new();
        let error = match write_head_tail(&mut output, &path, &row, false) {
            Ok(()) => anyhow::bail!("invalid UTF8 head-tail file unexpectedly rendered"),
            Err(error) => error,
        };
        let cause = error
            .downcast_ref::<std::io::Error>()
            .ok_or_else(|| anyhow::anyhow!("renderer discarded InvalidData cause: {error:#}"))?;
        anyhow::ensure!(
            cause.kind() == std::io::ErrorKind::InvalidData,
            "wrong UTF8 IO kind: {:?}",
            cause.kind()
        );
        anyhow::ensure!(output.is_empty(), "invalid UTF8 renderer emitted content");
        anyhow::ensure!(
            crate::format_error(&error)
                .starts_with(&format!("Error: Failed to read {}:", path.display())),
            "missing invalid UTF8 selected-path context"
        );
        std::fs::write(
            &path,
            "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
        )?;
        let mut retry = Vec::new();
        write_head_tail(&mut retry, &path, &row, false)?;
        let retry = std::str::from_utf8(&retry)?;
        anyhow::ensure!(
            retry == "one\ntwo\nthre\n// ... [33 bytes omitted] ...\nten\n",
            "wrong recovered UTF8 head-tail output: {retry:?}"
        );
        Ok(())
    }
}
