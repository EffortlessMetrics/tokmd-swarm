//! Fixed-read, bounded-retention UTF-8 excerpts. No line-sized allocation.

use std::collections::VecDeque;
use std::io::{self, Read, Write};

use tokmd_types::ContextFileRow;

pub(super) const READ_BYTES: usize = 8 * 1024;

pub(super) struct Excerpt {
    head: Vec<u8>,
    tail: VecDeque<u8>,
    total_bytes: usize,
    total_lines: usize,
    #[cfg(test)]
    pub(super) max_retained: usize,
    #[cfg(test)]
    pub(super) max_capacity: usize,
}

pub(super) fn capture<R: Read>(reader: &mut R, allowance: usize) -> io::Result<Excerpt> {
    let mut result = Excerpt {
        head: Vec::new(),
        tail: VecDeque::new(),
        total_bytes: 0,
        total_lines: 0,
        #[cfg(test)]
        max_retained: 0,
        #[cfg(test)]
        max_capacity: 0,
    };
    // The extra three bytes carry an incomplete UTF-8 scalar across reads.
    let mut buffer = [0_u8; READ_BYTES + 3];
    let mut pending = 0;
    let mut last = None;
    loop {
        let read_buffer = buffer
            .get_mut(pending..pending + READ_BYTES)
            .ok_or_else(|| io::Error::other("invalid UTF-8 carry range"))?;
        let n = reader.read(read_buffer)?;
        if n == 0 {
            if pending != 0 {
                return Err(invalid_utf8("incomplete UTF-8 scalar"));
            }
            break;
        }
        result.total_bytes = result
            .total_bytes
            .checked_add(n)
            .ok_or_else(|| io::Error::other("input byte count overflow"))?;
        let length = pending + n;
        let bytes = buffer
            .get(..length)
            .ok_or_else(|| io::Error::other("invalid read range"))?;
        let valid = match std::str::from_utf8(bytes) {
            Ok(_) => length,
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(error) => return Err(invalid_utf8(error)),
        };
        let bytes = buffer
            .get(..valid)
            .ok_or_else(|| io::Error::other("invalid UTF-8 range"))?;
        result.total_lines += bytes.iter().filter(|b| **b == b'\n').count();
        last = bytes.last().copied().or(last);
        let head_add = allowance.saturating_sub(result.head.len()).min(bytes.len());
        result.head.extend_from_slice(
            bytes
                .get(..head_add)
                .ok_or_else(|| io::Error::other("invalid head range"))?,
        );
        if allowance > 0 {
            if bytes.len() >= allowance {
                result.tail.clear();
                result.tail.extend(
                    bytes
                        .get(bytes.len() - allowance..)
                        .ok_or_else(|| io::Error::other("invalid tail range"))?,
                );
            } else {
                let excess = result
                    .tail
                    .len()
                    .saturating_add(bytes.len())
                    .saturating_sub(allowance);
                result.tail.drain(..excess);
                result.tail.extend(bytes);
            }
        }
        #[cfg(test)]
        {
            result.max_retained = result
                .max_retained
                .max(result.head.len() + result.tail.len());
            result.max_capacity = result
                .max_capacity
                .max(result.head.capacity() + result.tail.capacity());
        }
        pending = length - valid;
        buffer.copy_within(valid..length, 0);
    }
    result.total_lines += usize::from(last.is_some_and(|byte| byte != b'\n'));
    Ok(result)
}

fn invalid_utf8(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn prefix(text: &str, allowance: usize) -> &str {
    let mut end = allowance.min(text.len());
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    text.get(..end).unwrap_or_default()
}

fn suffix(text: &str, allowance: usize) -> &str {
    let mut start = text.len().saturating_sub(allowance);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text.get(start..).unwrap_or_default()
}

fn first_lines(text: &str, count: usize) -> &str {
    let bytes = text.split_inclusive('\n').take(count).map(str::len).sum();
    text.get(..bytes).unwrap_or_default()
}

fn last_lines(text: &str, count: usize) -> &str {
    let bytes: usize = text
        .split_inclusive('\n')
        .rev()
        .take(count)
        .map(str::len)
        .sum();
    text.get(text.len().saturating_sub(bytes)..)
        .unwrap_or_default()
}

fn fragment<W: Write>(writer: &mut W, text: &str, compress: bool) -> io::Result<()> {
    for line in text.lines() {
        if !compress || !line.trim().is_empty() {
            writeln!(writer, "{line}")?;
        }
    }
    Ok(())
}

pub(super) fn write<W: Write>(
    writer: &mut W,
    mut excerpt: Excerpt,
    file: &ContextFileRow,
    compress: bool,
    allowance: usize,
) -> io::Result<()> {
    let head_valid = match std::str::from_utf8(&excerpt.head) {
        Ok(text) => text.len(),
        Err(error) => error.valid_up_to(),
    };
    let head = std::str::from_utf8(excerpt.head.get(..head_valid).unwrap_or_default())
        .map_err(invalid_utf8)?;
    if excerpt.total_bytes <= allowance {
        return fragment(writer, head, compress);
    }
    let tail_bytes = excerpt.tail.make_contiguous();
    let tail_start = tail_bytes
        .iter()
        .position(|b| b & 0xc0 != 0x80)
        .unwrap_or(tail_bytes.len());
    let tail = std::str::from_utf8(tail_bytes.get(tail_start..).unwrap_or_default())
        .map_err(invalid_utf8)?;
    let effective = file.effective_tokens.unwrap_or(file.tokens);
    let density = file.tokens as f64 / excerpt.total_lines.max(1) as f64;
    let target = if density > 0.0 {
        (effective as f64 / density).ceil() as usize
    } else {
        excerpt.total_lines
    };
    let head_lines = (target as f64 * 0.6).ceil() as usize;
    let tail_lines = target.saturating_sub(head_lines);
    let (head, tail) = if target >= excerpt.total_lines {
        (head, tail)
    } else {
        (first_lines(head, head_lines), last_lines(tail, tail_lines))
    };
    // Reserve 40% for the tail; unused tail space is available to the head.
    // If that split cannot fit the final scalar, keep one scalar from each
    // end whenever the total allowance can accommodate both.
    // Captures overlap for short inputs, which the full-input branch above
    // emits once. Here retained head+tail <= allowance < total input bytes.
    let tail_allowance = allowance / 5 * 2 + allowance % 5 * 2 / 5;
    let mut kept_tail = suffix(tail, tail_allowance);
    if kept_tail.is_empty() {
        let first_bytes = head.chars().next().map_or(0, char::len_utf8);
        let last_bytes = tail.chars().next_back().map_or(0, char::len_utf8);
        if first_bytes > 0 && last_bytes > 0 && first_bytes.saturating_add(last_bytes) <= allowance
        {
            kept_tail = suffix(tail, last_bytes);
        }
    }
    let tail = kept_tail;
    let head = prefix(head, allowance.saturating_sub(tail.len()));
    fragment(writer, head, compress)?;
    let omitted = excerpt.total_bytes.saturating_sub(head.len() + tail.len());
    writeln!(writer, "// ... [{omitted} bytes omitted] ...")?;
    fragment(writer, tail, compress)
}
