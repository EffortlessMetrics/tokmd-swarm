use super::capability::{AstLanguage, SYNTAX_RECEIPT_SCHEMA_VERSION};
use serde_json::{Value, json};
use tree_sitter::{Language, Parser};

pub const DEFAULT_MAX_SYNTAX_BYTES: usize = 1_048_576;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyntaxParserCapability {
    pub language: AstLanguage,
    pub parser_crate: &'static str,
    pub grammar_symbol: &'static str,
    pub extensions: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxParseStatus {
    Complete,
    ParseDegraded,
    ParserFailed,
    SkippedGeneratedOrVendor,
    SkippedTooLarge,
    UnsupportedLanguage,
}

impl SyntaxParseStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::ParseDegraded => "parse_degraded",
            Self::ParserFailed => "parser_failed",
            Self::SkippedGeneratedOrVendor => "skipped_generated_or_vendor",
            Self::SkippedTooLarge => "skipped_too_large",
            Self::UnsupportedLanguage => "unsupported_language",
        }
    }

    #[must_use]
    pub const fn is_advisory(self) -> bool {
        !matches!(self, Self::Complete)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyntaxParseOptions {
    pub max_bytes: usize,
    pub skip_generated_vendor: bool,
}

impl Default for SyntaxParseOptions {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_SYNTAX_BYTES,
            skip_generated_vendor: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxParseReceipt {
    pub path: String,
    pub language: Option<AstLanguage>,
    pub parser_crate: Option<&'static str>,
    pub grammar_symbol: Option<&'static str>,
    pub status: SyntaxParseStatus,
    pub reason: Option<String>,
    pub source_bytes: usize,
    pub root_kind: Option<String>,
    pub has_error: bool,
}

impl SyntaxParseReceipt {
    #[must_use]
    pub fn to_value(&self) -> Value {
        json!({
            "schema": SYNTAX_RECEIPT_SCHEMA_VERSION,
            "path": self.path.as_str(),
            "language": self.language.map(AstLanguage::as_str),
            "parser_crate": self.parser_crate,
            "grammar_symbol": self.grammar_symbol,
            "status": self.status.as_str(),
            "advisory": self.status.is_advisory(),
            "reason": self.reason.as_deref(),
            "source_bytes": self.source_bytes,
            "root_kind": self.root_kind.as_deref(),
            "has_error": self.has_error,
        })
    }
}

pub const RUST_SYNTAX_CAPABILITY: SyntaxParserCapability = SyntaxParserCapability {
    language: AstLanguage::Rust,
    parser_crate: "tree-sitter-rust",
    grammar_symbol: "tree_sitter_rust",
    extensions: &["rs"],
};

pub const TYPESCRIPT_SYNTAX_CAPABILITY: SyntaxParserCapability = SyntaxParserCapability {
    language: AstLanguage::TypeScript,
    parser_crate: "tree-sitter-typescript",
    grammar_symbol: "tree_sitter_typescript",
    extensions: &["ts", "mts", "cts"],
};

pub const TSX_SYNTAX_CAPABILITY: SyntaxParserCapability = SyntaxParserCapability {
    language: AstLanguage::Tsx,
    parser_crate: "tree-sitter-typescript",
    grammar_symbol: "tree_sitter_tsx",
    extensions: &["tsx"],
};

pub const PYTHON_SYNTAX_CAPABILITY: SyntaxParserCapability = SyntaxParserCapability {
    language: AstLanguage::Python,
    parser_crate: "tree-sitter-python",
    grammar_symbol: "tree_sitter_python",
    extensions: &["py", "pyw"],
};

static SYNTAX_CAPABILITIES: &[SyntaxParserCapability] = &[
    RUST_SYNTAX_CAPABILITY,
    TYPESCRIPT_SYNTAX_CAPABILITY,
    TSX_SYNTAX_CAPABILITY,
    PYTHON_SYNTAX_CAPABILITY,
];

#[must_use]
pub fn syntax_capabilities() -> &'static [SyntaxParserCapability] {
    SYNTAX_CAPABILITIES
}

#[must_use]
pub fn syntax_capability_for_path(path: &str) -> Option<&'static SyntaxParserCapability> {
    let extension = syntax_extension(path)?;
    SYNTAX_CAPABILITIES
        .iter()
        .find(|capability| capability.extensions.contains(&extension.as_str()))
}

