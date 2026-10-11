//! Byte-bound controls independent of line-density estimates.

use std::io::{self, Read, Write};

use anyhow::{Result, ensure};
use tokmd_types::{ContextFileRow, InclusionPolicy};

use super::{CountingWriter, head_tail, write_head_tail};

fn row(tokens: usize) -> ContextFileRow {
    ContextFileRow {
        path: "long.rs".into(),
        module: "(root)".into(),
        lang: "Rust".into(),
        tokens: 4_490,
        code: 0,
        lines: 200,
        bytes: 8_000,
        value: 0,
        rank_reason: "test".into(),
        policy: InclusionPolicy::HeadTail,
        effective_tokens: Some(tokens),
        policy_reason: None,
        classifications: Vec::new(),
    }
}

// 20 decimal digits suffice for any usize on supported targets. This includes
// the omission marker and up to two inserted fragment-final newlines. The
// caller's path header and final separator are counted separately in CLI proof.
const BODY_FRAMING_BOUND: usize = 64;

#[test]
fn context_pack_head_tail_giant_lines_obey_actual_byte_allowance() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("long.rs");
    for last in [false, true] {
        let giant = format!("{}\n", "x".repeat(10_000));
        let short = "y".repeat(39) + "\n";
        let content = if last {
            short.repeat(199) + &giant
        } else {
            giant + &short.repeat(199)
        };
        std::fs::write(&path, content)?;
        for tokens in [0, 1, 2, 200] {
            for compress in [false, true] {
                let mut output = CountingWriter::new(Vec::new());
                write_head_tail(&mut output, &path, &row(tokens), compress)?;
                ensure!(
                    output.bytes() <= (tokens * 4 + BODY_FRAMING_BOUND) as u64,
                    "giant line escaped explicit byte allowance: last={last} tokens={tokens} compress={compress} bytes={}",
                    output.bytes()
                );
            }
        }
    }
    Ok(())
}

#[test]
fn context_pack_head_tail_single_line_and_utf8_boundaries_are_bounded() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("single.rs");
    for content in ["é🙂".repeat(3_000), "x".repeat(20_000)] {
        std::fs::write(&path, content)?;
        for tokens in [0, 1, 2, 17] {
            for compress in [false, true] {
                let mut bytes = Vec::new();
                write_head_tail(&mut bytes, &path, &row(tokens), compress)?;
                ensure!(
                    std::str::from_utf8(&bytes).is_ok(),
                    "UTF-8 boundary was split"
                );
                ensure!(
                    bytes.len() <= tokens * 4 + BODY_FRAMING_BOUND,
                    "single line escaped allowance"
                );
                ensure!(
                    !bytes.windows(3).any(|b| b == [0xef, 0xbf, 0xbd]),
                    "renderer silently repaired bytes"
                );
            }
        }
    }
    Ok(())
}

struct FaultWriter {
    bytes: Vec<u8>,
    remaining: usize,
}
impl Write for FaultWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            // Fail once: swallowing this error would allow later writes to
            // succeed, so the non-success control is discriminating.
            self.remaining = usize::MAX;
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "injected writer fault",
            ));
        }
        let n = self.remaining.min(bytes.len());
        self.bytes.extend_from_slice(
            bytes
                .get(..n)
                .ok_or_else(|| io::Error::other("invalid test range"))?,
        );
        self.remaining -= n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn context_pack_head_tail_writer_fault_keeps_partial_byte_count() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("input.rs");
    std::fs::write(
        &path,
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
    )?;
    for stop in [0, 1, 8, 19] {
        let mut writer = CountingWriter::new(FaultWriter {
            bytes: Vec::new(),
            remaining: stop,
        });
        let error = match write_head_tail(&mut writer, &path, &row(4), false) {
            Ok(()) => anyhow::bail!("injected writer fault unexpectedly succeeded"),
            Err(error) => error,
        };
        ensure!(
            error
                .downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe),
            "lost writer IO cause"
        );
        ensure!(
            writer.bytes() == stop as u64,
            "partial byte accounting is wrong"
        );
    }
    Ok(())
}

struct GeneratedReader {
    remaining: usize,
    max_requested: usize,
    read: usize,
    fail_after: Option<usize>,
}

impl Read for GeneratedReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.max_requested = self.max_requested.max(bytes.len());
        if bytes.len() > 8 * 1024 {
            return Err(io::Error::other("reader was given an unbounded buffer"));
        }
        if self.fail_after.is_some_and(|stop| self.read >= stop) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected omitted-middle read fault",
            ));
        }
        let n = bytes.len().min(self.remaining);
        bytes
            .get_mut(..n)
            .ok_or_else(|| io::Error::other("bad generated-reader range"))?
            .fill(b'x');
        self.remaining -= n;
        self.read += n;
        Ok(n)
    }
}

