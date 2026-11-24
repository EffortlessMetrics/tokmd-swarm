use std::io::Write;
use std::path::Path;
use std::process::Stdio;

use crate::cli;
use anyhow::{Context, Result};

use crate::git_support::git_cmd;

/// Exit codes for check-ignore:
/// - 0: Path is ignored
/// - 1: Path is not ignored
/// - 2: Error occurred
const EXIT_IGNORED: i32 = 0;
const EXIT_NOT_IGNORED: i32 = 1;

pub(crate) fn handle(args: cli::CliCheckIgnoreArgs, global: &cli::GlobalArgs) -> Result<()> {
    let exit = write_report(&args, global, &mut std::io::stdout().lock())?;
    std::process::exit(exit);
}

fn write_report(
    args: &cli::CliCheckIgnoreArgs,
    global: &cli::GlobalArgs,
    out: &mut impl Write,
) -> Result<i32> {
    let mut any_not_ignored = false;

    for path in &args.paths {
        let result = check_path(path, global, args.verbose)?;
        if !result.ignored {
            any_not_ignored = true;
        }
        write_result(out, &result, args.verbose)
            .context("Failed to write check-ignore report to stdout")?;
    }

    out.flush()
        .context("Failed to flush check-ignore report to stdout")?;

    // Exit code: 0 if all ignored, 1 if any not ignored
    Ok(if any_not_ignored {
        EXIT_NOT_IGNORED
    } else {
        EXIT_IGNORED
    })
}

struct CheckResult {
    path: String,
    ignored: bool,
    reasons: Vec<IgnoreReason>,
}

#[derive(Clone)]
enum IgnoreReason {
    Git {
        source: String,
        pattern: String,
        line: Option<usize>,
    },
    GitTracked, // File is tracked by git; gitignore rules don't apply
    Tokeignore {
        pattern: String,
    },
    ExcludeFlag {
        pattern: String,
    },
}

fn check_path(path: &Path, global: &cli::GlobalArgs, verbose: bool) -> Result<CheckResult> {
    let path_str = path.display().to_string();
    let mut reasons = Vec::new();
    let mut ignored = false;

    // Check if path exists before asking git/ignore matchers. `try_exists`
    // keeps access errors distinct from true missing paths.
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => {
            return Err(std::io::Error::from(std::io::ErrorKind::NotFound)).with_context(|| {
                format!("Failed to access path '{}': path does not exist", path_str)
            });
        }
        Err(err) => {
            return Err(err).with_context(|| format!("failed to access path '{}'", path_str));
        }
    }

    // 1. Check git ignore (if git is available and we're in a repo)
    if let Some(git_reason) = check_git_ignore(path) {
        reasons.push(git_reason);
        ignored = true;
    } else if is_git_tracked(path) {
        // File is tracked by git; gitignore rules don't apply
        reasons.push(IgnoreReason::GitTracked);
    }

    // 2. Check --exclude patterns from CLI
    for pattern in &global.excluded {
        if matches_glob(pattern, &path_str) {
            reasons.push(IgnoreReason::ExcludeFlag {
                pattern: pattern.clone(),
            });
            ignored = true;
        }
    }

    // 3. Check .tokeignore
    if let Some(tokeignore_reason) = check_tokeignore(path, &path_str) {
        reasons.push(tokeignore_reason);
        ignored = true;
    }

    // 4. Note if ignore flags would affect this
    if !ignored && verbose {
        if global.no_ignore {
            eprintln!("  note: --no-ignore is set, ignores disabled");
        }
        if global.no_ignore_vcs {
            eprintln!("  note: --no-ignore-vcs is set, VCS ignores disabled");
        }
        if global.no_ignore_dot {
            eprintln!("  note: --no-ignore-dot is set, .ignore/.tokeignore disabled");
        }
    }

    Ok(CheckResult {
        path: path_str,
        ignored,
        reasons,
    })
}

