use crate::syntax::{self, Extraction, Mapped, Mode, Reference, Route};
use crate::{Cancellation, CoreError};
use std::collections::BTreeMap;

#[derive(Clone)]
struct Attribute {
    name: String,
    value: String,
    start: usize,
    end: usize,
}
struct Tag {
    name: String,
    attributes: Vec<Attribute>,
    end: usize,
    closing: bool,
    empty: bool,
}

fn tag(source: &str, start: usize) -> Option<Tag> {
    let bytes = source.as_bytes();
    let mut i = start + 1;
    let closing = bytes.get(i) == Some(&b'/');
    if closing {
        i += 1;
    }
    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    let name_start = i;
    while bytes
        .get(i)
        .is_some_and(|b| b.is_ascii_alphanumeric() || b"_:-.".contains(b))
    {
        i += 1;
    }
    if i == name_start {
        return None;
    }
    let name = source[name_start..i].to_owned();
    let mut attributes = Vec::new();
    loop {
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        if bytes.get(i) == Some(&b'>') {
            return Some(Tag {
                name,
                attributes,
                end: i + 1,
                closing,
                empty: false,
            });
        }
        if bytes.get(i..i + 2) == Some(b"/>") {
            return Some(Tag {
                name,
                attributes,
                end: i + 2,
                closing,
                empty: true,
            });
        }
        let begin = i;
        while bytes
            .get(i)
            .is_some_and(|b| !b.is_ascii_whitespace() && !b"=/>".contains(b))
        {
            i += 1;
        }
        if begin == i {
            return None;
        }
        let attr_name = source[begin..i].to_owned();
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let quote = *bytes.get(i)?;
        let quoted = quote == b'\'' || quote == b'"';
        if quoted {
            i += 1;
        }
        let value_start = i;
        while bytes.get(i).is_some_and(|b| {
            if quoted {
                *b != quote
            } else {
                !b.is_ascii_whitespace() && *b != b'>'
            }
        }) {
            i += 1;
        }
        let value = source[value_start..i].to_owned();
        attributes.push(Attribute {
            name: attr_name,
            value,
            start: value_start,
            end: i,
        });
        if quoted {
            if bytes.get(i) != Some(&quote) {
                return None;
            }
            i += 1;
        }
    }
}

fn reference(
    value: &str,
    kind: &str,
    rule: &str,
    start: usize,
    end: usize,
    mode: Mode,
) -> Reference {
    let decoded = quick_xml::escape::unescape(value).map(|v| v.into_owned());
    let (value, mode) = match decoded {
        Ok(s) if !["${", "#{", "<%"].iter().any(|p| s.contains(p)) => (s, mode),
        _ => ("<動的参照／未展開実体>".into(), Mode::Dynamic),
    };
    Reference {
        value,
        kind: kind.into(),
        rule: rule.into(),
        start,
        end,
        mode,
    }
}

fn merge_java(out: &mut Extraction, extra: Extraction) {
    out.declarations.extend(extra.declarations);
    out.references.extend(extra.references);
    out.errors.extend(extra.errors);
}

fn namespace<'a>(name: &str, map: &'a BTreeMap<String, String>) -> &'a str {
    let prefix = name.split_once(':').map(|p| p.0).unwrap_or("");
    map.get(prefix).map(String::as_str).unwrap_or("")
}