#[test]
fn context_pack_head_tail_retention_is_independent_of_input_size() -> Result<()> {
    for input_bytes in [512 * 1024, 16 * 1024 * 1024] {
        for allowance in [0, 1, 800] {
            let mut reader = GeneratedReader {
                remaining: input_bytes,
                max_requested: 0,
                read: 0,
                fail_after: None,
            };
            let excerpt = head_tail::capture(&mut reader, allowance)?;
            ensure!(
                reader.read == input_bytes,
                "renderer stopped before validating omitted input"
            );
            ensure!(
                reader.max_requested == 8 * 1024,
                "read size scales with input"
            );
            ensure!(
                excerpt.max_retained <= allowance * 2,
                "retention scales with input"
            );
            ensure!(
                excerpt.max_capacity <= allowance * 4 + 16,
                "allocated capture capacity scales with input"
            );
            eprintln!(
                "input={input_bytes} allowance={allowance} max_read={} retained={} capacity={}",
                reader.max_requested, excerpt.max_retained, excerpt.max_capacity
            );
            let mut bytes = Vec::new();
            head_tail::write(&mut bytes, excerpt, &row(200), false, allowance)?;
            ensure!(
                bytes.len() <= allowance + BODY_FRAMING_BOUND,
                "synthetic excerpt escaped bound"
            );
        }
    }
    Ok(())
}

#[test]
fn context_pack_head_tail_utf8_clipping_keeps_useful_distinct_ends() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("unicode.rs");
    std::fs::write(&path, "éé🙂middle🙂Z")?;
    for compress in [false, true] {
        let mut output = Vec::new();
        write_head_tail(&mut output, &path, &row(2), compress)?;
        ensure!(
            output == "éé\n// ... [14 bytes omitted] ...\nZ\n".as_bytes(),
            "UTF-8 clipping dropped useful ends or overlapped ranges"
        );
    }
    std::fs::write(&path, "head0123456789middleéé🙂Z")?;
    for compress in [false, true] {
        let mut output = Vec::new();
        write_head_tail(&mut output, &path, &row(5), compress)?;
        ensure!(
            output == "head012345678\n// ... [9 bytes omitted] ...\né🙂Z\n".as_bytes(),
            "tail clipping should preserve all complete scalars within its share"
        );
    }
    Ok(())
}

#[test]
fn context_pack_head_tail_omitted_read_fault_is_required() -> Result<()> {
    let mut reader = GeneratedReader {
        remaining: 64 * 1024,
        max_requested: 0,
        read: 0,
        fail_after: Some(16 * 1024),
    };
    let error = match head_tail::capture(&mut reader, 16) {
        Ok(_) => anyhow::bail!("omitted-middle read fault was ignored"),
        Err(error) => error,
    };
    ensure!(
        error.kind() == io::ErrorKind::PermissionDenied,
        "original read cause lost"
    );
    ensure!(
        error.to_string().contains("injected omitted-middle"),
        "original IO diagnostic lost"
    );
    Ok(())
}

