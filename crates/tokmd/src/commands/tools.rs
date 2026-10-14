//! Handler for the `tokmd tools` command.

use crate::cli;
use crate::tool_schema::{build_tool_schema, render_output};
use anyhow::{Context, Result};
use clap::CommandFactory;
use std::io::{self, Write};

/// Handle the tools command.
pub(crate) fn handle(args: cli::ToolsArgs) -> Result<()> {
    let cmd = cli::Cli::command();
    let schema = build_tool_schema(&cmd);
    let output = render_output(&schema, args.format, args.pretty)?;
    write_output(&mut io::stdout().lock(), &output)
}

fn write_output(out: &mut impl Write, output: &str) -> Result<()> {
    out.write_all(output.as_bytes())
        .and_then(|()| out.write_all(b"\n"))
        .context("Failed to write tool schema to stdout")?;
    out.flush().context("Failed to flush tool schema to stdout")
}

#[cfg(test)]
mod tests {
    use super::write_output;
    use std::io::{self, Write};

    const PAYLOAD: &str = r#"{"name":"fixture"}"#;
    const EXPECTED: &[u8] = b"{\"name\":\"fixture\"}\n";

    struct Writer {
        bytes: Vec<u8>,
        fail_at: Option<usize>,
        fail_flush: bool,
        flushes: usize,
    }

    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_at == Some(self.bytes.len()) {
                self.fail_at = None;
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "write rejected"));
            }
            // Successful writes deliberately accept one byte at a time.
            // A single write instead of write_all must not pass this oracle.
            let len = bytes.len().min(1);
            self.bytes.extend_from_slice(&bytes[..len]);
            Ok(len)
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
    fn literal_output_survives_short_writes_and_flushes() {
        let mut out = writer(None, false);
        write_output(&mut out, PAYLOAD).unwrap();
        assert_eq!(out.bytes, EXPECTED);
        assert_eq!(out.flushes, 1);
    }

    #[test]
    fn initial_partial_and_newline_errors_preserve_native_cause() {
        for offset in [0, 4, PAYLOAD.len()] {
            let mut out = writer(Some(offset), false);
            let err = write_output(&mut out, PAYLOAD).unwrap_err();
            assert_eq!(err.to_string(), "Failed to write tool schema to stdout");
            let cause = err.downcast_ref::<io::Error>().unwrap();
            assert_eq!(cause.kind(), io::ErrorKind::BrokenPipe);
            assert_eq!(cause.to_string(), "write rejected");
            assert_eq!(crate::exit_code(&err), 1);
            assert_eq!(out.bytes, EXPECTED[..offset]);
            assert_eq!(out.flushes, 0);
        }
    }

    #[test]
    fn flush_error_preserves_native_cause_after_complete_output() {
        let mut out = writer(None, true);
        let err = write_output(&mut out, PAYLOAD).unwrap_err();
        assert_eq!(err.to_string(), "Failed to flush tool schema to stdout");
        let cause = err.downcast_ref::<io::Error>().unwrap();
        assert_eq!(cause.kind(), io::ErrorKind::Other);
        assert_eq!(cause.to_string(), "flush rejected");
        assert_eq!(crate::exit_code(&err), 1);
        assert_eq!(out.bytes, EXPECTED);
        assert_eq!(out.flushes, 1);
    }
}