#[must_use]
pub fn parse_syntax_receipt(
    path: &str,
    source: &str,
    options: SyntaxParseOptions,
) -> SyntaxParseReceipt {
    let normalized_path = normalize_syntax_path(path);
    let source_bytes = source.len();

    if options.skip_generated_vendor {
        if let Some(reason) = generated_or_vendor_reason(&normalized_path) {
            return advisory_receipt(
                normalized_path,
                None,
                SyntaxParseStatus::SkippedGeneratedOrVendor,
                reason,
                source_bytes,
            );
        }
    }

    let capability = match syntax_capability_for_path(&normalized_path) {
        Some(capability) => capability,
        None => {
            return advisory_receipt(
                normalized_path,
                None,
                SyntaxParseStatus::UnsupportedLanguage,
                "no locked Tree-sitter parser for file extension",
                source_bytes,
            );
        }
    };

    if source_bytes > options.max_bytes {
        return advisory_receipt(
            normalized_path,
            Some(capability),
            SyntaxParseStatus::SkippedTooLarge,
            format!(
                "file has {source_bytes} bytes, above syntax parser limit {}",
                options.max_bytes
            ),
            source_bytes,
        );
    }

    let mut parser = Parser::new();
    let language: Language = match capability.language {
        AstLanguage::Python => tree_sitter_python::LANGUAGE.into(),
        AstLanguage::Rust => tree_sitter_rust::LANGUAGE.into(),
        AstLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        AstLanguage::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
    };

    if let Err(error) = parser.set_language(&language) {
        return advisory_receipt(
            normalized_path,
            Some(capability),
            SyntaxParseStatus::ParserFailed,
            format!("failed to load Tree-sitter parser: {error}"),
            source_bytes,
        );
    }

    let Some(tree) = parser.parse(source, None) else {
        return advisory_receipt(
            normalized_path,
            Some(capability),
            SyntaxParseStatus::ParserFailed,
            "Tree-sitter parser returned no tree",
            source_bytes,
        );
    };

    let root = tree.root_node();
    let has_error = root.has_error();
    SyntaxParseReceipt {
        path: normalized_path,
        language: Some(capability.language),
        parser_crate: Some(capability.parser_crate),
        grammar_symbol: Some(capability.grammar_symbol),
        status: if has_error {
            SyntaxParseStatus::ParseDegraded
        } else {
            SyntaxParseStatus::Complete
        },
        reason: has_error.then(|| "Tree-sitter parsed with syntax errors".to_owned()),
        source_bytes,
        root_kind: Some(root.kind().to_owned()),
        has_error,
    }
}

fn advisory_receipt(
    path: String,
    capability: Option<&SyntaxParserCapability>,
    status: SyntaxParseStatus,
    reason: impl Into<String>,
    source_bytes: usize,
) -> SyntaxParseReceipt {
    SyntaxParseReceipt {
        path,
        language: capability.map(|capability| capability.language),
        parser_crate: capability.map(|capability| capability.parser_crate),
        grammar_symbol: capability.map(|capability| capability.grammar_symbol),
        status,
        reason: Some(reason.into()),
        source_bytes,
        root_kind: None,
        has_error: false,
    }
}

#[must_use]
pub fn normalize_syntax_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_owned()
}

fn syntax_extension(path: &str) -> Option<String> {
    normalize_syntax_path(path)
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
}