fn check_git_ignore(path: &Path) -> Option<IgnoreReason> {
    // NUL-delimited records keep source/pattern/path colons and tabs literal.
    // With -z Git also requires NUL-delimited paths on stdin.
    let mut child = git_cmd()
        .args(["check-ignore", "-v", "-z", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let written = child.stdin.take().map(|mut stdin| {
        stdin
            .write_all(path.as_os_str().as_encoded_bytes())
            .and_then(|()| stdin.write_all(&[0]))
    });
    // Reap Git even if it closed stdin before the path could be written.
    let output = child.wait_with_output().ok()?;
    written?.ok()?;
    if !output.status.success() {
        return None;
    }

    // Format: <source> NUL <line> NUL <pattern> NUL <path> NUL.
    let mut fields = output.stdout.split(|byte| *byte == 0);
    let source = fields.next()?;
    let line = fields.next()?;
    let pattern = fields.next()?;
    let _path = fields.next()?;
    // Verbose Git succeeds for an unignore match too; only positive rules
    // contribute an ignore reason. An escaped \! remains a positive rule.
    if pattern.starts_with(b"!") {
        return None;
    }

    Some(IgnoreReason::Git {
        source: String::from_utf8_lossy(source).into_owned(),
        pattern: String::from_utf8_lossy(pattern).into_owned(),
        line: std::str::from_utf8(line).ok()?.parse::<usize>().ok(),
    })
}

fn is_git_tracked(path: &Path) -> bool {
    git_cmd()
        .args(["ls-files", "--error-unmatch", "--"])
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn check_tokeignore(base_path: &Path, path_str: &str) -> Option<IgnoreReason> {
    // Look for .tokeignore in current directory and parents
    let mut dir = base_path.parent();
    while let Some(d) = dir {
        let tokeignore = d.join(".tokeignore");
        if let Ok(content) = std::fs::read_to_string(&tokeignore) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if matches_glob(line, path_str) {
                    return Some(IgnoreReason::Tokeignore {
                        pattern: line.to_string(),
                    });
                }
            }
        }
        dir = d.parent();
    }

    // Also check current working directory
    let cwd_tokeignore = Path::new(".tokeignore");
    if let Ok(content) = std::fs::read_to_string(cwd_tokeignore) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if matches_glob(line, path_str) {
                return Some(IgnoreReason::Tokeignore {
                    pattern: line.to_string(),
                });
            }
        }
    }

    None
}

pub(crate) fn matches_glob(pattern: &str, path: &str) -> bool {
    // Simple glob matching (could use the glob crate for more accuracy)
    // Handle common patterns: *, **, ?

    // Normalize path separators
    let path = path.replace('\\', "/");
    let pattern = pattern.replace('\\', "/");

    // Handle negation
    let (negated, pattern) = if let Some(stripped) = pattern.strip_prefix('!') {
        (true, stripped)
    } else {
        (false, pattern.as_str())
    };

    let matches = if pattern.contains("**") {
        // Double-star: match any path segments
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0].trim_end_matches('/');
            let suffix = parts[1].trim_start_matches('/');

            let prefix_matches = prefix.is_empty() || path.starts_with(prefix);
            let suffix_matches = suffix.is_empty() || path.ends_with(suffix);
            prefix_matches && suffix_matches
        } else {
            false
        }
    } else if pattern.contains('*') {
        // Single star: match within path segment
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            path.starts_with(parts[0]) && path.ends_with(parts[1])
        } else {
            // Multiple stars - simplistic handling
            path.contains(pattern.replace('*', "").as_str())
        }
    } else {
        // Exact match or suffix match
        path == pattern
            || path.ends_with(&format!("/{}", pattern))
            || path.starts_with(&format!("{}/", pattern))
    };

    if negated { !matches } else { matches }
}

