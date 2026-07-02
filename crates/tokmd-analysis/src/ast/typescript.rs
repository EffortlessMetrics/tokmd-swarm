use super::capability::{AstCapability, AstLanguage};
use super::facts::{
    SyntaxCallSite, SyntaxExport, SyntaxFacts, SyntaxImport, SyntaxRiskSeam, SyntaxSpan,
    SyntaxSymbol,
};
use std::error::Error;
use std::fmt;
use tree_sitter::{Node, Parser};

pub const TREE_SITTER_TYPESCRIPT_CRATE: &str = "tree-sitter-typescript";
pub const TYPESCRIPT_CAPABILITY: AstCapability =
    AstCapability::parser_backed_shadow(AstLanguage::TypeScript, TREE_SITTER_TYPESCRIPT_CRATE);
pub const TSX_CAPABILITY: AstCapability =
    AstCapability::parser_backed_shadow(AstLanguage::Tsx, TREE_SITTER_TYPESCRIPT_CRATE);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeScriptAstShadow {
    pub has_error: bool,
    pub landmarks: Vec<TypeScriptLandmark>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeScriptLandmark {
    pub kind: TypeScriptLandmarkKind,
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TypeScriptLandmarkKind {
    ControlFlow,
    Function,
    Import,
}

#[derive(Debug)]
pub enum TypeScriptAstError {
    Language(tree_sitter::LanguageError),
    ParseFailed,
    UnsupportedLanguage,
}

impl fmt::Display for TypeScriptAstError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Language(error) => {
                write!(f, "failed to load TypeScript Tree-sitter language: {error}")
            }
            Self::ParseFailed => f.write_str("failed to parse TypeScript source"),
            Self::UnsupportedLanguage => f.write_str("unsupported TypeScript shadow language"),
        }
    }
}

impl Error for TypeScriptAstError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Language(error) => Some(error),
            Self::ParseFailed | Self::UnsupportedLanguage => None,
        }
    }
}

pub fn parse_typescript_landmarks(
    source: &str,
    language: AstLanguage,
) -> Result<TypeScriptAstShadow, TypeScriptAstError> {
    let tree_sitter_language = match language {
        AstLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        AstLanguage::Tsx => tree_sitter_typescript::LANGUAGE_TSX,
        _ => return Err(TypeScriptAstError::UnsupportedLanguage),
    };

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_language.into())
        .map_err(TypeScriptAstError::Language)?;
    let tree = parser
        .parse(source, None)
        .ok_or(TypeScriptAstError::ParseFailed)?;

    let mut landmarks = Vec::new();
    collect_typescript_landmarks(tree.root_node(), source, &mut landmarks);
    landmarks.sort_by(|left, right| {
        left.start_line
            .cmp(&right.start_line)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.name.cmp(&right.name))
    });

    Ok(TypeScriptAstShadow {
        has_error: tree.root_node().has_error(),
        landmarks,
    })
}

fn collect_typescript_landmarks(
    node: Node<'_>,
    source: &str,
    landmarks: &mut Vec<TypeScriptLandmark>,
) {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" | "method_definition" => {
            if let Some(name) = typescript_function_name(node, source) {
                push_typescript_landmark(node, TypeScriptLandmarkKind::Function, name, landmarks);
            }
        }
        "import_statement" => {
            if let Some(name) = import_statement_name(node, source) {
                push_typescript_landmark(node, TypeScriptLandmarkKind::Import, name, landmarks);
            }
        }
        kind => {
            if let Some(name) = typescript_control_flow_name(kind) {
                push_typescript_landmark(
                    node,
                    TypeScriptLandmarkKind::ControlFlow,
                    name.to_owned(),
                    landmarks,
                );
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_typescript_landmarks(child, source, landmarks);
    }
}

fn typescript_function_name(node: Node<'_>, source: &str) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|name| text_if_named(source, name))
        .or_else(|| first_identifier_text(source, node))
}

fn import_statement_name(node: Node<'_>, source: &str) -> Option<String> {
    Some(compact_text(node_text(source, node)))
}

fn typescript_control_flow_name(kind: &str) -> Option<&'static str> {
    match kind {
        "if_statement" => Some("if"),
        "for_statement" | "for_in_statement" => Some("for"),
        "while_statement" => Some("while"),
        "switch_statement" => Some("switch"),
        _ => None,
    }
}

