use super::capability::{AST_SHADOW_SCHEMA_VERSION, AstLanguage, AstParserStatus, capabilities};
use super::python::{
    PythonAstError, PythonAstShadow, PythonLandmark, PythonLandmarkKind, parse_python_landmarks,
};
use super::rust::{
    RustAstError, RustAstShadow, RustLandmark, RustLandmarkKind, parse_rust_landmarks,
};
use super::typescript::{
    TypeScriptAstError, TypeScriptAstShadow, TypeScriptLandmark, TypeScriptLandmarkKind,
    parse_typescript_landmarks,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_SHADOW_OUTPUT_DIR: &str = "target/tokmd-ast-shadow";
pub const HEURISTIC_ARTIFACT: &str = "heuristic.json";
pub const AST_ARTIFACT: &str = "ast.json";
pub const DIFF_ARTIFACT: &str = "diff.json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShadowArtifactSet {
    pub output_dir: &'static str,
    pub heuristic: &'static str,
    pub ast: &'static str,
    pub diff: &'static str,
}

pub const DEFAULT_SHADOW_ARTIFACTS: ShadowArtifactSet = ShadowArtifactSet {
    output_dir: DEFAULT_SHADOW_OUTPUT_DIR,
    heuristic: HEURISTIC_ARTIFACT,
    ast: AST_ARTIFACT,
    diff: DIFF_ARTIFACT,
};

#[must_use]
pub const fn default_shadow_artifacts() -> &'static ShadowArtifactSet {
    &DEFAULT_SHADOW_ARTIFACTS
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShadowArtifacts {
    pub heuristic: Value,
    pub ast: Value,
    pub diff: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShadowArtifactPaths {
    pub heuristic: PathBuf,
    pub ast: PathBuf,
    pub diff: PathBuf,
}

#[derive(Debug)]
pub enum ShadowArtifactError {
    AbsolutePath(String),
    PythonAst(PythonAstError),
    RustAst(RustAstError),
    TypeScriptAst(TypeScriptAstError),
}

impl fmt::Display for ShadowArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AbsolutePath(path) => {
                write!(f, "AST shadow artifact paths must be relative: {path}")
            }
            Self::PythonAst(error) => write!(f, "{error}"),
            Self::RustAst(error) => write!(f, "{error}"),
            Self::TypeScriptAst(error) => write!(f, "{error}"),
        }
    }
}

impl Error for ShadowArtifactError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::AbsolutePath(_) => None,
            Self::PythonAst(error) => Some(error),
            Self::RustAst(error) => Some(error),
            Self::TypeScriptAst(error) => Some(error),
        }
    }
}

impl From<PythonAstError> for ShadowArtifactError {
    fn from(error: PythonAstError) -> Self {
        Self::PythonAst(error)
    }
}

impl From<RustAstError> for ShadowArtifactError {
    fn from(error: RustAstError) -> Self {
        Self::RustAst(error)
    }
}

impl From<TypeScriptAstError> for ShadowArtifactError {
    fn from(error: TypeScriptAstError) -> Self {
        Self::TypeScriptAst(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShadowFileInput<'a> {
    pub path: &'a str,
    pub language: AstLanguage,
    pub source: &'a str,
    pub heuristic_landmarks: &'a [ShadowLandmark],
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ShadowLandmark {
    pub kind: String,
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
}

impl ShadowLandmark {
    #[must_use]
    pub fn function(name: impl Into<String>, start_line: usize, end_line: usize) -> Self {
        Self {
            kind: "function".to_owned(),
            name: name.into(),
            start_line,
            end_line,
        }
    }
}

#[must_use]
pub fn normalize_shadow_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_owned()
}

pub fn build_shadow_artifacts(
    files: &[ShadowFileInput<'_>],
) -> Result<ShadowArtifacts, ShadowArtifactError> {
    let mut inputs = files
        .iter()
        .map(|input| normalized_input_path(input.path).map(|path| (path, input)))
        .collect::<Result<Vec<_>, _>>()?;
    inputs.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.language.as_str().cmp(right.1.language.as_str()))
    });

    let mut heuristic_files = Vec::with_capacity(inputs.len());
    let mut ast_files = Vec::with_capacity(inputs.len());
    let mut diff_files = Vec::with_capacity(inputs.len());
    let mut diff_summary = ShadowDiffSummary::default();

