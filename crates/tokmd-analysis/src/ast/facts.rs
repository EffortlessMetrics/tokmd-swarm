use serde_json::{Value, json};
use tree_sitter::Node;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SyntaxSpan {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl SyntaxSpan {
    #[must_use]
    pub fn from_node(node: Node<'_>) -> Self {
        let start = node.start_position();
        let end = node.end_position();
        Self {
            start_line: start.row + 1,
            start_column: start.column + 1,
            end_line: end.row + 1,
            end_column: end.column + 1,
        }
    }

    #[must_use]
    pub fn to_value(self) -> Value {
        json!({
            "start_line": self.start_line,
            "start_column": self.start_column,
            "end_line": self.end_line,
            "end_column": self.end_column,
        })
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SyntaxSymbol {
    pub kind: String,
    pub name: String,
    pub span: SyntaxSpan,
    pub exported: bool,
    pub public_surface: bool,
}

impl SyntaxSymbol {
    #[must_use]
    pub fn to_value(&self) -> Value {
        json!({
            "kind": self.kind,
            "name": self.name,
            "span": self.span.to_value(),
            "exported": self.exported,
            "public_surface": self.public_surface,
        })
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SyntaxImport {
    pub kind: String,
    pub module: Option<String>,
    pub imported: Vec<String>,
    pub dynamic: bool,
    pub span: SyntaxSpan,
}

impl SyntaxImport {
    #[must_use]
    pub fn to_value(&self) -> Value {
        json!({
            "kind": self.kind,
            "module": self.module,
            "imported": self.imported,
            "dynamic": self.dynamic,
            "span": self.span.to_value(),
        })
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SyntaxExport {
    pub kind: String,
    pub name: String,
    pub span: SyntaxSpan,
}

impl SyntaxExport {
    #[must_use]
    pub fn to_value(&self) -> Value {
        json!({
            "kind": self.kind,
            "name": self.name,
            "span": self.span.to_value(),
        })
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SyntaxCallSite {
    pub kind: String,
    pub callee: String,
    pub dynamic: bool,
    pub span: SyntaxSpan,
}

impl SyntaxCallSite {
    #[must_use]
    pub fn to_value(&self) -> Value {
        json!({
            "kind": self.kind,
            "callee": self.callee,
            "dynamic": self.dynamic,
            "span": self.span.to_value(),
        })
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SyntaxRiskSeam {
    pub kind: String,
    pub evidence: String,
    pub span: SyntaxSpan,
}

impl SyntaxRiskSeam {
    #[must_use]
    pub fn to_value(&self) -> Value {
        json!({
            "kind": self.kind,
            "evidence": self.evidence,
            "span": self.span.to_value(),
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SyntaxFacts {
    pub symbols: Vec<SyntaxSymbol>,
    pub imports: Vec<SyntaxImport>,
    pub exports: Vec<SyntaxExport>,
    pub call_sites: Vec<SyntaxCallSite>,
    pub risk_seams: Vec<SyntaxRiskSeam>,
}

impl SyntaxFacts {
    pub fn normalize(&mut self) {
        self.symbols.sort();
        self.symbols.dedup();
        self.imports.sort();
        self.imports.dedup();
        self.exports.sort();
        self.exports.dedup();
        self.call_sites.sort();
        self.call_sites.dedup();
        self.risk_seams.sort();
        self.risk_seams.dedup();
    }

    #[must_use]
    pub fn symbols_value(&self) -> Vec<Value> {
        self.symbols.iter().map(SyntaxSymbol::to_value).collect()
    }

    #[must_use]
    pub fn imports_value(&self) -> Vec<Value> {
        self.imports.iter().map(SyntaxImport::to_value).collect()
    }

    #[must_use]
    pub fn exports_value(&self) -> Vec<Value> {
        self.exports.iter().map(SyntaxExport::to_value).collect()
    }

    #[must_use]
    pub fn call_sites_value(&self) -> Vec<Value> {
        self.call_sites
            .iter()
            .map(SyntaxCallSite::to_value)
            .collect()
    }

    #[must_use]
    pub fn risk_seams_value(&self) -> Vec<Value> {
        self.risk_seams
            .iter()
            .map(SyntaxRiskSeam::to_value)
            .collect()
    }
}
