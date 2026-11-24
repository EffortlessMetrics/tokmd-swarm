use crate::cli;
use anyhow::{Context, Result};
use clap::CommandFactory;
use std::io::{self, Write};

pub(crate) fn handle(args: cli::CompletionsArgs) -> Result<()> {
    use clap_complete::generate;
    let mut cmd = cli::Cli::command();
    let name = cmd.get_name().to_string();
    let shell = match args.shell {
        cli::Shell::Bash => clap_complete::Shell::Bash,
        cli::Shell::Elvish => clap_complete::Shell::Elvish,
        cli::Shell::Fish => clap_complete::Shell::Fish,
        cli::Shell::Powershell => clap_complete::Shell::PowerShell,
        cli::Shell::Zsh => clap_complete::Shell::Zsh,
    };
    // Generators can panic on writer errors (including Fish's try_generate
    // helper), so give them an infallible buffer before touching stdout.
    let mut output = Vec::new();
    generate(shell, &mut cmd, name, &mut output);
    write_output(&mut io::stdout().lock(), &output)
}

fn write_output(out: &mut impl Write, output: &[u8]) -> Result<()> {
    out.write_all(output)
        .context("Failed to write completions to stdout")?;
    out.flush().context("Failed to flush completions to stdout")
}

#[cfg(test)]
mod tests {
    use super::write_output;
    use anyhow::{Context, Result};
    use std::io::{self, Write};

    const SCRIPT: &[u8] = b"# completions\ncomplete -c tokmd\n";

    #[derive(Default)]
    struct Writer {
        bytes: Vec<u8>,
        fail_at: Option<usize>,
        fail_flush: bool,
        flushes: usize,
    }

    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_at == Some(self.bytes.len()) {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "write rejected"));
            }
            // Force write_all to make progress through repeated short writes.
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

    #[test]
    fn short_writes_preserve_script_bytes_and_flush() -> Result<()> {
        let mut out = Writer::default();
        write_output(&mut out, SCRIPT)?;
        assert_eq!(out.bytes, SCRIPT);
        assert_eq!(out.flushes, 1);
        Ok(())
    }

    #[test]
    fn initial_and_partial_write_errors_preserve_native_cause() -> Result<()> {
        for offset in [0, 4, SCRIPT.len() - 1] {
            let mut out = Writer {
                fail_at: Some(offset),
                ..Writer::default()
            };
            let err = write_output(&mut out, SCRIPT)
                .err()
                .context("expected write failure")?;
            assert_eq!(err.to_string(), "Failed to write completions to stdout");
            let cause = err
                .downcast_ref::<io::Error>()
                .context("native I/O cause")?;
            assert_eq!(cause.kind(), io::ErrorKind::BrokenPipe);
            assert_eq!(cause.to_string(), "write rejected");
            assert_eq!(crate::exit_code(&err), 1);
            assert_eq!(out.bytes, SCRIPT[..offset]);
            assert_eq!(out.flushes, 0);
        }
        Ok(())
    }

    #[test]
    fn flush_errors_preserve_native_cause_after_complete_script() -> Result<()> {
        let mut out = Writer {
            fail_flush: true,
            ..Writer::default()
        };
        let err = write_output(&mut out, SCRIPT)
            .err()
            .context("expected flush failure")?;
        assert_eq!(err.to_string(), "Failed to flush completions to stdout");
        let cause = err
            .downcast_ref::<io::Error>()
            .context("native I/O cause")?;
        assert_eq!(cause.kind(), io::ErrorKind::Other);
        assert_eq!(cause.to_string(), "flush rejected");
        assert_eq!(crate::exit_code(&err), 1);
        assert_eq!(out.bytes, SCRIPT);
        assert_eq!(out.flushes, 1);
        Ok(())
    }
}
