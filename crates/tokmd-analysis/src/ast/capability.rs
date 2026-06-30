use super::python;
use super::rust;
use super::typescript;

pub const AST_SHADOW_SCHEMA_VERSION: &str = "tokmd.ast_shadow.v1";
pub const SYNTAX_RECEIPT_SCHEMA_VERSION: &str = "tokmd.syntax_receipt.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AstLanguage {
    Python,
    Rust,
    TypeScript,
    Tsx,
}

impl AstLanguage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AstParserStatus {
    ParserBackedShadow,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AstCapability {
    pub language: AstLanguage,
    pub parser_crate: &'static str,
    pub parser_status: AstParserStatus,
    pub shadow_only: bool,
    pub changes_default_receipts: bool,
}

impl AstCapability {
    pub const fn parser_backed_shadow(language: AstLanguage, parser_crate: &'static str) -> Self {
        Self {
            language,
            parser_crate,
            parser_status: AstParserStatus::ParserBackedShadow,
            shadow_only: true,
            changes_default_receipts: false,
        }
    }
}

#[must_use]
pub fn capabilities() -> &'static [AstCapability] {
    static CAPABILITIES: &[AstCapability] = &[
        rust::RUST_CAPABILITY,
        typescript::TYPESCRIPT_CAPABILITY,
        typescript::TSX_CAPABILITY,
        python::PYTHON_CAPABILITY,
    ];
    CAPABILITIES
}

#[cfg(test)]
mod tests {
    use super::{
        AST_SHADOW_SCHEMA_VERSION, AstCapability, AstLanguage, AstParserStatus,
        SYNTAX_RECEIPT_SCHEMA_VERSION,
    };

    #[test]
    fn parser_backed_shadow_constructor_never_changes_default_receipts() {
        let capability = AstCapability::parser_backed_shadow(AstLanguage::Rust, "tree-sitter-rust");

        assert_eq!(capability.language.as_str(), "rust");
        assert_eq!(
            capability.parser_status,
            AstParserStatus::ParserBackedShadow
        );
        assert!(capability.shadow_only);
        assert!(!capability.changes_default_receipts);
    }

    #[test]
    fn syntax_languages_have_stable_wire_values() {
        assert_eq!(AstLanguage::Python.as_str(), "python");
        assert_eq!(AstLanguage::Rust.as_str(), "rust");
        assert_eq!(AstLanguage::TypeScript.as_str(), "typescript");
        assert_eq!(AstLanguage::Tsx.as_str(), "tsx");
    }

    #[test]
    fn schema_names_are_stable() {
        assert_eq!(AST_SHADOW_SCHEMA_VERSION, "tokmd.ast_shadow.v1");
        assert_eq!(SYNTAX_RECEIPT_SCHEMA_VERSION, "tokmd.syntax_receipt.v1");
    }
}