fn generated_or_vendor_reason(path: &str) -> Option<&'static str> {
    let normalized = normalize_syntax_path(path).to_ascii_lowercase();
    let segments = normalized.split('/').collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| matches!(*segment, "vendor" | "node_modules"))
    {
        return Some("generated/vendor policy excluded this file");
    }
    if normalized.contains("/generated/")
        || normalized.contains(".generated.")
        || normalized.contains(".gen.")
    {
        return Some("generated/vendor policy excluded this file");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_MAX_SYNTAX_BYTES, SyntaxParseOptions, SyntaxParseStatus, parse_syntax_receipt,
        syntax_capabilities, syntax_capability_for_path,
    };

    #[test]
    fn registry_locks_supported_parsers_and_extensions() {
        let capabilities = syntax_capabilities();

        assert_eq!(capabilities.len(), 4);
        assert_eq!(
            capabilities
                .iter()
                .map(|capability| (
                    capability.language.as_str(),
                    capability.parser_crate,
                    capability.grammar_symbol,
                    capability.extensions
                ))
                .collect::<Vec<_>>(),
            vec![
                ("rust", "tree-sitter-rust", "tree_sitter_rust", &["rs"][..]),
                (
                    "typescript",
                    "tree-sitter-typescript",
                    "tree_sitter_typescript",
                    &["ts", "mts", "cts"][..]
                ),
                (
                    "tsx",
                    "tree-sitter-typescript",
                    "tree_sitter_tsx",
                    &["tsx"][..]
                ),
                (
                    "python",
                    "tree-sitter-python",
                    "tree_sitter_python",
                    &["py", "pyw"][..]
                ),
            ]
        );
    }

    #[test]
    fn routes_paths_to_locked_capabilities() {
        assert_eq!(
            syntax_capability_for_path("src/main.rs")
                .unwrap()
                .language
                .as_str(),
            "rust"
        );
        assert_eq!(
            syntax_capability_for_path("src/app.TS")
                .unwrap()
                .language
                .as_str(),
            "typescript"
        );
        assert_eq!(
            syntax_capability_for_path("src/app.tsx")
                .unwrap()
                .language
                .as_str(),
            "tsx"
        );
        assert_eq!(
            syntax_capability_for_path("tools/script.py")
                .unwrap()
                .language
                .as_str(),
            "python"
        );
        assert!(syntax_capability_for_path("README.md").is_none());
    }

    #[test]
    fn parses_supported_languages_without_network_or_global_state() {
        let cases = [
            ("src/lib.rs", "fn main() {}\n", "rust", "source_file"),
            (
                "src/app.ts",
                "export function run(value: number) { return value + 1; }\n",
                "typescript",
                "program",
            ),
            (
                "src/App.tsx",
                "export function App() { return <main>Hello</main>; }\n",
                "tsx",
                "program",
            ),
            (
                "tools/run.py",
                "def run(value):\n    return value + 1\n",
                "python",
                "module",
            ),
        ];

        for (path, source, language, root_kind) in cases {
            let receipt = parse_syntax_receipt(path, source, SyntaxParseOptions::default());

            assert_eq!(receipt.status, SyntaxParseStatus::Complete, "{path}");
            assert_eq!(receipt.language.unwrap().as_str(), language);
            assert_eq!(receipt.root_kind.as_deref(), Some(root_kind));
            assert!(!receipt.has_error);
            assert_eq!(receipt.to_value()["status"], "complete");
            assert_eq!(receipt.to_value()["advisory"], false);
        }
    }

    #[test]
    fn malformed_syntax_degrades_explicitly() {
        let receipt = parse_syntax_receipt(
            "src/lib.rs",
            "fn ok() {}\nfn broken(",
            SyntaxParseOptions::default(),
        );

        assert_eq!(receipt.status, SyntaxParseStatus::ParseDegraded);
        assert!(receipt.has_error);
        assert_eq!(receipt.to_value()["status"], "parse_degraded");
        assert_eq!(receipt.to_value()["advisory"], true);
        assert!(receipt.reason.unwrap().contains("syntax errors"));
    }

    #[test]
    fn unsupported_language_is_an_advisory_receipt() {
        let receipt = parse_syntax_receipt("README.md", "# docs\n", SyntaxParseOptions::default());

        assert_eq!(receipt.status, SyntaxParseStatus::UnsupportedLanguage);
        assert!(receipt.language.is_none());
        assert_eq!(receipt.to_value()["schema"], "tokmd.syntax_receipt.v1");
        assert_eq!(receipt.to_value()["status"], "unsupported_language");
        assert_eq!(receipt.to_value()["advisory"], true);
    }

    #[test]
    fn generated_or_vendor_files_are_skipped_by_policy() {
        for path in [
            "vendor/crate/src/lib.rs",
            "node_modules/pkg/index.ts",
            "src/generated/model.py",
            "src/api.generated.ts",
        ] {
            let receipt =
                parse_syntax_receipt(path, "fn ignored() {}", SyntaxParseOptions::default());

            assert_eq!(
                receipt.status,
                SyntaxParseStatus::SkippedGeneratedOrVendor,
                "{path}"
            );
            assert_eq!(receipt.to_value()["advisory"], true);
            assert!(receipt.reason.unwrap().contains("policy"));
        }
    }

    #[test]
    fn huge_files_are_skipped_with_limit_evidence() {
        let receipt = parse_syntax_receipt(
            "src/lib.rs",
            "fn main() {}\n",
            SyntaxParseOptions {
                max_bytes: 4,
                skip_generated_vendor: false,
            },
        );

        assert_eq!(receipt.status, SyntaxParseStatus::SkippedTooLarge);
        assert_eq!(receipt.language.unwrap().as_str(), "rust");
        assert!(
            receipt
                .reason
                .unwrap()
                .contains("above syntax parser limit 4")
        );
        assert!(DEFAULT_MAX_SYNTAX_BYTES > 4);
    }
}
