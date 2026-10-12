use crate::{Cancellation, CoreError};
use std::ops::ControlFlow;
use tree_sitter::{Node, ParseOptions, Parser};

pub(crate) type SyntaxError = (usize, usize, &'static str);

#[derive(Clone, Debug)]
pub(crate) struct Reference {
    pub value: String,
    pub kind: String,
    pub rule: String,
    pub start: usize,
    pub end: usize,
    pub mode: Mode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Java,
    JavaImport,
    JavaWildcard,
    File,
    Url,
    Js,
    JsCandidate,
    Literal(bool),
    Dynamic,
}

#[derive(Default)]
pub(crate) struct Extraction {
    pub package: String,
    pub declarations: Vec<Declaration>,
    pub references: Vec<Reference>,
    pub errors: Vec<SyntaxError>,
    pub routes: Vec<Route>,
}

#[derive(Clone, Debug)]
pub(crate) struct Declaration {
    pub name: String,
    pub kind: String,
    pub start: usize,
    pub end: usize,
    pub scope_start: usize,
    pub scope_end: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Route {
    pub pattern: String,
    pub target: String,
    pub java: bool,
    pub evidence: Vec<(usize, usize)>,
}

pub(crate) fn nodes(root: Node<'_>) -> Vec<Node<'_>> {
    let mut stack = vec![root];
    let mut out = Vec::new();
    while let Some(n) = stack.pop() {
        out.push(n);
        for i in (0..n.named_child_count()).rev() {
            if let Some(c) = n.named_child(i as u32) {
                stack.push(c);
            }
        }
    }
    out
}

pub(crate) fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

pub(crate) fn tree(
    source: &str,
    java: bool,
    cancel: &Cancellation,
) -> Result<tree_sitter::Tree, CoreError> {
    cancel.check()?;
    let mut parser = Parser::new();
    let language = if java {
        tree_sitter_java::LANGUAGE
    } else {
        tree_sitter_javascript::LANGUAGE
    };
    parser
        .set_language(&language.into())
        .map_err(|_| CoreError::Internal)?;
    let mut progress = |_: &tree_sitter::ParseState| {
        if cancel.is_cancelled() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let result = parser.parse_with_options(
        &mut |offset, _| &source.as_bytes()[offset..],
        None,
        Some(ParseOptions::new().progress_callback(&mut progress)),
    );
    cancel.check()?;
    result.ok_or(CoreError::Internal)
}

/// UTF-8 byte correspondence back to the original decoded source.
pub(crate) struct Mapped {
    pub text: String,
    pub spans: Vec<(usize, usize)>,
}
impl Mapped {
    pub fn identity(text: &str, start: usize) -> Self {
        Self {
            text: text.into(),
            spans: (0..text.len())
                .map(|i| (start + i, start + i + 1))
                .collect(),
        }
    }
    pub fn wrap(self, prefix: &str, suffix: &str) -> Self {
        let mut spans = vec![(usize::MAX, usize::MAX); prefix.len()];
        spans.extend(self.spans);
        spans.extend(vec![(usize::MAX, usize::MAX); suffix.len()]);
        Self {
            text: format!("{prefix}{}{suffix}", self.text),
            spans,
        }
    }
    pub fn range(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        let first = self.spans.get(start)?;
        let last = self.spans.get(end.saturating_sub(1))?;
        if first.0 == usize::MAX || last.1 == usize::MAX {
            None
        } else {
            Some((first.0, last.1))
        }
    }
}

pub(crate) fn unicode_java(
    input: Mapped,
    cancel: &Cancellation,
) -> Result<(Mapped, Vec<SyntaxError>), CoreError> {
    let mut output = Mapped {
        text: String::new(),
        spans: Vec::new(),
    };
    let mut errors = Vec::new();
    let mut i = 0;
    let mut slash_run = 0;
    let mut previous_escape = false;
    while i < input.text.len() {
        if i % 4096 == 0 {
            cancel.check()?;
        }
        let remaining = &input.text[i..];
        let mut end = i + remaining.chars().next().unwrap().len_utf8();
        let mut produced = remaining.chars().next().unwrap();
        let mut escaped = false;
        if produced == '\\'
            && (previous_escape || slash_run % 2 == 0)
            && remaining.starts_with("\\u")
        {
            let mut h = i + 1;
            while input.text.as_bytes().get(h) == Some(&b'u') {
                h += 1;
            }
            let raw = input
                .text
                .get(h..h + 4)
                .and_then(|s| u16::from_str_radix(s, 16).ok());
            match raw {
                Some(v) if !(0xd800..=0xdfff).contains(&v) => {
                    produced = char::from_u32(v as u32).unwrap();
                    end = h + 4;
                    escaped = true;
                }
                Some(v) if (0xd800..=0xdbff).contains(&v) => {
                    let next = h + 4;
                    let mut h2 = next + 1;
                    if input.text.get(next..).unwrap_or("").starts_with("\\u") {
                        while input.text.as_bytes().get(h2) == Some(&b'u') {
                            h2 += 1;
                        }
                        if let Some(low) = input
                            .text
                            .get(h2..h2 + 4)
                            .and_then(|s| u16::from_str_radix(s, 16).ok())
                            .filter(|v| (0xdc00..=0xdfff).contains(v))
                        {
                            produced = char::from_u32(
                                0x10000 + (((v - 0xd800) as u32) << 10) + (low - 0xdc00) as u32,
                            )
                            .unwrap();
                            end = h2 + 4;
                            escaped = true;
                        }
                    }
                    if !escaped {
                        if let Some((s, e)) = input.range(i, (h + 4).min(input.text.len())) {
                            errors.push((s, e, "unicode-escape"));
                        }
                    }
                }
                _ => {
                    if let Some((s, e)) = input.range(i, (h + 4).min(input.text.len())) {
                        errors.push((s, e, "unicode-escape"));
                    }
                }
            }
        }
        output.text.push(produced);
        let span = input.range(i, end).unwrap_or((usize::MAX, usize::MAX));
        output
            .spans
            .extend(std::iter::repeat_n(span, produced.len_utf8()));
        slash_run = if produced == '\\' { slash_run + 1 } else { 0 };
        previous_escape = escaped;
        i = end;
    }
    Ok((output, errors))
}

pub(crate) fn java(mapped: Mapped, cancel: &Cancellation) -> Result<Extraction, CoreError> {
    let (mapped, mut errors) = unicode_java(mapped, cancel)?;
    let parsed = tree(&mapped.text, true, cancel)?;
    let all = nodes(parsed.root_node());
    for n in &all {
        if n.is_error() || n.is_missing() {
            if let Some((s, e)) = mapped.range(n.start_byte(), n.end_byte().max(n.start_byte() + 1))
            {
                errors.push((s, e, "syntax"));
            }
        }
    }
    let mut result = Extraction {
        errors,
        ..Default::default()
    };
    let mut type_params = Vec::new();
    for n in &all {
        cancel.check()?;
        if n.kind() == "package_declaration" {
            result.package = text(*n, &mapped.text)
                .trim_start_matches("package")
                .trim()
                .trim_end_matches(';')
                .trim()
                .into();
        }
        if matches!(
            n.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            if let Some(name) = n.child_by_field_name("name") {
                if let Some((start, end)) = mapped.range(name.start_byte(), name.end_byte()) {
                    let mut names = vec![text(name, &mapped.text).to_owned()];
                    let mut parent = n.parent();
                    while let Some(p) = parent {
                        if matches!(
                            p.kind(),
                            "class_declaration"
                                | "interface_declaration"
                                | "enum_declaration"
                                | "record_declaration"
                        ) {
                            if let Some(pn) = p.child_by_field_name("name") {
                                names.push(text(pn, &mapped.text).into());
                            }
                        }
                        parent = p.parent();
                    }
                    names.reverse();
                    let scoped = names.join(".");
                    let full = if result.package.is_empty() {
                        scoped
                    } else {
                        format!("{}.{scoped}", result.package)
                    };
                    let (scope_start, scope_end) = mapped
                        .range(n.start_byte(), n.end_byte())
                        .unwrap_or((start, end));
                    result.declarations.push(Declaration {
                        name: full,
                        kind: n.kind().trim_end_matches("_declaration").into(),
                        start,
                        end,
                        scope_start,
                        scope_end,
                    });
                }
            }
        }
        if n.kind() == "type_parameter" {
            if let Some(name) = n.named_child(0) {
                let scope = n.parent().and_then(|p| p.parent()).unwrap_or(*n);
                type_params.push((
                    text(name, &mapped.text).to_owned(),
                    scope.start_byte(),
                    scope.end_byte(),
                ));
            }
        }
    }
    for n in &all {
        cancel.check()?;
        if n.kind() == "import_declaration" {
            if let Some((start, end)) = mapped.range(n.start_byte(), n.end_byte()) {
                let value = text(*n, &mapped.text)
                    .trim_start_matches("import")
                    .trim()
                    .trim_end_matches(';')
                    .trim();
                let is_static = value.starts_with("static ");
                let value = value.trim_start_matches("static ").trim();
                let wildcard = value.ends_with(".*");
                let value = if is_static {
                    value.rsplit_once('.').map(|p| p.0).unwrap_or(value)
                } else {
                    value
                };
                result.references.push(Reference {
                    value: value.into(),
                    kind: "import".into(),
                    rule: "JAVA-IMPORT".into(),
                    start,
                    end,
                    mode: if wildcard && !is_static {
                        Mode::JavaWildcard
                    } else {
                        Mode::JavaImport
                    },
                });
            }
        }
        let annotation = matches!(n.kind(), "annotation" | "marker_annotation");
        if matches!(n.kind(), "type_identifier" | "scoped_type_identifier") || annotation {
            if n.parent().is_some_and(|p| {
                matches!(p.kind(), "scoped_type_identifier" | "type_parameter")
                    || p.child_by_field_name("name")
                        .is_some_and(|name| name.id() == n.id())
                        && matches!(
                            p.kind(),
                            "class_declaration"
                                | "interface_declaration"
                                | "enum_declaration"
                                | "record_declaration"
                                | "annotation_type_declaration"
                        )
            }) {
                continue;
            }
            let target = if annotation {
                n.child_by_field_name("name").unwrap_or(*n)
            } else {
                *n
            };
            let value = text(target, &mapped.text).to_owned();
            if value == "var"
                || type_params.iter().any(|(name, s, e)| {
                    *name == value && n.start_byte() >= *s && n.end_byte() <= *e
                })
            {
                continue;
            }
            let mut parent = n.parent();
            let mut kind = "type";
            let mut inside_import = false;
            while let Some(p) = parent {
                if p.kind() == "import_declaration" {
                    inside_import = true;
                    break;
                }
                if p.kind() == "superclass" {
                    kind = "extends";
                    break;
                }
                if p.kind() == "super_interfaces" || p.kind() == "extends_interfaces" {
                    kind = if p.kind() == "extends_interfaces" {
                        "extends"
                    } else {
                        "implements"
                    };
                    break;
                }
                parent = p.parent();
            }
            if inside_import {
                continue;
            }
            if let Some((start, end)) = mapped.range(target.start_byte(), target.end_byte()) {
                if !result.errors.iter().any(|(s, e, _)| start < *e && end > *s) {
                    result.references.push(Reference {
                        value,
                        kind: kind.into(),
                        rule: "JAVA-TYPE".into(),
                        start,
                        end,
                        mode: Mode::Java,
                    });
                }
            }
        }
    }
    Ok(result)
}

fn js_literal(node: Node<'_>, source: &str) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
    let s = text(node, source);
    let content = s.get(1..s.len().checked_sub(1)?)?;
    let mut out = Vec::<u16>::new();
    let mut chars = content.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.extend(ch.encode_utf16(&mut [0; 2]).iter().copied());
            continue;
        }
        match chars.next()? {
            '\\' => out.push(b'\\' as u16),
            '\'' => out.push(b'\'' as u16),
            '"' => out.push(b'"' as u16),
            '/' => out.push(b'/' as u16),
            'n' => out.push(b'\n' as u16),
            'r' => out.push(b'\r' as u16),
            't' => out.push(b'\t' as u16),
            'b' => out.push(8),
            'f' => out.push(12),
            '\n' => {}
            '\r' => {
                if chars.clone().next() == Some('\n') {
                    chars.next();
                }
            }
            escape @ ('u' | 'x') => {
                let braced = escape == 'u' && chars.clone().next() == Some('{');
                if braced {
                    chars.next();
                }
                let mut digits = String::new();
                if braced {
                    loop {
                        let ch = chars.next()?;
                        if ch == '}' {
                            break;
                        }
                        if !ch.is_ascii_hexdigit() || digits.len() >= 6 {
                            return None;
                        }
                        digits.push(ch);
                    }
                } else {
                    for _ in 0..if escape == 'u' { 4 } else { 2 } {
                        digits.push(chars.next()?);
                    }
                }
                let value = u32::from_str_radix(&digits, 16).ok()?;
                if braced {
                    out.extend(
                        char::from_u32(value)?
                            .encode_utf16(&mut [0; 2])
                            .iter()
                            .copied(),
                    );
                } else {
                    out.push(value as u16);
                }
            }
            _ => return None,
        }
    }
    String::from_utf16(&out).ok()
}

pub(crate) fn javascript(source: &str, cancel: &Cancellation) -> Result<Extraction, CoreError> {
    let parsed = tree(source, false, cancel)?;
    let all = nodes(parsed.root_node());
    let mut out = Extraction::default();
    let mut require_scopes = Vec::new();
    for n in &all {
        if n.is_error() || n.is_missing() {
            out.errors.push((n.start_byte(), n.end_byte(), "syntax"));
        }
        if n.kind() == "identifier" && text(*n, source) == "require" {
            if let Some(p) = n.parent() {
                if matches!(
                    p.kind(),
                    "variable_declarator"
                        | "formal_parameters"
                        | "function_declaration"
                        | "import_clause"
                        | "import_specifier"
                        | "required_parameter"
                        | "rest_pattern"
                ) {
                    let mut scope = p;
                    while let Some(s) = scope.parent() {
                        scope = s;
                        if matches!(
                            scope.kind(),
                            "statement_block"
                                | "program"
                                | "function_declaration"
                                | "arrow_function"
                                | "function_expression"
                        ) {
                            break;
                        }
                    }
                    require_scopes.push(scope.byte_range());
                }
            }
        }
    }
    for n in all {
        cancel.check()?;
        let mut mode = Mode::Js;
        let mut target = None;
        if matches!(n.kind(), "import_statement" | "export_statement") {
            target = n.child_by_field_name("source");
        }
        if n.kind() == "call_expression" {
            let function = n.child_by_field_name("function");
            if let Some(f) = function {
                if matches!(text(f, source), "require" | "import") {
                    target = n.child_by_field_name("arguments").and_then(|a| {
                        if a.named_child_count() == 1 {
                            a.named_child(0)
                        } else {
                            Some(a)
                        }
                    });
                    if text(f, source) == "require"
                        && require_scopes.iter().any(|s| s.contains(&n.start_byte()))
                    {
                        mode = Mode::JsCandidate;
                    }
                }
            }
        }
        if let Some(target) = target {
            let value = js_literal(target, source);
            let (value, mode) = if let Some(value) = value {
                (value, mode)
            } else {
                ("<動的参照>".into(), Mode::Dynamic)
            };
            if !out
                .errors
                .iter()
                .any(|(s, e, _)| target.start_byte() < *e && target.end_byte() > *s)
            {
                out.references.push(Reference {
                    value,
                    kind: "module".into(),
                    rule: "JS-MODULE".into(),
                    start: target.start_byte(),
                    end: target.end_byte(),
                    mode,
                });
            }
        }
    }
    Ok(out)
}