#[test]
fn context_pack_head_tail_small_exact_and_overlapping_inputs_are_emitted_once() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("boundary.rs");
    for (content, tokens, expected) in [
        ("", 0, ""),
        ("a\nb\n", 1, "a\nb\n"),
        ("éé", 1, "éé\n"),
        ("abcd", 1, "abcd\n"),
        ("a\r\nb\r\n", 2, "a\nb\n"),
    ] {
        for compress in [false, true] {
            std::fs::write(&path, content)?;
            let mut output = Vec::new();
            write_head_tail(&mut output, &path, &row(tokens), compress)?;
            ensure!(
                output == expected.as_bytes(),
                "short input was duplicated or changed: {content:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn context_pack_head_tail_rebalances_four_byte_final_scalar() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("scalar.rs");
    std::fs::write(&path, "😃😃middle🙂")?;
    for (tokens, expected) in [
        (2, "😃\n// ... [10 bytes omitted] ...\n🙂\n"),
        (1, "😃\n// ... [14 bytes omitted] ...\n"),
    ] {
        for compress in [false, true] {
            let mut output = Vec::new();
            write_head_tail(&mut output, &path, &row(tokens), compress)?;
            ensure!(
                output == expected.as_bytes(),
                "final scalar should survive when both ends fit: tokens={tokens} compress={compress}"
            );
        }
    }
    Ok(())
}

#[test]
fn context_pack_head_tail_blank_tail_preserves_visible_ends() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("line_end.rs");
    for (content, tokens, expected) in [
        ("abcdefZ\n", 1, "ab\n// ... [4 bytes omitted] ...\nZ\n"),
        ("abcdefZ\r\n", 1, "a\n// ... [5 bytes omitted] ...\nZ\n"),
        ("😃😃🙂\n", 1, "😃\n// ... [9 bytes omitted] ...\n"),
        ("😃😃🙂\r\n", 1, "😃\n// ... [10 bytes omitted] ...\n"),
        ("abcdefZ\n", 0, "// ... [8 bytes omitted] ...\n"),
        ("abcdefZ \n", 1, "a\n// ... [5 bytes omitted] ...\nZ \n"),
        ("abcdefZ\t\n", 1, "a\n// ... [5 bytes omitted] ...\nZ\t\n"),
        (
            "abcdefZ\u{2003}\n",
            2,
            "abc\n// ... [3 bytes omitted] ...\nZ\u{2003}\n",
        ),
    ] {
        std::fs::write(&path, content)?;
        for compress in [false, true] {
            let mut output = Vec::new();
            write_head_tail(&mut output, &path, &row(tokens), compress)?;
            ensure!(
                output == expected.as_bytes(),
                "blank tail hid visible ends: content={content:?} tokens={tokens} compress={compress}"
            );
        }
    }
    Ok(())
}

#[test]
fn context_pack_head_tail_blank_head_rebalances_without_losing_visible_tail() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("leading.rs");
    for (content, tokens, plain, compressed) in [
        (
            "     abcdefXYZ",
            2,
            "     a\n// ... [6 bytes omitted] ...\nYZ\n",
            "     a\n// ... [6 bytes omitted] ...\nYZ\n",
        ),
        (
            "\u{2003}😃abcdefZ",
            2,
            "\u{2003}😃\n// ... [6 bytes omitted] ...\nZ\n",
            "\u{2003}😃\n// ... [6 bytes omitted] ...\nZ\n",
        ),
        (
            "\nabcdeXYZ",
            2,
            "\nabcd\n// ... [1 bytes omitted] ...\nXYZ\n",
            "abcd\n// ... [1 bytes omitted] ...\nXYZ\n",
        ),
        (
            "    Z",
            1,
            "   \n// ... [1 bytes omitted] ...\nZ\n",
            "// ... [1 bytes omitted] ...\nZ\n",
        ),
        (
            "abcd    ",
            1,
            "abcd\n// ... [4 bytes omitted] ...\n",
            "abcd\n// ... [4 bytes omitted] ...\n",
        ),
    ] {
        std::fs::write(&path, content)?;
        for compress in [false, true] {
            let mut output = Vec::new();
            write_head_tail(&mut output, &path, &row(tokens), compress)?;
            let expected = if compress { compressed } else { plain };
            ensure!(
                output == expected.as_bytes(),
                "blank head discarded feasible visible content: content={content:?} compress={compress}"
            );
        }
    }
    Ok(())
}

struct OneByteReader<'a>(&'a [u8]);
impl Read for OneByteReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        match self.0.split_first() {
            None => Ok(0),
            Some((byte, remainder)) => {
                *output
                    .first_mut()
                    .ok_or_else(|| io::Error::other("empty read buffer"))? = *byte;
                self.0 = remainder;
                Ok(1)
            }
        }
    }
}

#[test]
fn context_pack_head_tail_utf8_crosses_read_boundaries_without_repair() -> Result<()> {
    for input in ["é🙂tail".as_bytes(), "headé🙂".as_bytes()] {
        let mut reader = OneByteReader(input);
        let excerpt = head_tail::capture(&mut reader, 4)?;
        let mut output = Vec::new();
        head_tail::write(&mut output, excerpt, &row(1), false, 4)?;
        ensure!(std::str::from_utf8(&output).is_ok(), "split UTF-8 scalar");
        ensure!(
            !output.windows(3).any(|b| b == [0xef, 0xbf, 0xbd]),
            "lossy repair"
        );
    }
    for input in [b"head\xfftail".as_slice(), b"head\xf0\x9f".as_slice()] {
        let mut reader = OneByteReader(input);
        let error = match head_tail::capture(&mut reader, 4) {
            Ok(_) => anyhow::bail!("invalid UTF-8 unexpectedly succeeded"),
            Err(error) => error,
        };
        ensure!(
            error.kind() == io::ErrorKind::InvalidData,
            "wrong malformed input cause"
        );
    }
    Ok(())
}