    for (path, input) in inputs {
        let mut heuristic_landmarks = input.heuristic_landmarks.to_vec();
        heuristic_landmarks.sort();

        let (mut ast_landmarks, parse_degraded, unsupported, parser_status) = match input.language {
            AstLanguage::Rust => {
                let ast_shadow = parse_rust_landmarks(input.source)?;
                (
                    shadow_landmarks_from_rust(&ast_shadow),
                    ast_shadow.has_error,
                    false,
                    AstParserStatus::ParserBackedShadow,
                )
            }
            AstLanguage::Python => {
                let ast_shadow = parse_python_landmarks(input.source)?;
                (
                    shadow_landmarks_from_python(&ast_shadow),
                    ast_shadow.has_error,
                    false,
                    AstParserStatus::ParserBackedShadow,
                )
            }
            AstLanguage::TypeScript | AstLanguage::Tsx => {
                let ast_shadow = parse_typescript_landmarks(input.source, input.language)?;
                (
                    shadow_landmarks_from_typescript(&ast_shadow),
                    ast_shadow.has_error,
                    false,
                    AstParserStatus::ParserBackedShadow,
                )
            }
        };
        ast_landmarks.sort();

        let diff = compare_landmarks(&heuristic_landmarks, &ast_landmarks);
        diff_summary.add_file(&diff, parse_degraded, unsupported);

        heuristic_files.push(json!({
            "path": path,
            "language": input.language.as_str(),
            "source": "caller_supplied",
            "landmarks": landmarks_value(&heuristic_landmarks),
        }));
        ast_files.push(json!({
            "path": path,
            "language": input.language.as_str(),
            "parser_status": parser_status_value(parser_status),
            "has_error": parse_degraded,
            "landmarks": landmarks_value(&ast_landmarks),
        }));
        diff_files.push(json!({
            "path": path,
            "language": input.language.as_str(),
            "status": if unsupported {
                "unsupported"
            } else if parse_degraded {
                "parse_degraded"
            } else {
                "compared"
            },
            "parse_degraded": parse_degraded,
            "unsupported": unsupported,
            "matches": landmarks_value(&diff.matches),
            "heuristic_only": landmarks_value(&diff.heuristic_only),
            "ast_only": landmarks_value(&diff.ast_only),
        }));
    }

    Ok(ShadowArtifacts {
        heuristic: json!({
            "schema": AST_SHADOW_SCHEMA_VERSION,
            "kind": "heuristic",
            "files": heuristic_files,
        }),
        ast: json!({
            "schema": AST_SHADOW_SCHEMA_VERSION,
            "kind": "ast",
            "capabilities": capabilities_value(),
            "files": ast_files,
        }),
        diff: json!({
            "schema": AST_SHADOW_SCHEMA_VERSION,
            "kind": "diff",
            "summary": diff_summary.value(),
            "files": diff_files,
        }),
    })
}

pub fn write_shadow_artifacts(
    out_dir: impl AsRef<Path>,
    artifacts: &ShadowArtifacts,
) -> anyhow::Result<ShadowArtifactPaths> {
    let out_dir = out_dir.as_ref();
    fs::create_dir_all(out_dir)?;

    let paths = ShadowArtifactPaths {
        heuristic: out_dir.join(HEURISTIC_ARTIFACT),
        ast: out_dir.join(AST_ARTIFACT),
        diff: out_dir.join(DIFF_ARTIFACT),
    };
    write_pretty_json(&paths.heuristic, &artifacts.heuristic)?;
    write_pretty_json(&paths.ast, &artifacts.ast)?;
    write_pretty_json(&paths.diff, &artifacts.diff)?;

    Ok(paths)
}

fn normalized_input_path(path: &str) -> Result<String, ShadowArtifactError> {
    if is_absolute_input_path(path) {
        return Err(ShadowArtifactError::AbsolutePath(path.to_owned()));
    }
    Ok(normalize_shadow_path(path))
}

fn is_absolute_input_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.starts_with('/')
        || normalized.starts_with("//")
        || normalized
            .as_bytes()
            .get(1)
            .is_some_and(|separator| *separator == b':')
}

fn write_pretty_json(path: &Path, value: &Value) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

