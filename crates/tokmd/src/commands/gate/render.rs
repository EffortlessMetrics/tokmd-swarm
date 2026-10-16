//! Result rendering for the `tokmd gate` command.

use std::io::{self, Write};

use anyhow::{Context, Result};
use tokmd_gate::RuleLevel;

use super::CombinedGateResult;

/// Print combined results in text format.
pub(super) fn print_text_result(result: &CombinedGateResult) {
    let policy_count = result
        .policy
        .as_ref()
        .map(|p| p.rule_results.len())
        .unwrap_or(0);
    let ratchet_count = result
        .ratchet
        .as_ref()
        .map(|r| r.ratchet_results.len())
        .unwrap_or(0);
    let total_rules = policy_count + ratchet_count;

    if result.passed {
        println!("Gate PASSED ({} rules evaluated)", total_rules);
    } else {
        println!(
            "Gate FAILED: {} error(s), {} warning(s)",
            result.total_errors, result.total_warnings
        );
    }

    println!();

    if let Some(policy) = &result.policy
        && !policy.rule_results.is_empty()
    {
        println!("Policy Rules:");
        for rule_result in &policy.rule_results {
            let status = if rule_result.passed { "PASS" } else { "FAIL" };
            let level = match rule_result.level {
                RuleLevel::Error => "error",
                RuleLevel::Warn => "warn",
            };

            if rule_result.passed {
                println!("  [{}] {} ({})", status, rule_result.name, level);
            } else {
                println!("  [{}] {} ({})", status, rule_result.name, level);
                println!("        Expected: {}", rule_result.expected);
                if let Some(actual) = &rule_result.actual {
                    println!("        Actual: {}", actual);
                }
                if let Some(msg) = &rule_result.message {
                    println!("        Message: {}", msg);
                }
            }
        }
        println!();
    }

    if let Some(ratchet) = &result.ratchet
        && !ratchet.ratchet_results.is_empty()
    {
        println!("Ratchet Rules:");
        for ratchet_result in &ratchet.ratchet_results {
            let status = if ratchet_result.passed {
                "PASS"
            } else {
                "FAIL"
            };
            let level = match ratchet_result.rule.level {
                RuleLevel::Error => "error",
                RuleLevel::Warn => "warn",
            };

            let name = ratchet_result
                .rule
                .description
                .as_deref()
                .unwrap_or(&ratchet_result.rule.pointer);

            println!("  [{}] {} ({})", status, name, level);

            if let Some(baseline) = ratchet_result.baseline_value {
                if let Some(pct) = ratchet_result.change_pct {
                    println!(
                        "        Baseline: {:.2} -> Current: {:.2} ({:+.2}%)",
                        baseline, ratchet_result.current_value, pct
                    );
                } else {
                    println!(
                        "        Baseline: {:.2}, Current: {:.2}",
                        baseline, ratchet_result.current_value
                    );
                }
            } else {
                println!("        Current: {:.2}", ratchet_result.current_value);
            }

            if !ratchet_result.passed {
                println!("        Message: {}", ratchet_result.message);
            }
        }
    }
}

/// Print combined results in JSON format.
pub(super) fn print_json_result(result: &CombinedGateResult) -> Result<()> {
    let json = serde_json::to_string_pretty(&result)?;
    write_json_output(&mut io::stdout().lock(), &json)
}

fn write_json_output(out: &mut impl Write, json: &str) -> Result<()> {
    writeln!(out, "{json}").context("Failed to write gate JSON to stdout")?;
    // A failing gate exits directly after rendering; detect buffered failures
    // before its policy verdict can be returned to the caller.
    out.flush().context("Failed to flush gate JSON to stdout")
}

#[cfg(test)]
mod tests {
    use super::write_json_output;
    use anyhow::{Context, Result};
    use std::io::{self, Write};

    struct Writer {
        bytes: Vec<u8>,
        fail_at: Option<usize>,
        fail_flush: bool,
        flushes: usize,
    }

    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_at == Some(self.bytes.len()) {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            let Some(&byte) = bytes.first() else {
                return Ok(0);
            };
            self.bytes.push(byte);
            Ok(1)
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
    fn short_writes_preserve_literal_json_and_flush() -> Result<()> {
        let mut out = writer(None, false);
        write_json_output(&mut out, "{}")?;
        assert_eq!(out.bytes, b"{}\n");
        assert_eq!(out.flushes, 1);
        Ok(())
    }

    #[test]
    fn initial_partial_and_newline_errors_preserve_io_cause() -> Result<()> {
        for offset in [0, 1, 2] {
            let mut out = writer(Some(offset), false);
            let error = write_json_output(&mut out, "{}")
                .err()
                .context("expected write failure")?;
            assert_eq!(error.to_string(), "Failed to write gate JSON to stdout");
            assert_eq!(
                error
                    .downcast_ref::<io::Error>()
                    .context("I/O cause")?
                    .kind(),
                io::ErrorKind::BrokenPipe
            );
            assert_eq!(crate::exit_code(&error), 1);
            assert_eq!(out.bytes, b"{}\n".get(..offset).context("output prefix")?);
            assert_eq!(out.flushes, 0);
        }
        Ok(())
    }

    #[test]
    fn flush_error_is_returned_after_complete_json() -> Result<()> {
        let mut out = writer(None, true);
        let error = write_json_output(&mut out, "{}")
            .err()
            .context("expected flush failure")?;
        assert_eq!(error.to_string(), "Failed to flush gate JSON to stdout");
        let cause = error.downcast_ref::<io::Error>().context("I/O cause")?;
        assert_eq!(cause.kind(), io::ErrorKind::Other);
        assert_eq!(cause.to_string(), "flush rejected");
        assert_eq!(crate::exit_code(&error), 1);
        assert_eq!(out.bytes, b"{}\n");
        assert_eq!(out.flushes, 1);
        Ok(())
    }
}