fn write_result(out: &mut impl Write, result: &CheckResult, verbose: bool) -> std::io::Result<()> {
    if result.ignored {
        writeln!(out, "{}: ignored", result.path)?;
        if verbose {
            for reason in &result.reasons {
                match reason {
                    IgnoreReason::Git {
                        source,
                        pattern,
                        line,
                    } => {
                        if let Some(ln) = line {
                            writeln!(out, "  gitignore: {}:{} -> {}", source, ln, pattern)?;
                        } else {
                            writeln!(out, "  gitignore: {} -> {}", source, pattern)?;
                        }
                    }
                    IgnoreReason::Tokeignore { pattern } => {
                        writeln!(out, "  .tokeignore: {}", pattern)?;
                    }
                    IgnoreReason::ExcludeFlag { pattern } => {
                        writeln!(out, "  --exclude: {}", pattern)?;
                    }
                    IgnoreReason::GitTracked => {
                        writeln!(out, "  git: tracked (gitignore rules don't apply)")?;
                    }
                }
            }
        }
    } else {
        writeln!(out, "{}: not ignored", result.path)?;
        if verbose {
            for reason in &result.reasons {
                if let IgnoreReason::GitTracked = reason {
                    writeln!(out, "  note: tracked by git; gitignore rules don't apply")?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::GlobalArgs;
    use tempfile::tempdir;

    #[derive(Default)]
    struct ShortWriter {
        bytes: Vec<u8>,
        fail_at: Option<usize>,
        fail_flush: bool,
        flushes: usize,
    }

    impl Write for ShortWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.fail_at == Some(self.bytes.len()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "report write rejected",
                ));
            }
            if let Some(&byte) = bytes.first() {
                self.bytes.push(byte);
                Ok(1)
            } else {
                Ok(0)
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            if self.fail_flush {
                Err(std::io::Error::other("report flush rejected"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn report_short_writes_and_failures_precede_decision_exit() -> Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("fixture.rs");
        std::fs::write(&path, "fn fixture() {}\n")?;
        let args = cli::CliCheckIgnoreArgs {
            paths: vec![path.clone()],
            verbose: false,
        };
        for ignored in [true, false] {
            let global = cli::GlobalArgs {
                excluded: if ignored {
                    vec!["**/fixture.rs".into()]
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            let decision = if ignored { "ignored" } else { "not ignored" };
            let expected = format!("{}: {decision}\n", path.display());
            let mut out = ShortWriter::default();
            let code = write_report(&args, &global, &mut out)?;
            anyhow::ensure!(code == i32::from(!ignored), "decision exit: {code}");
            anyhow::ensure!(out.bytes == expected.as_bytes(), "short write truncated");
            anyhow::ensure!(out.flushes == 1, "missing final flush");

            for offset in [0, 1, expected.len() - 1] {
                let mut out = ShortWriter {
                    fail_at: Some(offset),
                    ..Default::default()
                };
                let err = write_report(&args, &global, &mut out)
                    .err()
                    .context("expected write error before returning the decision")?;
                anyhow::ensure!(
                    err.to_string() == "Failed to write check-ignore report to stdout",
                    "{err:#}"
                );
                let cause = err.downcast_ref::<std::io::Error>().context("I/O cause")?;
                anyhow::ensure!(cause.kind() == std::io::ErrorKind::BrokenPipe, "{err:#}");
                anyhow::ensure!(cause.to_string() == "report write rejected", "{err:#}");
                anyhow::ensure!(out.bytes == expected.as_bytes()[..offset]);
                anyhow::ensure!(out.flushes == 0, "flush after failed write");
                anyhow::ensure!(crate::exit_code(&err) == 1, "runtime error exit");
            }

            let mut out = ShortWriter {
                fail_flush: true,
                ..Default::default()
            };
            let err = write_report(&args, &global, &mut out)
                .err()
                .context("expected flush error before returning the decision")?;
            anyhow::ensure!(
                err.to_string() == "Failed to flush check-ignore report to stdout",
                "{err:#}"
            );
            let cause = err.downcast_ref::<std::io::Error>().context("I/O cause")?;
            anyhow::ensure!(cause.to_string() == "report flush rejected", "{err:#}");
            anyhow::ensure!(out.bytes == expected.as_bytes(), "flush must follow output");
            anyhow::ensure!(out.flushes == 1, "final flush count");
            anyhow::ensure!(crate::exit_code(&err) == 1, "runtime error exit");
        }
        Ok(())
    }

    #[test]
    fn verbose_report_reasons_keep_literal_bytes_under_short_writes() -> Result<()> {
        let result = CheckResult {
            path: "fixture.rs".into(),
            ignored: true,
            reasons: vec![
                IgnoreReason::Git {
                    source: ".gitignore".into(),
                    pattern: "*.rs".into(),
                    line: Some(3),
                },
                IgnoreReason::Git {
                    source: "parent/.gitignore".into(),
                    pattern: "fixture.rs".into(),
                    line: None,
                },
                IgnoreReason::Tokeignore {
                    pattern: "*.rs".into(),
                },
                IgnoreReason::ExcludeFlag {
                    pattern: "fixture.rs".into(),
                },
                IgnoreReason::GitTracked,
            ],
        };
        let expected = b"fixture.rs: ignored\n  gitignore: .gitignore:3 -> *.rs\n  gitignore: parent/.gitignore -> fixture.rs\n  .tokeignore: *.rs\n  --exclude: fixture.rs\n  git: tracked (gitignore rules don't apply)\n";
        let mut out = ShortWriter::default();
        write_result(&mut out, &result, true)?;
        anyhow::ensure!(out.bytes == expected);
        // A failure in any reason line must propagate, including the final newline.
        for offset in [21, 50, 100, expected.len() - 1] {
            let mut out = ShortWriter {
                fail_at: Some(offset),
                ..Default::default()
            };
            let err = write_result(&mut out, &result, true)
                .err()
                .context("expected verbose write failure")?;
            anyhow::ensure!(err.kind() == std::io::ErrorKind::BrokenPipe);
            anyhow::ensure!(out.bytes == expected[..offset]);
        }
        Ok(())
    }

    #[test]
    fn matches_glob_handles_double_star() {
        assert!(matches_glob("**/main.rs", "src/main.rs"));
        assert!(matches_glob("src/**", "src/nested/main.rs"));
    }

    #[test]
    fn matches_glob_handles_single_star() {
        assert!(matches_glob("*.rs", "src/main.rs"));
        assert!(matches_glob("src/*.rs", "src/main.rs"));
    }

    #[test]
    fn matches_glob_handles_negation() {
        assert!(!matches_glob("!*.rs", "src/main.rs"));
        assert!(matches_glob("!*.js", "src/main.rs"));
    }

    #[test]
    fn check_tokeignore_matches_parent_dir() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir)?;
        std::fs::write(dir.path().join(".tokeignore"), "*.rs\n")?;

        let file_path = src_dir.join("main.rs");
        std::fs::write(&file_path, "fn main() {}\n")?;

        let reason = check_tokeignore(&file_path, file_path.to_string_lossy().as_ref());
        assert!(matches!(
            reason,
            Some(IgnoreReason::Tokeignore { pattern }) if pattern == "*.rs"
        ));
        Ok(())
    }

    #[test]
    fn check_path_errors_on_missing_path() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let missing = dir.path().join("missing.rs");

        let err = match check_path(&missing, &GlobalArgs::default(), false) {
            Ok(_) => panic!("missing path should error"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("does not exist"));
        Ok(())
    }

    #[test]
    fn check_path_honors_exclude_flag() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("skip.rs");
        std::fs::write(&file_path, "fn skip() {}\n")?;

        let mut global = GlobalArgs::default();
        global.excluded.push("*.rs".to_string());

        let result = check_path(&file_path, &global, false)?;
        assert!(result.ignored);
        assert!(
            result.reasons.iter().any(|r| {
                matches!(r, IgnoreReason::ExcludeFlag { pattern } if pattern == "*.rs")
            })
        );
        Ok(())
    }
}