fn capabilities_value() -> Vec<Value> {
    capabilities()
        .iter()
        .map(|capability| {
            json!({
                "language": capability.language.as_str(),
                "parser_crate": capability.parser_crate,
                "parser_status": parser_status_value(capability.parser_status),
                "shadow_only": capability.shadow_only,
                "changes_default_receipts": capability.changes_default_receipts,
            })
        })
        .collect()
}

fn parser_status_value(status: AstParserStatus) -> &'static str {
    match status {
        AstParserStatus::ParserBackedShadow => "parser_backed_shadow",
        AstParserStatus::Unsupported => "unsupported",
    }
}

fn shadow_landmarks_from_python(shadow: &PythonAstShadow) -> Vec<ShadowLandmark> {
    shadow
        .landmarks
        .iter()
        .map(shadow_landmark_from_python)
        .collect()
}

fn shadow_landmark_from_python(landmark: &PythonLandmark) -> ShadowLandmark {
    ShadowLandmark {
        kind: python_landmark_kind_value(landmark.kind).to_owned(),
        name: landmark.name.clone(),
        start_line: landmark.start_line,
        end_line: landmark.end_line,
    }
}

fn python_landmark_kind_value(kind: PythonLandmarkKind) -> &'static str {
    match kind {
        PythonLandmarkKind::ControlFlow => "control_flow",
        PythonLandmarkKind::Function => "function",
        PythonLandmarkKind::Import => "import",
    }
}

fn shadow_landmarks_from_typescript(shadow: &TypeScriptAstShadow) -> Vec<ShadowLandmark> {
    shadow
        .landmarks
        .iter()
        .map(shadow_landmark_from_typescript)
        .collect()
}

fn shadow_landmark_from_typescript(landmark: &TypeScriptLandmark) -> ShadowLandmark {
    ShadowLandmark {
        kind: typescript_landmark_kind_value(landmark.kind).to_owned(),
        name: landmark.name.clone(),
        start_line: landmark.start_line,
        end_line: landmark.end_line,
    }
}

fn typescript_landmark_kind_value(kind: TypeScriptLandmarkKind) -> &'static str {
    match kind {
        TypeScriptLandmarkKind::ControlFlow => "control_flow",
        TypeScriptLandmarkKind::Function => "function",
        TypeScriptLandmarkKind::Import => "import",
    }
}

fn shadow_landmarks_from_rust(shadow: &RustAstShadow) -> Vec<ShadowLandmark> {
    shadow
        .landmarks
        .iter()
        .map(shadow_landmark_from_rust)
        .collect()
}

fn shadow_landmark_from_rust(landmark: &RustLandmark) -> ShadowLandmark {
    ShadowLandmark {
        kind: rust_landmark_kind_value(landmark.kind).to_owned(),
        name: landmark.name.clone(),
        start_line: landmark.start_line,
        end_line: landmark.end_line,
    }
}

fn rust_landmark_kind_value(kind: RustLandmarkKind) -> &'static str {
    match kind {
        RustLandmarkKind::ControlFlow => "control_flow",
        RustLandmarkKind::Function => "function",
        RustLandmarkKind::Import => "import",
    }
}

struct LandmarkDiff {
    matches: Vec<ShadowLandmark>,
    heuristic_only: Vec<ShadowLandmark>,
    ast_only: Vec<ShadowLandmark>,
}

#[derive(Default)]
struct ShadowDiffSummary {
    files: usize,
    matched: usize,
    heuristic_only: usize,
    ast_only: usize,
    parse_degraded: usize,
    unsupported: usize,
}

impl ShadowDiffSummary {
    fn add_file(&mut self, diff: &LandmarkDiff, parse_degraded: bool, unsupported: bool) {
        self.files += 1;
        self.matched += diff.matches.len();
        self.heuristic_only += diff.heuristic_only.len();
        self.ast_only += diff.ast_only.len();
        self.parse_degraded += usize::from(parse_degraded);
        self.unsupported += usize::from(unsupported);
    }

    fn value(&self) -> Value {
        json!({
            "files": self.files,
            "matched": self.matched,
            "heuristic_only": self.heuristic_only,
            "ast_only": self.ast_only,
            "parse_degraded": self.parse_degraded,
            "unsupported": self.unsupported,
        })
    }
}

