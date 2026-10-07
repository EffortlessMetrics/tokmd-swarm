use anyhow::Error;
use clap::CommandFactory;

/// Subcommand names as clap actually knows them, including visible aliases.
///
/// Derived from the parser rather than hard-coded: the previous literal list
/// silently fell four commands behind (`syntax`, `evidence-packet`, `packet`,
/// `render`), so those typos got no suggestion. It also picks up feature-gated
/// commands only when they are compiled in.
fn known_subcommands() -> Vec<String> {
    crate::cli::Cli::command()
        .get_subcommands()
        .filter(|c| !c.is_hide_set())
        // Suggestions are rendered as command-position tokens. Keep this
        // pool canonical: visible aliases can be accepted by clap in some
        // contexts but are not necessarily valid as the suggested token.
        .map(|c| c.get_name().to_string())
        .filter(|name| name != "help")
        .collect()
}

/// Typed context for a failed baseline read whose path is a directory.
#[derive(Debug)]
pub(crate) struct BaselineDirectoryRead;

impl std::fmt::Display for BaselineDirectoryRead {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Expected a baseline JSON file, but found a directory")
    }
}

pub(crate) fn format(err: &Error) -> String {
    let mut out = if let Some(token) = missing_path_as_unrecognized_subcommand(err) {
        format!("Error: Unrecognized subcommand '{token}'")
    } else {
        format!("Error: {err:#}")
    };
    let mut hints = suggestions(err);
    if out.starts_with("Error: Unrecognized subcommand ") {
        hints.retain(|h| {
            !h.contains("was intended as a subcommand")
                && !h.contains("was meant to be a subcommand")
        });
    }
    if !hints.is_empty() {
        out.push_str("\n\nHints:\n");
        for hint in hints {
            out.push_str("- ");
            out.push_str(&hint);
            out.push('\n');
        }
    }
    out
}

fn missing_path_as_unrecognized_subcommand(err: &Error) -> Option<String> {
    for entry in err.chain() {
        let message = entry.to_string();
        let token = message
            .strip_prefix("Path not found: ")
            .or_else(|| message.strip_prefix("Input path does not exist: "));

        if let Some(token) = token {
            let token = token.trim();
            if looks_like_bare_subcommand_token(token) {
                return Some(token.to_string());
            }
        }
    }

    None
}

fn looks_like_bare_subcommand_token(token: &str) -> bool {
    !token.is_empty()
        && !token.starts_with('-')
        && !token.contains('/')
        && !token.contains('\\')
        && !token.contains('.')
        && !token.contains(':')
}