fn push_typescript_landmark(
    node: Node<'_>,
    kind: TypeScriptLandmarkKind,
    name: String,
    landmarks: &mut Vec<TypeScriptLandmark>,
) {
    let start = node.start_position();
    let end = node.end_position();
    landmarks.push(TypeScriptLandmark {
        kind,
        name,
        start_line: start.row + 1,
        end_line: end.row + 1,
    });
}

#[must_use]
pub fn extract_typescript_facts(root: Node<'_>, source: &str) -> SyntaxFacts {
    let mut facts = SyntaxFacts::default();
    visit_node(root, source, false, &mut facts);
    facts.normalize();
    facts
}

fn visit_node(node: Node<'_>, source: &str, exported: bool, facts: &mut SyntaxFacts) {
    let kind = node.kind();
    let exported = exported || kind == "export_statement";

    match kind {
        "import_statement" => push_static_import(node, source, facts),
        "function_declaration" | "generator_function_declaration" => {
            push_named_symbol(node, source, "function", exported, facts);
        }
        "class_declaration" => {
            push_named_symbol(node, source, "class", exported, facts);
        }
        "method_definition" | "public_field_definition" => {
            push_named_symbol(node, source, "member", exported, facts);
        }
        "variable_declarator" => {
            push_named_symbol(node, source, "variable", exported, facts);
        }
        "call_expression" => push_call_expression(node, source, facts),
        "new_expression" => push_new_expression(node, source, facts),
        "as_expression" | "type_assertion" => {
            push_risk("risky_cast", node_text(source, node), node, facts);
        }
        "non_null_expression" => {
            push_risk("non_null_assertion", node_text(source, node), node, facts);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_node(child, source, exported, facts);
    }
}

fn push_named_symbol(
    node: Node<'_>,
    source: &str,
    kind: &str,
    exported: bool,
    facts: &mut SyntaxFacts,
) {
    let name = node
        .child_by_field_name("name")
        .and_then(|name| text_if_named(source, name))
        .or_else(|| first_identifier_text(source, node))
        .unwrap_or_else(|| {
            if exported {
                "default".to_owned()
            } else {
                "<anonymous>".to_owned()
            }
        });
    let public_surface = exported
        || looks_native_or_binding(&name)
        || looks_native_or_binding(node_text(source, node));
    let span = SyntaxSpan::from_node(node);

    facts.symbols.push(SyntaxSymbol {
        kind: kind.to_owned(),
        name: name.clone(),
        span,
        exported,
        public_surface,
        parameters: Vec::new(),
        ffi_entry: false,
    });

    if exported {
        facts.exports.push(SyntaxExport {
            kind: kind.to_owned(),
            name: name.clone(),
            span,
        });
    }

    if public_surface && looks_native_or_binding(&name) {
        facts.risk_seams.push(SyntaxRiskSeam {
            kind: "native_boundary_hint".to_owned(),
            evidence: name,
            span,
            test_context: false,
        });
    }
}

fn push_static_import(node: Node<'_>, source: &str, facts: &mut SyntaxFacts) {
    let module = node
        .child_by_field_name("source")
        .and_then(|source_node| cleaned_string(source, source_node))
        .or_else(|| first_string_text(source, node).and_then(|module| cleaned_literal(&module)));
    let imported = named_imports(source, node);

    facts.imports.push(SyntaxImport {
        kind: "static".to_owned(),
        module,
        imported,
        dynamic: false,
        span: SyntaxSpan::from_node(node),
    });
}

fn push_call_expression(node: Node<'_>, source: &str, facts: &mut SyntaxFacts) {
    let callee = node
        .child_by_field_name("function")
        .map(|callee| compact_text(node_text(source, callee)))
        .or_else(|| first_named_child_text(source, node))
        .unwrap_or_else(|| "<unknown>".to_owned());
    let span = SyntaxSpan::from_node(node);
    let dynamic = is_dynamic_callee(&callee);

    facts.call_sites.push(SyntaxCallSite {
        kind: "call".to_owned(),
        callee: callee.clone(),
        dynamic,
        span,
    });

    if callee == "import" {
        let module = first_string_text(source, node).and_then(|module| cleaned_literal(&module));
        facts.imports.push(SyntaxImport {
            kind: "dynamic".to_owned(),
            module,
            imported: Vec::new(),
            dynamic: true,
            span,
        });
        push_risk("dynamic_import", "import(...)", node, facts);
    }

    if dynamic {
        push_risk("dynamic_call", callee.as_str(), node, facts);
    }
    if is_entrypoint_callee(&callee) {
        push_risk("entrypoint", callee.as_str(), node, facts);
    }
    if looks_native_or_binding(&callee) || looks_native_or_binding(node_text(source, node)) {
        push_risk("native_boundary_hint", callee.as_str(), node, facts);
    }
}

fn push_new_expression(node: Node<'_>, source: &str, facts: &mut SyntaxFacts) {
    let callee = node
        .child_by_field_name("constructor")
        .map(|callee| compact_text(node_text(source, callee)))
        .or_else(|| first_named_child_text(source, node))
        .unwrap_or_else(|| "<unknown>".to_owned());
    let span = SyntaxSpan::from_node(node);
    let dynamic = is_dynamic_callee(&callee);

    facts.call_sites.push(SyntaxCallSite {
        kind: "new".to_owned(),
        callee: callee.clone(),
        dynamic,
        span,
    });

    if callee == "Function" {
        push_risk("dynamic_call", "new Function(...)", node, facts);
    }
}

fn push_risk(kind: &str, evidence: &str, node: Node<'_>, facts: &mut SyntaxFacts) {
    facts.risk_seams.push(SyntaxRiskSeam {
        kind: kind.to_owned(),
        evidence: compact_text(evidence),
        span: SyntaxSpan::from_node(node),
        test_context: false,
    });
}

fn node_text<'source>(source: &'source str, node: Node<'_>) -> &'source str {
    source.get(node.byte_range()).unwrap_or("")
}

fn text_if_named(source: &str, node: Node<'_>) -> Option<String> {
    node.is_named()
        .then(|| compact_text(node_text(source, node)))
}

fn first_named_child_text(source: &str, node: Node<'_>) -> Option<String> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.is_named())
        .map(|child| compact_text(node_text(source, child)))
}

fn first_identifier_text(source: &str, node: Node<'_>) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "identifier" | "property_identifier" | "type_identifier"
        ) {
            return Some(compact_text(node_text(source, child)));
        }
    }
    None
}