fn compare_landmarks(heuristic: &[ShadowLandmark], ast: &[ShadowLandmark]) -> LandmarkDiff {
    let heuristic = heuristic.iter().cloned().collect::<BTreeSet<_>>();
    let ast = ast.iter().cloned().collect::<BTreeSet<_>>();

    LandmarkDiff {
        matches: heuristic.intersection(&ast).cloned().collect(),
        heuristic_only: heuristic.difference(&ast).cloned().collect(),
        ast_only: ast.difference(&heuristic).cloned().collect(),
    }
}

fn landmarks_value(landmarks: &[ShadowLandmark]) -> Vec<Value> {
    landmarks
        .iter()
        .map(|landmark| {
            json!({
                "kind": landmark.kind,
                "name": landmark.name,
                "start_line": landmark.start_line,
                "end_line": landmark.end_line,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        AST_ARTIFACT, DEFAULT_SHADOW_ARTIFACTS, DIFF_ARTIFACT, HEURISTIC_ARTIFACT,
        ShadowArtifactError, ShadowFileInput, ShadowLandmark, build_shadow_artifacts,
        normalize_shadow_path, write_shadow_artifacts,
    };
    use crate::ast::AstLanguage;

    #[test]
    fn artifact_names_match_adr_contract() {
        assert_eq!(DEFAULT_SHADOW_ARTIFACTS.heuristic, HEURISTIC_ARTIFACT);
        assert_eq!(DEFAULT_SHADOW_ARTIFACTS.ast, AST_ARTIFACT);
        assert_eq!(DEFAULT_SHADOW_ARTIFACTS.diff, DIFF_ARTIFACT);
    }

    #[test]
    fn builds_deterministic_shadow_artifacts_for_rust() {
        let heuristic = [ShadowLandmark::function("top_level", 1, 1)];
        let files = [
            ShadowFileInput {
                path: "src/z.rs",
                language: AstLanguage::Rust,
                source: "fn zed() {}\n",
                heuristic_landmarks: &[],
            },
            ShadowFileInput {
                path: ".\\src\\lib.rs",
                language: AstLanguage::Rust,
                source: "fn top_level() {}\nfn ast_only() {}\n",
                heuristic_landmarks: &heuristic,
            },
        ];

        let artifacts = build_shadow_artifacts(&files).expect("shadow artifacts should build");

        assert_eq!(artifacts.heuristic["schema"], "tokmd.ast_shadow.v1");
        assert_eq!(artifacts.heuristic["kind"], "heuristic");
        assert_eq!(artifacts.ast["kind"], "ast");
        assert_eq!(artifacts.diff["kind"], "diff");
        assert_eq!(artifacts.heuristic["files"][0]["path"], "src/lib.rs");
        assert_eq!(artifacts.heuristic["files"][1]["path"], "src/z.rs");
        assert_eq!(
            artifacts.diff["files"][0]["matches"][0]["name"],
            "top_level"
        );
        assert_eq!(
            artifacts.diff["files"][0]["ast_only"][0]["name"],
            "ast_only"
        );
        assert_eq!(artifacts.diff["summary"]["files"], 2);
        assert_eq!(artifacts.diff["summary"]["matched"], 1);
        assert_eq!(artifacts.diff["summary"]["heuristic_only"], 0);
        assert_eq!(artifacts.diff["summary"]["ast_only"], 2);
        assert_eq!(artifacts.diff["summary"]["parse_degraded"], 0);
        assert_eq!(artifacts.diff["summary"]["unsupported"], 0);
        assert!(artifacts.heuristic.get("generated_at").is_none());
        assert!(artifacts.ast.get("generated_at").is_none());
        assert!(artifacts.diff.get("generated_at").is_none());
    }

    #[test]
    fn diff_summary_counts_match_comparison_entries() {
        let heuristic = [ShadowLandmark::function("heuristic_only", 1, 1)];
        let files = [ShadowFileInput {
            path: "src/lib.rs",
            language: AstLanguage::Rust,
            source: "fn ast_only() {}\n",
            heuristic_landmarks: &heuristic,
        }];

        let artifacts = build_shadow_artifacts(&files).expect("shadow artifacts should build");

        assert_eq!(artifacts.diff["summary"]["files"], 1);
        assert_eq!(artifacts.diff["summary"]["matched"], 0);
        assert_eq!(artifacts.diff["summary"]["heuristic_only"], 1);
        assert_eq!(artifacts.diff["summary"]["ast_only"], 1);
        assert_eq!(artifacts.diff["summary"]["parse_degraded"], 0);
        assert_eq!(artifacts.diff["summary"]["unsupported"], 0);
        assert_eq!(
            artifacts.diff["summary"]["heuristic_only"]
                .as_u64()
                .unwrap(),
            artifacts.diff["files"][0]["heuristic_only"]
                .as_array()
                .unwrap()
                .len() as u64
        );
        assert_eq!(
            artifacts.diff["summary"]["ast_only"].as_u64().unwrap(),
            artifacts.diff["files"][0]["ast_only"]
                .as_array()
                .unwrap()
                .len() as u64
        );
    }

    #[test]
    fn marks_parse_degraded_files_without_claiming_failure() {
        let files = [ShadowFileInput {
            path: "src/lib.rs",
            language: AstLanguage::Rust,
            source: "fn ok() {}\nfn broken(",
            heuristic_landmarks: &[],
        }];

        let artifacts = build_shadow_artifacts(&files).expect("Tree-sitter should recover");

        assert_eq!(artifacts.ast["files"][0]["has_error"], true);
        assert_eq!(artifacts.diff["files"][0]["status"], "parse_degraded");
        assert_eq!(artifacts.diff["files"][0]["parse_degraded"], true);
        assert_eq!(artifacts.diff["summary"]["parse_degraded"], 1);
    }

    #[test]
    fn compares_python_shadow_inputs_without_marking_unsupported() -> Result<(), ShadowArtifactError>
    {
        let heuristic = [ShadowLandmark::function("run", 1, 2)];
        let files = [ShadowFileInput {
            path: "tools/run.py",
            language: AstLanguage::Python,
            source: "import os\n\ndef run():\n    if True:\n        return 1\n",
            heuristic_landmarks: &heuristic,
        }];

        let artifacts = build_shadow_artifacts(&files)?;

        assert_eq!(
            artifacts.ast["files"][0]["parser_status"],
            "parser_backed_shadow"
        );
        assert_eq!(artifacts.diff["files"][0]["status"], "compared");
        assert_eq!(artifacts.diff["files"][0]["unsupported"], false);
        assert_eq!(artifacts.diff["summary"]["unsupported"], 0);
        Ok(())
    }

    #[test]
    fn writes_shadow_artifacts_to_expected_names() {
        let temp = tempfile::tempdir().expect("tempdir should be created");
        let files = [ShadowFileInput {
            path: "src/lib.rs",
            language: AstLanguage::Rust,
            source: "fn top_level() {}\n",
            heuristic_landmarks: &[],
        }];
        let artifacts = build_shadow_artifacts(&files).expect("shadow artifacts should build");

        let paths =
            write_shadow_artifacts(temp.path(), &artifacts).expect("artifacts should write");

        assert_eq!(paths.heuristic.file_name().unwrap(), HEURISTIC_ARTIFACT);
        assert_eq!(paths.ast.file_name().unwrap(), AST_ARTIFACT);
        assert_eq!(paths.diff.file_name().unwrap(), DIFF_ARTIFACT);
        assert!(paths.heuristic.exists());
        assert!(paths.ast.exists());
        assert!(paths.diff.exists());

        let ast = std::fs::read_to_string(paths.ast).expect("ast artifact should be readable");
        assert!(ast.contains("\"schema\": \"tokmd.ast_shadow.v1\""));
        assert!(ast.ends_with('\n'));
    }

    #[test]
    fn normalizes_shadow_paths_without_absolute_leakage() {
        assert_eq!(normalize_shadow_path(".\\src\\main.rs"), "src/main.rs");
        assert_eq!(normalize_shadow_path("./src/lib.rs"), "src/lib.rs");
        assert_eq!(normalize_shadow_path("/src/lib.rs"), "src/lib.rs");
    }

    #[test]
    fn rejects_absolute_paths_before_artifact_building() {
        let files = [ShadowFileInput {
            path: "C:\\repo\\src\\lib.rs",
            language: AstLanguage::Rust,
            source: "fn top_level() {}\n",
            heuristic_landmarks: &[],
        }];

        let error = build_shadow_artifacts(&files).expect_err("absolute paths should be rejected");

        assert!(matches!(error, ShadowArtifactError::AbsolutePath(_)));
    }
}
