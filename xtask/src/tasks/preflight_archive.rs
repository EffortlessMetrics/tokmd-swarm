//! Offline content verification of the authenticated preflight ZIP.
use std::collections::BTreeSet;
use std::io::{self, Cursor, Read};

use anyhow::{Context, Result, ensure};
use zip::ZipArchive;

use super::{Manifest, ReleaseState, StateFact, state_fact};
use crate::tasks::release_preflight::{
    CommandStatus, ReleasePreflightReceipt, reaggregate_archive_input,
};

const MIB: u64 = 1024 * 1024;
const JSON_LIMIT: u64 = MIB;
const LOG_LIMIT: u64 = 64 * MIB;
const TOTAL_LIMIT: u64 = 256 * MIB;

type Archive = ZipArchive<Cursor<Vec<u8>>>;

fn field<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N]> {
    bytes
        .get(offset..)
        .and_then(|tail| tail.first_chunk())
        .copied()
        .context("truncated ZIP directory")
}

/// github/actions artifacts use a single disk. Bound metadata before the ZIP decoder
/// allocates its index. The raw count also exposes duplicates coalesced by that index.
fn directory(blob: &[u8]) -> Result<(usize, u64)> {
    let end = blob
        .len()
        .checked_sub(22)
        .context("missing ZIP end record")?;
    let trailer = blob.get(end..).context("missing ZIP end record")?;
    ensure!(
        trailer.starts_with(b"PK\x05\x06") && field::<2>(trailer, 20)? == [0; 2],
        "unsupported ZIP end record or archive comment"
    );
    ensure!(
        field::<4>(trailer, 4)? == [0; 4],
        "multi-disk ZIP is unsupported"
    );
    let locator = end
        .checked_sub(20)
        .and_then(|pos| blob.get(pos..end))
        .filter(|header| header.starts_with(b"PK\x06\x07"));
    let (count, size, start, directory_end) = if let Some(locator) = locator {
        ensure!(
            field::<4>(locator, 4)? == [0; 4] && u32::from_le_bytes(field(locator, 16)?) == 1,
            "multi-disk ZIP64 is unsupported"
        );
        let pos = usize::try_from(u64::from_le_bytes(field(locator, 8)?))?;
        let header = blob
            .get(pos..end - 20)
            .context("invalid ZIP64 end record")?;
        ensure!(
            header.len() == 56
                && header.starts_with(b"PK\x06\x06")
                && u64::from_le_bytes(field(header, 4)?) == 44
                && field::<8>(header, 16)? == [0; 8]
                && field::<8>(header, 24)? == field::<8>(header, 32)?,
            "invalid ZIP64 metadata"
        );
        (
            u64::from_le_bytes(field(header, 32)?),
            u64::from_le_bytes(field(header, 40)?),
            u64::from_le_bytes(field(header, 48)?),
            pos,
        )
    } else {
        ensure!(
            field::<2>(trailer, 8)? == field::<2>(trailer, 10)?,
            "inconsistent ZIP count"
        );
        (
            u64::from(u16::from_le_bytes(field(trailer, 10)?)),
            u64::from(u32::from_le_bytes(field(trailer, 12)?)),
            u64::from(u32::from_le_bytes(field(trailer, 16)?)),
            end,
        )
    };
    ensure!(
        count > 0
            && count <= 1024
            && size <= MIB
            && start.checked_add(size) == Some(u64::try_from(directory_end)?),
        "invalid or oversized ZIP directory"
    );
    // Traverse only the bounded central headers. An underreported count must not hide
    // another root receipt or log beyond the entries read by ZipArchive.
    let mut bytes = blob
        .get(usize::try_from(start)?..directory_end)
        .context("invalid ZIP directory range")?;
    for _ in 0..count {
        ensure!(
            bytes.starts_with(b"PK\x01\x02"),
            "invalid ZIP directory entry"
        );
        let name = usize::from(u16::from_le_bytes(field(bytes, 28)?));
        let extra = usize::from(u16::from_le_bytes(field(bytes, 30)?));
        let comment = usize::from(u16::from_le_bytes(field(bytes, 32)?));
        bytes = bytes
            .get(46 + name + extra + comment..)
            .context("truncated ZIP directory entry")?;
    }
    ensure!(bytes.is_empty(), "unaccounted ZIP directory entries");
    Ok((usize::try_from(count)?, start))
}