fn first_string_text(source: &str, node: Node<'_>) -> Option<String> {
    if matches!(node.kind(), "string" | "string_fragment") {
        return Some(compact_text(node_text(source, node)));
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(value) = first_string_text(source, child) {
            return Some(value);
        }
    }
    None
}

fn cleaned_string(source: &str, node: Node<'_>) -> Option<String> {
    cleaned_literal(node_text(source, node))
}

fn cleaned_literal(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.len() >= 2 {
        let first = trimmed.as_bytes()[0];
        let last = trimmed.as_bytes()[trimmed.len() - 1];
        if matches!(first, b'\'' | b'"' | b'`') && first == last {
            return Some(trimmed[1..trimmed.len() - 1].to_owned());
        }
    }
    None
}

fn named_imports(source: &str, node: Node<'_>) -> Vec<String> {
    let mut names = Vec::new();
    collect_import_identifiers(source, node, &mut names);
    names.sort();
    names.dedup();
    names
}

fn collect_import_identifiers(source: &str, node: Node<'_>, names: &mut Vec<String>) {
    if matches!(node.kind(), "identifier" | "property_identifier") {
        let name = compact_text(node_text(source, node));
        if !matches!(name.as_str(), "from" | "import" | "type") {
            names.push(name);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "string" {
            collect_import_identifiers(source, child, names);
        }
    }
}

fn compact_text(text: &str) -> String {
    let mut compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.len() > 120 {
        compact.truncate(117);
        compact.push_str("...");
    }
    compact
}

fn is_dynamic_callee(callee: &str) -> bool {
    callee == "import"
        || callee == "eval"
        || callee == "Function"
        || callee == "Reflect.apply"
        || callee.contains('[')
}

fn is_entrypoint_callee(callee: &str) -> bool {
    matches!(callee, "Bun.serve" | "addEventListener")
}

fn looks_native_or_binding(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("ffi")
        || lower.contains("dlopen")
        || lower.contains("native")
        || lower.contains("binding")
}
