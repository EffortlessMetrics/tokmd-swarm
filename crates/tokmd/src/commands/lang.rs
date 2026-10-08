use crate::cli;
use anyhow::Result;
use clap::ValueEnum;
use tokmd_format as format;
use tokmd_model as model;
use tokmd_scan as scan;
use tokmd_settings::ScanOptions;

use crate::config::{self, ResolvedConfig};
use crate::progress::Progress;

pub(crate) fn handle(
    cli_args: cli::CliLangArgs,
    global: &cli::GlobalArgs,
    resolved: &ResolvedConfig,
) -> Result<()> {
    if cli_args.format.is_none()
        && let Some(value) = resolved.format()
        && cli::TableFormat::from_str(value, true).is_err()
    {
        let source = if resolved
            .toml_view
            .and_then(|view| view.format.as_deref())
            .is_some()
        {
            resolved.toml_path.map_or_else(
                || "the selected TOML view".to_string(),
                |path| format!("the selected TOML config {}", path.display()),
            )
        } else {
            "the selected profile".to_string()
        };
        return Err(anyhow::Error::new(
            crate::error_hints::InvalidLangProfileFormat(format!(
                "Invalid lang format {value:?} in {source}; expected md, tsv, json. Fix the selected profile or pass --format json."
            )),
        ));
    }

    let args = config::resolve_lang_with_config(&cli_args, resolved);
    let scan_opts = ScanOptions::from(global);

    let progress = Progress::new(!global.no_progress);
    progress.set_message("Scanning codebase...");
    let languages = scan::scan(&args.paths, &scan_opts)?;
    let report = model::create_lang_report(&languages, args.top, args.files, args.children);
    // Clear the stderr spinner before the report is written to stdout.
    progress.finish_and_clear();

    format::print_lang_report(&report, &scan_opts, &args)?;
    Ok(())
}