fn suggestions(err: &Error) -> Vec<String> {
    let chain: Vec<String> = err.chain().map(|e| e.to_string()).collect();
    let haystack = chain.join(" | ").to_ascii_lowercase();
    let mut out: Vec<String> = Vec::new();

    if err.downcast_ref::<BaselineDirectoryRead>().is_some() {
        push_hint(
            &mut out,
            "The baseline path is a directory. Select a JSON file, then retry.",
        );
        return out;
    }

    // Stable file failures can carry network keywords in their resource paths.
    // Match typed causes and local producer context, or explicit path markers,
    // before considering the text-only remote-service recovery hints.
    let missing_file = err.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    });
    let unparsed_io = missing_file
        || err.chain().any(|cause| {
            cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied)
        });
    let local_file_context = chain.iter().any(|message| {
        let message = message.to_ascii_lowercase();
        [
            "failed to access path ",
            "failed to read ",
            "failed to open ",
            "failed to resolve scan root ",
            "failed to resolve bounded path ",
            "failed to load policy from ",
            "failed to load ratchet config from ",
            "failed to load toml config from ",
            "failed to write ",
            "failed to create ",
        ]
        .iter()
        .any(|prefix| message.starts_with(prefix))
    });
    let explicit_missing_path = chain.iter().any(|message| {
        let message = message.to_ascii_lowercase();
        [
            "path not found: ",
            "input path does not exist: ",
            "bounded path not found: ",
        ]
        .iter()
        .any(|prefix| message.starts_with(prefix))
    });
    // Baselines are read as UTF-8 before JSON parsing. InvalidData in other
    // producers can represent parser failures, so keep this context specific.
    let invalid_baseline_data = err.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::InvalidData)
    }) && chain.iter().any(|message| {
        message
            .to_ascii_lowercase()
            .starts_with("failed to read baseline from ")
    });
    let stable_local_failure =
        (unparsed_io && local_file_context) || explicit_missing_path || invalid_baseline_data;

    if invalid_baseline_data {
        push_hint(
            &mut out,
            "Save the baseline file named above as valid UTF-8 text, then retry.",
        );
        return out;
    }

    if haystack.contains("git is not available on path")
        || haystack.contains("requires the 'git' feature")
    {
        push_hint(&mut out, "Install git and verify it with `git --version`.");
        push_hint(
            &mut out,
            "If git metrics are optional, disable them with `--no-git`.",
        );
    }

    if haystack.contains("not inside a git repository") {
        push_hint(
            &mut out,
            "Run the command from a git repository, or disable git-dependent behavior.",
        );
        push_hint(&mut out, "Initialize git first if needed: `git init`.");
    }

    if !stable_local_failure
        && (haystack.contains("rate limit")
            || haystack.contains("rate_limit")
            || haystack.contains("too many requests")
            || haystack.contains("http 429")
            || haystack.contains("status 429"))
    {
        push_hint(
            &mut out,
            "The upstream service is limiting requests. Wait briefly, then retry.",
        );
        push_hint(
            &mut out,
            "Honor provider retry windows such as `Retry-After` when available.",
        );
        push_hint(
            &mut out,
            "Use a smaller input scope if this command contacts a remote service.",
        );
    }

    if !stable_local_failure
        && (haystack.contains("timed out")
            || haystack.contains("timeout")
            || haystack.contains("temporary")
            || haystack.contains("temporarily")
            || haystack.contains("connection reset")
            || haystack.contains("connection refused")
            || haystack.contains("broken pipe")
            || haystack.contains("dns")
            || haystack.contains("network error")
            || haystack.contains("service unavailable")
            || haystack.contains("http 503")
            || haystack.contains("status 503"))
    {
        push_hint(
            &mut out,
            "This looks transient. Retry with backoff after network or service health recovers.",
        );
        push_hint(
            &mut out,
            "Check network, VPN, or proxy settings if retries keep failing.",
        );
    }

    if haystack.contains("parent traversal")
        || haystack.contains("must be relative")
        || haystack.contains("escapes scan root")
        || haystack.contains("scan root must not be empty")
        || haystack.contains("bounded path must not be empty")
    {
        push_hint(
            &mut out,
            "Pass paths inside the selected scan root; parent traversal (`..`) is rejected.",
        );
        push_hint(
            &mut out,
            "Use root-relative paths for scanned entries, or choose the containing directory as the root.",
        );

        if haystack.contains("escapes scan root") {
            push_hint(
                &mut out,
                "Avoid symlinked or redirected paths that resolve outside the scan root.",
            );
        }
    }

    // OS error messages vary by platform and locale. Use the typed cause before
    // falling back to legacy string-only missing-path diagnostics.
    if missing_file
        || haystack.contains("path not found")
        || haystack.contains("input path does not exist")
        || haystack.contains("no such file or directory")
    {
        let mut did_you_mean = false;

        let mut extracted_bad_path = None;

        // Only an explicit missing bare path can be a typoed subcommand.
        if haystack.contains("path not found") || haystack.contains("input path does not exist") {
            // Find the original path string from the chain
            for e in err.chain() {
                let e_str = e.to_string();
                if let Some(bad_path) = e_str
                    .strip_prefix("Path not found: ")
                    .or_else(|| e_str.strip_prefix("Input path does not exist: "))
                {
                    let bad_path = bad_path.trim();
                    extracted_bad_path = Some(bad_path.to_string());
                    if looks_like_bare_subcommand_token(bad_path) {
                        let known = known_subcommands();

                        let mut best_match = None;
                        let mut best_dist = usize::MAX;

                        for k in &known {
                            let d = levenshtein(bad_path, k);
                            if d < best_dist {
                                best_dist = d;
                                best_match = Some(k.as_str());
                            }
                        }

                        if let Some(m) = best_match {
                            // Max distance 2 for a typo, or proportional to length
                            let threshold = std::cmp::max(2, m.len() / 3);
                            if best_dist <= threshold && best_dist > 0 {
                                push_hint(&mut out, &format!("Did you mean the subcommand `{m}`?"));
                                did_you_mean = true;
                            }
                        }
                    }
                    break;
                }
            }
        }

        if !did_you_mean
            && let Some(bp) = extracted_bad_path.as_deref()
            && looks_like_bare_subcommand_token(bp)
        {
            push_hint(
                &mut out,
                "Run `tokmd --help` to see a list of available subcommands.",
            );
            return out;
        }

        if did_you_mean {
            return out;
        }

        let output_context = chain.iter().any(|message| {
            let message = message.to_ascii_lowercase();
            message.starts_with("failed to write")
                || message.starts_with("failed to create output")
                || message.starts_with("failed to create baseline file")
                || message.starts_with("failed to create bundle")
        });
        let input_context = extracted_bad_path.is_some()
            || chain.iter().any(|message| {
                let message = message.to_ascii_lowercase();
                message.starts_with("failed to access path ")
                    || message.starts_with("failed to read")
                    || message.starts_with("failed to load")
            });
        if output_context {
            push_hint(
                &mut out,
                "Create the parent directory for the output path named above, then retry.",
            );
        } else if input_context {
            push_hint(&mut out, "Verify the input path exists and is readable.");
            push_hint(
                &mut out,
                "Use an absolute path to avoid working-directory confusion.",
            );
        } else {
            push_hint(
                &mut out,
                "Check the path for the failed operation; for output files, ensure the parent directory exists.",
            );
        }
    }

    if !stable_local_failure && haystack.contains("base ref") && haystack.contains("not found") {
        push_hint(
            &mut out,
            "Fetch refs (`git fetch --tags --prune`) and retry with `--base <ref>`.",
        );
        push_hint(
            &mut out,
            "You can also set `TOKMD_GIT_BASE_REF` to a valid default base ref.",
        );
    }

    if haystack.contains("failed to load diff source") || haystack.contains("invalid reference") {
        push_hint(
            &mut out,
            "If you meant to compare files, ensure they both exist locally.",
        );
        push_hint(
            &mut out,
            "If you meant to compare git refs, ensure the branch, tag, or commit exists.",
        );
    }

    if haystack.contains("unknown metric/finding key") {
        push_hint(
            &mut out,
            "Run `tokmd analyze --explain list` to see supported keys.",
        );
    }

    // Match producer context, not arbitrary format words in resource paths.
    // Gate usage contexts preserve their causes, so inspect each producer
    // entry without searching across a pathname and its flattened causes.
    let json_receipt_parse = !unparsed_io
        && chain.iter().any(|message| {
            let message = message.to_ascii_lowercase();
            [
                "failed to parse json from ",
                "failed to parse baseline json",
                "failed to parse lang receipt",
                "failed to parse language receipt",
                "failed to parse run receipt",
                "failed to parse run language receipt",
                "failed to parse export rows",
            ]
            .iter()
            .any(|prefix| message.starts_with(prefix))
        });
    let toml_parse = !unparsed_io
        && !json_receipt_parse
        && chain.iter().any(|message| {
            let message = message.to_ascii_lowercase();
            message.starts_with("invalid toml:")
                || message.starts_with("toml parse error")
                || message.starts_with("failed to parse policy toml")
        });
    if toml_parse {
        push_hint(
            &mut out,
            "Check TOML syntax and key names in the file named above, then retry.",
        );
    }

    // JSON receipt / baseline failures retain receipt recovery even under a
    // directory named `toml`. Unrelated JSON evidence/config is not a receipt.
    if json_receipt_parse {
        push_hint(
            &mut out,
            "Ensure the file is a tokmd JSON receipt (produced by `tokmd run`, `tokmd export`, or `tokmd analyze`).",
        );
        push_hint(
            &mut out,
            "If it was hand-edited or truncated, regenerate the receipt and retry.",
        );
    }

    out
}

