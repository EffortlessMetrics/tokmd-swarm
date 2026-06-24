//! Render audience-specific Markdown from cross-tool packet bundles.

use std::fs;
use std::io::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use tokmd_format::render_packet_preset_markdown;
use tokmd_types::TokmdPacketsManifest;

use crate::cli;

const MANIFEST_NAME: &str = "tokmd-packets.json";

pub(crate) fn handle(args: cli::RenderArgs) -> Result<()> {
    let manifest = load_manifest(&args.from_packets)?;
    let preset = args.preset.as_str();
    let markdown = render_packet_preset_markdown(&manifest, preset)
        .with_context(|| format!("failed to render preset {preset}"))?;

    if let Some(output) = args.output {
        if let Some(parent) = output.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&output, &markdown)
            .with_context(|| format!("failed to write {}", output.display()))?;
    } else {
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(markdown.as_bytes())
            .context("failed to write rendered packet to stdout")?;
        if !markdown.ends_with('\n') {
            stdout
                .write_all(b"\n")
                .context("failed to write trailing newline")?;
        }
    }

    Ok(())
}

fn load_manifest(bundle_dir: &Path) -> Result<TokmdPacketsManifest> {
    let manifest_path = bundle_dir.join(MANIFEST_NAME);
    let raw = fs::read_to_string(&manifest_path).with_context(|| {
        format!(
            "failed to read packet manifest {}; pass a bundle directory with {MANIFEST_NAME}",
            manifest_path.display()
        )
    })?;
    let manifest: TokmdPacketsManifest = serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse packet manifest {}",
            manifest_path.display()
        )
    })?;
    if !manifest.schema_matches() {
        bail!(
            "unsupported packet schema {:?} in {}; expected tokmd.packets/v1",
            manifest.schema,
            manifest_path.display()
        );
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use tokmd_types::{PacketPresetInput, TOKMD_PACKETS_SCHEMA};

    #[test]
    fn load_manifest_reads_bundle_file() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = TokmdPacketsManifest {
            schema: TOKMD_PACKETS_SCHEMA.into(),
            producer: None,
            inputs_present: vec![],
            inputs_absent: vec![],
            non_claims: vec!["Does not prove UB.".into()],
            preset_inputs: BTreeMap::from([(
                "bun-ub-handoff".into(),
                PacketPresetInput {
                    sections: BTreeMap::from([("candidate_identity".into(), "seed-1".into())]),
                    limitations: vec![],
                    missing_sections: vec![],
                },
            )]),
        };
        fs::write(
            dir.path().join(MANIFEST_NAME),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        let loaded = load_manifest(dir.path()).unwrap();
        assert_eq!(loaded.schema, TOKMD_PACKETS_SCHEMA);
    }
}
