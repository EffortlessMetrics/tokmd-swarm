//! Syntax receipt command parser types.

use std::path::PathBuf;

use clap::Args;
use serde::{Deserialize, Serialize};

#[derive(Args, Debug, Clone, Serialize, Deserialize)]
#[command(
    after_help = "Examples:\n  tokmd syntax src/runtime/api\n  tokmd syntax --max-bytes 262144 src/runtime/api src/bun.js/bindings"
)]
pub struct SyntaxArgs {
    /// Maximum bytes per file before syntax parsing is skipped.
    #[arg(long, default_value_t = tokmd_analysis::ast::DEFAULT_MAX_SYNTAX_BYTES)]
    pub max_bytes: usize,

    /// Include generated and vendor paths instead of recording policy skips.
    #[arg(long)]
    pub include_generated_vendor: bool,

    /// Paths to parse into advisory syntax receipts.
    #[arg(value_name = "PATH", required = true)]
    pub paths: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::cli::parser::{Cli, Commands};

    #[test]
    fn syntax_args_parse_paths_and_limits() {
        let cli = Cli::try_parse_from([
            "tokmd",
            "syntax",
            "--max-bytes",
            "4096",
            "--include-generated-vendor",
            "src/runtime/api",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Commands::Syntax(args) => {
                assert_eq!(args.max_bytes, 4096);
                assert!(args.include_generated_vendor);
                assert_eq!(args.paths, vec![PathBuf::from("src/runtime/api")]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