fn check_index(archive: &mut Archive) -> Result<()> {
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let member = archive.by_index_raw(index)?;
        let name = std::str::from_utf8(member.name_raw())?;
        ensure!(member.name()?.as_ref() == name, "ambiguous ZIP member name");
        let key = name.strip_suffix('/').unwrap_or(name);
        ensure!(
            !key.contains(['\\', '\0'])
                && !key.split('/').any(|part| matches!(part, "" | "." | ".."))
                && !key.split('/').next().is_some_and(|part| part.contains(':'))
                && names.insert(key.to_owned()),
            "duplicate or noncanonical ZIP path"
        );
        let kind = member.unix_mode().unwrap_or(0) & 0o170000;
        let regular = if member.is_dir() { 0o040000 } else { 0o100000 };
        ensure!(
            !member.encrypted() && (kind == 0 || kind == regular),
            "unsupported ZIP member type"
        );
        total = total
            .checked_add(member.size())
            .context("ZIP size overflow")?;
        ensure!(total <= TOTAL_LIMIT, "ZIP contents exceed limit");
    }
    Ok(())
}

fn read_member(
    archive: &mut Archive,
    name: &str,
    limit: u64,
    output: &mut impl io::Write,
) -> Result<()> {
    let member = archive
        .by_name(name)
        .with_context(|| format!("read preflight member {name}"))?;
    let size = member.size();
    ensure!(
        member.is_file() && size <= limit,
        "invalid size or type for {name}"
    );
    let actual = io::copy(&mut member.take(limit + 1), output)?;
    ensure!(actual == size, "inconsistent decompressed size for {name}");
    Ok(())
}

/// Open either authenticated Actions ZIP with the same path, size and count bounds.
pub(super) fn open_archive(blob: Vec<u8>) -> Result<ZipArchive<Cursor<Vec<u8>>>> {
    let (entries, start) = directory(&blob)?;
    let mut archive = ZipArchive::new(Cursor::new(blob))?;
    ensure!(
        archive.len() == entries && archive.central_directory_start() == start,
        "duplicate ZIP members or inconsistent central directory"
    );
    check_index(&mut archive)?;
    Ok(archive)
}

pub(super) fn verify_archive(
    blob: Vec<u8>,
    manifest: &Manifest,
    version: &str,
) -> Result<StateFact> {
    let mut archive = open_archive(blob)?;
    let mut input = Vec::new();
    read_member(&mut archive, "input.json", JSON_LIMIT, &mut input)?;
    let expected = reaggregate_archive_input(&input)?;
    let mut receipt_bytes = Vec::new();
    read_member(&mut archive, "receipt.json", JSON_LIMIT, &mut receipt_bytes)?;
    // Typed deserialization rejects duplicate fields and wrong scalar types; comparing
    // the JSON shape also rejects unknown or omitted fields, including optional ones.
    let receipt: ReleasePreflightReceipt = serde_json::from_slice(&receipt_bytes)?;
    let recorded: serde_json::Value = serde_json::from_slice(&receipt_bytes)?;
    ensure!(
        receipt == expected && recorded == serde_json::to_value(&expected)?,
        "archived preflight receipt differs from the producer aggregate"
    );
    ensure!(
        receipt.source_sha == manifest.source_sha
            && receipt.affected_base_sha == manifest.affected_base_sha
            && receipt.expected_version == version
            && receipt.release_kind == manifest.release_kind,
        "preflight source, affected base, version or release kind differs from manifest"
    );
    for row in &receipt.required_commands {
        let name = format!("logs/{}.log", row.id);
        read_member(&mut archive, &name, LOG_LIMIT, &mut io::sink())?;
    }
    let state = match receipt.overall {
        CommandStatus::Passed => ReleaseState::Passed,
        CommandStatus::Failed => ReleaseState::Failed,
        CommandStatus::NotRun => ReleaseState::NotRun,
        CommandStatus::Cancelled | CommandStatus::Unavailable => ReleaseState::Unavailable,
    };
    Ok(state_fact(
        state,
        &format!("validated v2 preflight aggregate: {:?}", receipt.overall),
        None,
    ))
}