pub(crate) fn markup(
    source: &str,
    xml: bool,
    cancel: &Cancellation,
) -> Result<Extraction, CoreError> {
    let mut out = Extraction::default();
    let mut i = 0;
    let mut html_comment = false;
    let mut namespaces = vec![BTreeMap::new()];
    let mut tags = Vec::<String>::new();
    while i < source.len() {
        if i % 4096 == 0 {
            cancel.check()?;
        }
        let rest = &source[i..];
        if rest.starts_with("<%--") {
            match rest.find("--%>") {
                Some(e) => i += e + 4,
                None => {
                    out.errors.push((i, source.len(), "jsp-comment"));
                    break;
                }
            }
            continue;
        }
        if rest.starts_with("<!--") {
            html_comment = true;
            i += 4;
            continue;
        }
        if rest.starts_with("-->") {
            html_comment = false;
            i += 3;
            continue;
        }
        if rest.starts_with("<%") {
            let Some(close) = rest.find("%>") else {
                out.errors.push((i, source.len(), "jsp-region"));
                break;
            };
            let end = i + close;
            let prefix = source.as_bytes().get(i + 2).copied();
            if prefix == Some(b'@') {
                let fake = format!("<{}>", &source[i + 3..end]);
                if let Some(t) = tag(&fake, 0) {
                    for a in t.attributes {
                        let start = i + 2 + a.start;
                        let finish = i + 2 + a.end;
                        if t.name == "include" && a.name == "file" {
                            out.references.push(reference(
                                &a.value,
                                "include",
                                "JSP-INCLUDE",
                                start,
                                finish,
                                Mode::File,
                            ));
                        }
                        if t.name == "page" && a.name == "import" {
                            let mut offset = 0;
                            for value in a.value.split(',') {
                                let trimmed = value.trim();
                                let leading = value.len() - value.trim_start().len();
                                if !trimmed.is_empty() {
                                    out.references.push(Reference {
                                        value: trimmed.into(),
                                        kind: "import".into(),
                                        rule: "JAVA-IMPORT".into(),
                                        start: start + offset + leading,
                                        end: start + offset + leading + trimmed.len(),
                                        mode: if trimmed.ends_with(".*") {
                                            Mode::JavaWildcard
                                        } else {
                                            Mode::JavaImport
                                        },
                                    });
                                }
                                offset += value.len() + 1;
                            }
                        }
                    }
                }
            } else {
                let begin = i + if matches!(prefix, Some(b'!' | b'=')) {
                    3
                } else {
                    2
                };
                let mapped = Mapped::identity(&source[begin..end], begin);
                let mapped = match prefix {
                    Some(b'!') => mapped.wrap("class __Jsp {\n", "\n}"),
                    Some(b'=') => mapped.wrap("class __Jsp { Object run(){return ", ";} }"),
                    _ => mapped.wrap("class __Jsp { void run(){\n", "\n} }"),
                };
                merge_java(&mut out, syntax::java(mapped, cancel)?);
            }
            i = end + 2;
            continue;
        }
        if rest.starts_with('<') {
            if let Some(t) = tag(source, i) {
                i = t.end;
                if t.closing {
                    if tags.last() == Some(&t.name) {
                        tags.pop();
                        namespaces.pop();
                    }
                    continue;
                }
                let mut ns = namespaces.last().cloned().unwrap_or_default();
                for a in &t.attributes {
                    if a.name == "xmlns" {
                        ns.insert("".into(), a.value.clone());
                    } else if let Some(prefix) = a.name.strip_prefix("xmlns:") {
                        ns.insert(prefix.into(), a.value.clone());
                    }
                }
                let uri = namespace(&t.name, &ns);
                let local = t.name.rsplit(':').next().unwrap_or("");
                let jsp = if xml {
                    matches!(
                        uri,
                        "http://java.sun.com/JSP/Page"
                            | "http://xmlns.jcp.org/JSP/Page"
                            | "jakarta.tags.jsp"
                    )
                } else {
                    t.name.starts_with("jsp:")
                };
                let facelets = matches!(
                    uri,
                    "http://java.sun.com/jsf/facelets"
                        | "http://xmlns.jcp.org/jsf/facelets"
                        | "jakarta.faces.facelets"
                );
                let html = if xml {
                    uri == "http://www.w3.org/1999/xhtml"
                } else {
                    !t.name.contains(':')
                };
                for a in &t.attributes {
                    let rule = if jsp { "JSP-INCLUDE" } else { "MARKUP-REF" };
                    let kind = if jsp && matches!(local, "include" | "forward") && a.name == "page"
                    {
                        Some(if local == "include" {
                            "include"
                        } else {
                            "navigation"
                        })
                    } else if !html_comment && html {
                        match (local, a.name.as_str()) {
                            ("script", "src") => Some("script"),
                            ("a", "href") | ("form", "action") | ("iframe", "src") => {
                                Some("navigation")
                            }
                            _ => None,
                        }
                    } else if !html_comment
                        && facelets
                        && matches!(
                            (local, a.name.as_str()),
                            ("composition" | "decorate", "template") | ("include", "src")
                        )
                    {
                        Some("include")
                    } else {
                        None
                    };
                    if let Some(kind) = kind {
                        out.references.push(reference(
                            &a.value,
                            kind,
                            rule,
                            a.start,
                            a.end,
                            Mode::Url,
                        ));
                    }
                    if jsp && local == "directive.page" && a.name == "import" {
                        for value in a.value.split(',').map(str::trim) {
                            out.references.push(Reference {
                                value: value.into(),
                                kind: "import".into(),
                                rule: "JAVA-IMPORT".into(),
                                start: a.start,
                                end: a.end,
                                mode: if value.ends_with(".*") {
                                    Mode::JavaWildcard
                                } else {
                                    Mode::JavaImport
                                },
                            });
                        }
                    }
                    if jsp && local == "directive.include" && a.name == "file" {
                        out.references.push(reference(
                            &a.value,
                            "include",
                            rule,
                            a.start,
                            a.end,
                            Mode::File,
                        ));
                    }
                }
                if jsp && matches!(local, "declaration" | "scriptlet" | "expression") && !t.empty {
                    let closing = format!("</{}>", t.name);
                    if let Some(n) = source[i..].find(&closing) {
                        let finish = i + n;
                        let (begin, end) = if source[i..finish].starts_with("<![CDATA[")
                            && source[i..finish].ends_with("]]>")
                        {
                            (i + 9, finish - 3)
                        } else {
                            (i, finish)
                        };
                        let content = &source[begin..end];
                        // Entity decoding changes offsets; reject that range rather than invent evidence.
                        if content.contains('&') && !source[i..finish].starts_with("<![CDATA[") {
                            out.errors.push((begin, end, "jspx-entity-position"));
                        } else {
                            let mapped = Mapped::identity(content, begin);
                            let mapped = match local {
                                "declaration" => mapped.wrap("class __Jsp {\n", "\n}"),
                                "expression" => {
                                    mapped.wrap("class __Jsp { Object run(){return ", ";} }")
                                }
                                _ => mapped.wrap("class __Jsp { void run(){\n", "\n} }"),
                            };
                            merge_java(&mut out, syntax::java(mapped, cancel)?);
                        }
                        i = finish + closing.len();
                        continue;
                    }
                }
                if !t.empty
                    && !(!xml && matches!(local, "input" | "meta" | "link" | "br" | "img" | "hr"))
                {
                    tags.push(t.name);
                    namespaces.push(ns);
                }
                continue;
            }
        }
        i += source[i..].chars().next().unwrap().len_utf8();
    }
    if xml {
        out.errors.extend(xml_tree(source, cancel)?.1);
    }
    Ok(out)
}

