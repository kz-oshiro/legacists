use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub byte: usize,
    pub line: usize,
    pub column: usize,
}

pub fn position(text: &str, byte: usize) -> Position {
    let byte = byte.min(text.len());
    let prefix = &text[..byte];
    Position {
        byte,
        line: prefix.bytes().filter(|b| *b == b'\n').count() + 1,
        column: prefix
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .encode_utf16()
            .count()
            + 1,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub id: String,
    pub resource_id: String,
    pub start: Position,
    pub end: Position,
    pub rule: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub id: String,
    pub path: String,
    pub language: String,
    pub hash: Option<String>,
    pub encoding: Option<String>,
    pub encoding_reason: String,
    pub read: String,
    pub decode: String,
    pub parse: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    pub name: String,
    pub resource_id: String,
    pub kind: String,
    pub start: Position,
    pub end: Position,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Resolution {
    Confirmed,
    Candidate,
    Unresolved,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    pub id: String,
    pub from: String,
    pub kind: String,
    pub status: Resolution,
    pub targets: Vec<String>,
    pub reference: String,
    pub reason: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub id: String,
    pub code: String,
    pub stage: String,
    pub resource_id: Option<String>,
    pub path: Option<String>,
    pub start: Option<Position>,
    pub end: Option<Position>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    Utf8,
    Sjis,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisOptions {
    pub exclusions: Vec<String>,
    pub web_root: Option<String>,
    pub encodings: BTreeMap<String, Encoding>,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            exclusions: [".git", ".svn", "node_modules", "target", "build", "dist"]
                .map(String::from)
                .to_vec(),
            web_root: None,
            encodings: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skip {
    pub path: String,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct AnalysisSnapshot {
    pub id: String,
    pub options: AnalysisOptions,
    pub resources: Vec<Resource>,
    pub symbols: Vec<Symbol>,
    pub relations: Vec<Relation>,
    pub evidences: Vec<Evidence>,
    pub diagnostics: Vec<Diagnostic>,
    pub skipped: Vec<Skip>,
    pub web_roots: Vec<String>,
    pub(crate) sources: BTreeMap<String, String>,
}

impl AnalysisSnapshot {
    pub fn source(&self, id: &str) -> Result<&str, crate::CoreError> {
        self.resources
            .iter()
            .find(|r| r.id == id)
            .ok_or(crate::CoreError::InvalidId)?;
        self.sources
            .get(id)
            .map(String::as_str)
            .ok_or(crate::CoreError::DecodeFailed)
    }
    pub fn resource(&self, id: &str) -> Result<&Resource, crate::CoreError> {
        self.resources
            .iter()
            .find(|r| r.id == id)
            .ok_or(crate::CoreError::InvalidId)
    }
}

pub fn language(path: &str) -> Option<&'static str> {
    match path.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "java" => Some("java"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "jsp" | "jspf" => Some("jsp"),
        "jspx" => Some("jspx"),
        "xhtml" => Some("xhtml"),
        "xml" => Some("xml"),
        "properties" => Some("properties"),
        _ => None,
    }
}