fn push_hint(out: &mut Vec<String>, hint: &str) {
    if !out.iter().any(|h| h == hint) {
        out.push(hint.to_string());
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    if a_chars.is_empty() {
        return b_chars.len();
    }
    if b_chars.is_empty() {
        return a_chars.len();
    }

    let mut d = vec![vec![0; b_chars.len() + 1]; a_chars.len() + 1];

    for (i, row) in d.iter_mut().enumerate().take(a_chars.len() + 1) {
        row[0] = i;
    }
    for (j, item) in d[0].iter_mut().enumerate().take(b_chars.len() + 1) {
        *item = j;
    }

    for i in 1..=a_chars.len() {
        for j in 1..=b_chars.len() {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            d[i][j] = std::cmp::min(
                std::cmp::min(d[i - 1][j] + 1, d[i][j - 1] + 1),
                d[i - 1][j - 1] + cost,
            );
        }
    }

    d[a_chars.len()][b_chars.len()]
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;

    #[test]
    fn json_receipt_parse_guidance_ignores_toml_in_paths() -> anyhow::Result<()> {
        for context in [
            "Failed to parse JSON from configs/toml/receipt.json",
            "Failed to parse baseline JSON from configs/toml/baseline.json",
            "Failed to parse lang receipt",
            "Failed to parse run receipt",
            "Failed to parse export rows",
        ] {
            let cause = serde_json::from_str::<serde_json::Value>("{broken")
                .err()
                .ok_or_else(|| anyhow!("malformed JSON unexpectedly parsed"))?;
            let err = anyhow::Error::new(cause).context(context);
            let hints = super::suggestions(&err);
            anyhow::ensure!(
                hints
                    == vec![
                        "Ensure the file is a tokmd JSON receipt (produced by `tokmd run`, `tokmd export`, or `tokmd analyze`).",
                        "If it was hand-edited or truncated, regenerate the receipt and retry.",
                    ],
                "wrong guidance for {context}: {hints:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn toml_policy_and_config_parse_guidance_stays_distinct() -> anyhow::Result<()> {
        for (context, usage_context) in [
            (
                "Failed to load policy from json/receipt-policy.json",
                "Invalid gate policy",
            ),
            (
                "Failed to load ratchet config from json/receipt-ratchet.json",
                "Invalid ratchet config",
            ),
        ] {
            let cause = tokmd_gate::PolicyConfig::from_toml("{broken")
                .err()
                .ok_or_else(|| anyhow!("malformed policy unexpectedly parsed"))?;
            let err = crate::commands::UsageError::context(
                anyhow::Error::new(cause).context(context),
                usage_context,
            );
            let hints = super::suggestions(&err);
            anyhow::ensure!(
                hints
                    == vec!["Check TOML syntax and key names in the file named above, then retry."],
                "wrong guidance for {context}: {hints:?}"
            );
            anyhow::ensure!(crate::exit_code(&err) == 2);
        }
        Ok(())
    }

    #[test]
    fn gate_usage_context_retains_localized_io_cause_and_recovery() -> anyhow::Result<()> {
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
        ] {
            for usage_context in ["Invalid gate policy", "Invalid ratchet config"] {
                let cause = tokmd_gate::GateError::IoError(std::io::Error::new(
                    kind,
                    "operation impossible",
                ));
                let err = crate::commands::UsageError::context(
                    anyhow::Error::new(cause)
                        .context("Failed to load policy from inputs: invalid TOML: config.json"),
                    usage_context,
                );
                anyhow::ensure!(crate::exit_code(&err) == 2);
                anyhow::ensure!(err.chain().any(|cause| {
                    cause
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == kind)
                }));
                let expected = if kind == std::io::ErrorKind::NotFound {
                    vec![
                        "Verify the input path exists and is readable.",
                        "Use an absolute path to avoid working-directory confusion.",
                    ]
                } else {
                    vec![]
                };
                anyhow::ensure!(super::suggestions(&err) == expected);
            }
        }
        Ok(())
    }

    #[test]
    fn toml_config_parser_cause_retains_literal_guidance() -> anyhow::Result<()> {
        let cause = toml::from_str::<toml::Value>("broken = [")
            .err()
            .ok_or_else(|| anyhow!("malformed TOML unexpectedly parsed"))?;
        let err = anyhow::Error::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))
            .context("Failed to load TOML config from json/receipt-config.json");
        let hints = super::suggestions(&err);
        anyhow::ensure!(
            hints == vec!["Check TOML syntax and key names in the file named above, then retry."],
            "wrong TOML parser guidance: {hints:?}"
        );
        Ok(())
    }

    #[test]
    fn format_words_in_io_paths_do_not_add_parse_guidance() -> anyhow::Result<()> {
        for (kind, context, expected) in [
            (
                std::io::ErrorKind::NotFound,
                "Failed to read baseline from invalid-toml/receipt.json",
                vec![
                    "Verify the input path exists and is readable.",
                    "Use an absolute path to avoid working-directory confusion.",
                ],
            ),
            (
                std::io::ErrorKind::NotFound,
                "Failed to write badge to invalid-toml/badge.svg",
                vec!["Create the parent directory for the output path named above, then retry."],
            ),
            (
                std::io::ErrorKind::PermissionDenied,
                "Failed to read baseline from invalid-toml/receipt.json",
                vec![],
            ),
            (
                std::io::ErrorKind::NotFound,
                "Failed to load TOML config from inputs: invalid TOML: config.json",
                vec![
                    "Verify the input path exists and is readable.",
                    "Use an absolute path to avoid working-directory confusion.",
                ],
            ),
            (
                std::io::ErrorKind::PermissionDenied,
                "Failed to load TOML config from inputs: invalid TOML: config.json",
                vec![],
            ),
        ] {
            let err = anyhow::Error::new(std::io::Error::new(kind, "operation impossible"))
                .context(context);
            let hints = super::suggestions(&err);
            anyhow::ensure!(hints == expected, "wrong guidance for {context}: {hints:?}");
        }
        Ok(())
    }

    #[test]
    fn contextual_not_found_gets_file_recovery_in_any_locale() -> anyhow::Result<()> {
        let err = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "fichier introuvable",
        ))
        .context("Failed to read baseline from missing-baseline");
        let rendered = super::format(&err);
        anyhow::ensure!(rendered.contains("Failed to read baseline from missing-baseline"));
        anyhow::ensure!(rendered.contains("Verify the input path exists and is readable."));
        anyhow::ensure!(rendered.contains("Use an absolute path"));
        anyhow::ensure!(!rendered.contains("Run `tokmd --help`"));
        anyhow::ensure!(!rendered.contains("Unrecognized subcommand"));
        Ok(())
    }

    #[test]
    fn contextual_unix_not_found_does_not_suggest_subcommand_help() -> anyhow::Result<()> {
        let err = anyhow!("Failed to read baseline from baseline.json: No such file or directory");
        let hints = super::suggestions(&err);
        anyhow::ensure!(hints.iter().any(|h| h.contains("input path exists")));
        anyhow::ensure!(!hints.iter().any(|h| h.contains("Run `tokmd --help`")));
        Ok(())
    }

    #[test]
    fn alternate_missing_path_prefix_preserves_path_and_typo_guidance() -> anyhow::Result<()> {
        let path_error = anyhow!("Input path does not exist: receipts/current.json");
        let path_hints = super::suggestions(&path_error);
        anyhow::ensure!(path_hints.iter().any(|h| h.contains("input path exists")));
        anyhow::ensure!(!path_hints.iter().any(|h| h.contains("Run `tokmd --help`")));

        let typo_error = anyhow!("Input path does not exist: anolyze");
        let typo_hints = super::suggestions(&typo_error);
        anyhow::ensure!(
            typo_hints
                .iter()
                .any(|h| h.contains("Did you mean the subcommand `analyze`?"))
        );
        anyhow::ensure!(!typo_hints.iter().any(|h| h.contains("input path exists")));
        Ok(())
    }

    #[test]
    fn contextual_not_found_preserves_diff_recovery() -> anyhow::Result<()> {
        let err = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "fichier introuvable",
        ))
        .context("Failed to load diff source 'receipts/before.json'");
        let hints = super::suggestions(&err);
        anyhow::ensure!(hints.iter().any(|h| h.contains("input path exists")));
        anyhow::ensure!(
            hints
                .iter()
                .any(|h| h.contains("ensure they both exist locally"))
        );
        anyhow::ensure!(
            hints
                .iter()
                .any(|h| h.contains("ensure the branch, tag, or commit exists"))
        );
        anyhow::ensure!(!hints.iter().any(|h| h.contains("Run `tokmd --help`")));
        Ok(())
    }

    #[test]
    fn permission_denied_does_not_get_missing_file_guidance() -> anyhow::Result<()> {
        let err = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "access denied",
        ))
        .context("Failed to read baseline from baseline.json");
        let hints = super::suggestions(&err);
        anyhow::ensure!(!hints.iter().any(|h| h.contains("input path exists")));
        anyhow::ensure!(!hints.iter().any(|h| h.contains("Run `tokmd --help`")));
        Ok(())
    }

    #[test]
    fn output_not_found_gets_parent_directory_recovery() -> anyhow::Result<()> {
        let err = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "fichier introuvable",
        ))
        .context("Failed to write badge to missing-output/badge.svg");
        let rendered = super::format(&err);
        anyhow::ensure!(rendered.contains("missing-output/badge.svg"));
        anyhow::ensure!(rendered.contains("Create the parent directory for the output path"));
        anyhow::ensure!(!rendered.contains("input path exists"));
        anyhow::ensure!(!rendered.contains("Use an absolute path"));
        anyhow::ensure!(!rendered.contains("Run `tokmd --help`"));
        Ok(())
    }

    #[test]
    fn unclassified_not_found_does_not_assume_an_input_read() -> anyhow::Result<()> {
        let err = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "fichier introuvable",
        ));
        let hints = super::suggestions(&err);
        anyhow::ensure!(hints.iter().any(|h| h.contains("for output files")));
        anyhow::ensure!(!hints.iter().any(|h| h.contains("input path exists")));
        anyhow::ensure!(!hints.iter().any(|h| h.contains("Run `tokmd --help`")));
        Ok(())
    }

    #[test]
    fn malformed_json_does_not_get_missing_file_guidance() -> anyhow::Result<()> {
        let err = anyhow!("Failed to parse baseline JSON from baseline.json: expected value");
        let hints = super::suggestions(&err);
        anyhow::ensure!(hints.iter().any(|h| h.contains("tokmd JSON receipt")));
        anyhow::ensure!(!hints.iter().any(|h| h.contains("input path exists")));
        anyhow::ensure!(!hints.iter().any(|h| h.contains("Run `tokmd --help`")));
        Ok(())
    }

    use super::{format, suggestions};

    #[test]
    fn suggests_for_missing_git() {
        let err = anyhow!("git is not available on PATH");
        let hints = suggestions(&err);
        assert!(hints.iter().any(|h| h.contains("git --version")));
        assert!(hints.iter().any(|h| h.contains("--no-git")));
    }

    #[test]
    fn suggests_for_typo_subcommand() {
        let err = anyhow!("Path not found: anolyze");
        let hints = suggestions(&err);
        assert!(
            hints
                .iter()
                .any(|h| h.contains("Did you mean the subcommand `analyze`?"))
        );
        assert!(!hints.iter().any(|h| h.contains("Run `tokmd --help`")));
        assert!(!hints.iter().any(|h| h.contains("input path exists")));
        assert!(
            !hints
                .iter()
                .any(|h| h.contains("subcommand, it is not recognized"))
        );
    }

    #[test]
    fn format_rewrites_bare_missing_path_as_unrecognized_subcommand() {
        let err = anyhow!("Path not found: frobnicate");
        let rendered = format(&err);
        assert!(rendered.contains("Error: Unrecognized subcommand 'frobnicate'"));
        assert!(!rendered.contains("Error: Path not found: frobnicate"));
        assert!(!rendered.contains("was intended as a subcommand"));
        assert!(rendered.contains("Run `tokmd --help` to see a list of available subcommands."));
        assert!(!rendered.contains("Verify the input path exists and is readable."));
    }

    #[test]
    fn format_preserves_path_shaped_missing_path_errors() {
        let err = anyhow!("Path not found: missing/file.rs");
        let rendered = format(&err);
        assert!(rendered.contains("Error: Path not found: missing/file.rs"));
        assert!(!rendered.contains("Unrecognized subcommand"));
    }

    #[test]
    fn suggests_for_missing_path() {
        let err = anyhow!("Path not found: missing/file.rs");
        let hints = suggestions(&err);
        assert!(hints.iter().any(|h| h.contains("input path exists")));
        assert!(hints.iter().any(|h| h.contains("absolute path")));
        assert!(!hints.iter().any(|h| h.contains("Run `tokmd --help`")));
    }

    #[test]
    fn suggests_help_for_unrecognized_bare_subcommand() {
        let err = anyhow!("Path not found: frobnicate");
        let hints = suggestions(&err);
        assert!(
            hints
                .iter()
                .any(|h| h.contains("Run `tokmd --help` to see a list of available subcommands."))
        );
        assert!(!hints.iter().any(|h| h.contains("input path exists")));
    }

    #[test]
    fn suggests_for_parent_traversal() {
        let err = anyhow!("Bounded path must not contain parent traversal: ../secret.txt");
        let hints = suggestions(&err);
        assert!(
            hints
                .iter()
                .any(|h| h.contains("inside the selected scan root"))
        );
        assert!(hints.iter().any(|h| h.contains("root-relative paths")));
    }

    #[test]
    fn suggests_for_root_escape() {
        let err = anyhow!("Bounded path escapes scan root C:/repo: C:/secret.txt");
        let rendered = format(&err);
        assert!(rendered.contains("Error:"));
        assert!(rendered.contains("Hints:"));
        assert!(rendered.contains("inside the selected scan root"));
        assert!(rendered.contains("resolve outside the scan root"));
    }

    #[test]
    fn resolve_failures_do_not_get_bounded_path_hints() {
        let err = anyhow!("Failed to resolve scan root C:/repo: permission denied");
        let hints = suggestions(&err);
        assert!(
            !hints
                .iter()
                .any(|h| h.contains("parent traversal") || h.contains("root-relative"))
        );
    }

    #[test]
    fn suggests_for_unknown_explain_key() {
        let err = anyhow!("Unknown metric/finding key 'foo'.");
        let hints = suggestions(&err);
        assert!(hints.iter().any(|h| h.contains("--explain list")));
    }

    #[test]
    fn suggests_for_missing_diff_source() {
        let err = anyhow!(
            "Failed to load diff source 'missing_file.json': Failed to create worktree for 'missing_file.json': git worktree add failed for 'missing_file.json'"
        );
        let hints = suggestions(&err);
        assert!(
            hints
                .iter()
                .any(|h| h.contains("ensure they both exist locally"))
        );
        assert!(
            hints
                .iter()
                .any(|h| h.contains("ensure the branch, tag, or commit exists"))
        );
    }

    #[test]
    fn format_includes_hints_section() {
        let err = anyhow!("Path not found: no-file");
        let rendered = format(&err);
        assert!(rendered.contains("Error:"));
        assert!(rendered.contains("Hints:"));
    }

    #[test]
    fn suggests_for_rate_limit_errors() {
        let err = anyhow!("GitHub returned HTTP 429 Too Many Requests");
        let hints = suggestions(&err);
        assert!(hints.iter().any(|h| h.contains("limiting requests")));
        assert!(hints.iter().any(|h| h.contains("Retry-After")));
        assert!(hints.iter().any(|h| h.contains("smaller input scope")));
    }

    #[test]
    fn suggests_for_json_receipt_parse_failure() {
        let err = anyhow!("Failed to parse JSON from receipt.json: expected value at line 1");
        let hints = suggestions(&err);
        assert!(
            hints.iter().any(|h| h.contains("tokmd JSON receipt")),
            "expected receipt-source guidance, got: {hints:?}"
        );
        assert!(
            hints.iter().any(|h| h.contains("regenerate the receipt")),
            "expected regenerate guidance, got: {hints:?}"
        );
    }

    #[test]
    fn suggests_for_lang_receipt_parse_failure() {
        let err = anyhow!("Failed to parse lang receipt");
        let hints = suggestions(&err);
        assert!(hints.iter().any(|h| h.contains("tokmd JSON receipt")));
    }

    #[test]
    fn suggests_for_baseline_json_parse_failure() {
        let err = anyhow!("Failed to parse baseline JSON from baseline.json");
        let hints = suggestions(&err);
        assert!(hints.iter().any(|h| h.contains("tokmd JSON receipt")));
    }

    #[test]
    fn toml_parse_failure_does_not_get_receipt_hint() {
        let err = anyhow!("invalid TOML: failed to parse key at line 3");
        let hints = suggestions(&err);
        assert!(hints.iter().any(|h| h.contains("TOML syntax")));
        assert!(!hints.iter().any(|h| h.contains("init --force")));
        assert!(
            !hints.iter().any(|h| h.contains("tokmd JSON receipt")),
            "TOML parse errors must not get the JSON receipt hint, got: {hints:?}"
        );
    }

    #[test]
    fn suggests_for_transient_network_errors() {
        let err = anyhow!("request timed out while contacting remote service");
        let hints = suggestions(&err);
        assert!(hints.iter().any(|h| h.contains("looks transient")));
        assert!(hints.iter().any(|h| h.contains("VPN, or proxy")));
    }
}