#[derive(Clone, Default)]
struct XmlNode {
    name: String,
    uri: String,
    value: String,
    start: usize,
    end: usize,
    attributes: Vec<Attribute>,
    children: Vec<XmlNode>,
}

fn xml_tree(
    source: &str,
    cancel: &Cancellation,
) -> Result<(Vec<XmlNode>, Vec<crate::syntax::SyntaxError>), CoreError> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::reader::NsReader::from_str(source);
    let mut stack = Vec::<XmlNode>::new();
    let mut roots = Vec::new();
    let mut errors = Vec::new();
    loop {
        cancel.check()?;
        let start = reader.buffer_position() as usize;
        let event = reader.read_resolved_event().map(|(ns, event)| {
            (
                match ns {
                    quick_xml::name::ResolveResult::Bound(n) => n.as_ref().to_owned(),
                    _ => String::new(),
                },
                event,
            )
        });
        let end = reader.buffer_position() as usize;
        match event {
            Ok((resolved, Event::Start(e) | Event::Empty(e))) => {
                let name = e.local_name().as_ref().to_owned();
                let uri = resolved;
                let t = tag(source, start);
                let empty = t.as_ref().is_some_and(|t| t.empty);
                let node = XmlNode {
                    name,
                    uri,
                    start,
                    end,
                    attributes: t.map(|t| t.attributes).unwrap_or_default(),
                    ..Default::default()
                };
                if empty {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else {
                        roots.push(node);
                    }
                } else {
                    stack.push(node);
                }
            }
            Ok((_, Event::End(_))) => {
                if let Some(mut node) = stack.pop() {
                    node.end = end;
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else {
                        roots.push(node);
                    }
                }
            }
            Ok((_, Event::Text(e))) => {
                if let Some(node) = stack.last_mut() {
                    node.value.push_str(e.as_ref());
                }
            }
            Ok((_, Event::CData(e))) => {
                if let Some(node) = stack.last_mut() {
                    node.value.push_str(e.as_ref());
                }
            }
            Ok((_, Event::GeneralRef(e))) => {
                if let Some(node) = stack.last_mut() {
                    node.value.push('&');
                    node.value.push_str(e.as_ref());
                    node.value.push(';');
                }
            }
            Ok((_, Event::Eof)) => break,
            Err(_) => {
                errors.push((start, end.max(start).min(source.len()), "xml-syntax"));
                break;
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        errors.push((stack[0].start, source.len(), "xml-syntax"));
    }
    Ok((roots, errors))
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}
fn known(name: &str) -> bool {
    matches!(
        local(name),
        "class"
            | "class-name"
            | "servlet-class"
            | "filter-class"
            | "listener-class"
            | "file"
            | "path"
            | "resource"
            | "location"
            | "jsp-file"
            | "className"
    )
}
fn child<'a>(node: &'a XmlNode, name: &str) -> Option<&'a XmlNode> {
    node.children.iter().find(|n| n.name == name)
}

pub(crate) fn configuration(source: &str, cancel: &Cancellation) -> Result<Extraction, CoreError> {
    let (roots, errors) = xml_tree(source, cancel)?;
    let mut out = Extraction {
        errors,
        ..Default::default()
    };
    let mut stack: Vec<_> = roots.iter().collect();
    while let Some(node) = stack.pop() {
        cancel.check()?;
        for a in &node.attributes {
            out.references.push(reference(
                a.value.trim(),
                "configuration",
                "CONFIG-LITERAL",
                a.start,
                a.end,
                Mode::Literal(known(&a.name)),
            ));
        }
        if !node.value.trim().is_empty() {
            out.references.push(reference(
                node.value.trim(),
                "configuration",
                "CONFIG-LITERAL",
                node.start,
                node.end,
                Mode::Literal(known(&node.name)),
            ));
        }
        stack.extend(node.children.iter());
    }
    for root in &roots {
        let standard = root.name == "web-app"
            && matches!(
                root.uri.as_str(),
                "" | "http://java.sun.com/xml/ns/j2ee"
                    | "http://java.sun.com/xml/ns/javaee"
                    | "http://xmlns.jcp.org/xml/ns/javaee"
                    | "https://jakarta.ee/xml/ns/jakartaee"
            );
        if !standard {
            continue;
        }
        for mapping in root.children.iter().filter(|n| n.name == "servlet-mapping") {
            let Some(name) = child(mapping, "servlet-name") else {
                continue;
            };
            let servlets: Vec<_> = root
                .children
                .iter()
                .filter(|n| {
                    n.name == "servlet"
                        && child(n, "servlet-name")
                            .is_some_and(|s| s.value.trim() == name.value.trim())
                })
                .collect();
            for pattern in mapping.children.iter().filter(|n| n.name == "url-pattern") {
                let mut produced = false;
                for servlet in &servlets {
                    for target in servlet
                        .children
                        .iter()
                        .filter(|n| matches!(n.name.as_str(), "servlet-class" | "jsp-file"))
                    {
                        produced = true;
                        out.routes.push(Route {
                            pattern: pattern.value.trim().into(),
                            target: target.value.trim().into(),
                            java: target.name == "servlet-class",
                            evidence: vec![
                                (name.start, name.end),
                                (pattern.start, pattern.end),
                                (target.start, target.end),
                            ],
                        });
                    }
                }
                if !produced {
                    out.routes.push(Route {
                        pattern: pattern.value.trim().into(),
                        target: "<missing-servlet>".into(),
                        java: true,
                        evidence: vec![(name.start, name.end), (pattern.start, pattern.end)],
                    });
                }
            }
        }
        if let Some(list) = child(root, "welcome-file-list") {
            for welcome in &list.children {
                if welcome.name == "welcome-file" {
                    out.references.push(reference(
                        &format!("/{}", welcome.value.trim().trim_start_matches('/')),
                        "configuration",
                        "WEBXML-ROUTE",
                        welcome.start,
                        welcome.end,
                        Mode::Url,
                    ));
                }
            }
        }
    }
    Ok(out)
}

fn property_unescape(input: &str) -> Option<String> {
    let mut units = Vec::<u16>::new();
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            let mut buf = [0u16; 2];
            units.extend_from_slice(ch.encode_utf16(&mut buf));
            continue;
        }
        match chars.next()? {
            'u' => {
                let h: String = chars.by_ref().take(4).collect();
                if h.len() != 4 {
                    return None;
                }
                units.push(u16::from_str_radix(&h, 16).ok()?);
            }
            'n' => units.push(10),
            'r' => units.push(13),
            't' => units.push(9),
            'f' => units.push(12),
            ch => {
                let mut buf = [0u16; 2];
                units.extend_from_slice(ch.encode_utf16(&mut buf));
            }
        }
    }
    String::from_utf16(&units).ok()
}

pub(crate) fn properties(source: &str, cancel: &Cancellation) -> Result<Extraction, CoreError> {
    let mut out = Extraction::default();
    let mut logical = String::new();
    let mut start = 0;
    let mut offset = 0;
    for physical in source.split_inclusive('\n') {
        cancel.check()?;
        if logical.is_empty() {
            start = offset;
        }
        let line = physical.trim_end_matches(['\r', '\n']);
        let line = if logical.is_empty() {
            line
        } else {
            line.trim_start_matches([' ', '\t', '\u{c}'])
        };
        let slash = line.bytes().rev().take_while(|b| *b == b'\\').count();
        logical.push_str(if slash % 2 == 1 {
            &line[..line.len() - 1]
        } else {
            line
        });
        offset += physical.len();
        if slash % 2 == 1 {
            continue;
        }
        let trimmed = logical.trim_start_matches([' ', '\t', '\u{c}']);
        if !trimmed.starts_with(['#', '!']) && !trimmed.is_empty() {
            let mut escaped = false;
            let mut sep = trimmed.len();
            for (i, ch) in trimmed.char_indices() {
                if !escaped && (matches!(ch, '=' | ':' | ' ' | '\t' | '\u{c}')) {
                    sep = i;
                    break;
                }
                escaped = ch == '\\' && !escaped;
            }
            let key = property_unescape(&trimmed[..sep]);
            let raw = trimmed[sep..]
                .trim_start_matches([' ', '\t', '\u{c}'])
                .trim_start_matches(['=', ':'])
                .trim_start_matches([' ', '\t', '\u{c}']);
            if let (Some(key), Some(value)) = (key, property_unescape(raw)) {
                let name = key.rsplit('.').next().unwrap_or("");
                out.references.push(Reference {
                    value,
                    kind: "configuration".into(),
                    rule: "CONFIG-LITERAL".into(),
                    start,
                    end: offset,
                    mode: Mode::Literal(known(name)),
                });
            } else {
                out.errors.push((start, offset, "properties-escape"));
            }
        }
        logical.clear();
    }
    if !logical.is_empty() {
        out.errors
            .push((start, source.len(), "properties-continuation"));
    }
    Ok(out)
}
