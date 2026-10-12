use crate::{
    decode, input, markup,
    syntax::{self, Extraction, Mode, Reference},
    *,
};
use std::collections::{BTreeMap, BTreeSet};

fn error(err: InputError) -> CoreError {
    if err == InputError::Cancelled {
        CoreError::Cancelled
    } else {
        CoreError::InputBoundary
    }
}

fn diagnostic(
    snapshot: &mut AnalysisSnapshot,
    code: &str,
    stage: &str,
    path: Option<&str>,
    range: Option<(usize, usize)>,
    reason: &str,
) {
    let resource_id = path.and_then(|p| {
        snapshot
            .resources
            .iter()
            .find(|r| r.path == p)
            .map(|r| r.id.clone())
    });
    let source = resource_id.as_ref().and_then(|id| snapshot.sources.get(id));
    let pos = |byte| {
        source.and_then(|s| {
            if s.is_char_boundary(byte) {
                Some(position(s, byte))
            } else {
                None
            }
        })
    };
    snapshot.diagnostics.push(Diagnostic {
        id: format!("d{}", snapshot.diagnostics.len()),
        code: code.into(),
        stage: stage.into(),
        resource_id,
        path: path.map(String::from),
        start: range.and_then(|r| pos(r.0)),
        end: range.and_then(|r| pos(r.1)),
        reason: reason.into(),
    });
}

pub fn analyze(
    options: AnalysisOptions,
    input: &dyn InputProvider,
    cancel: &Cancellation,
    mut progress: impl FnMut(Progress),
) -> Result<AnalysisSnapshot, CoreError> {
    if options.exclusions.iter().any(|p| !input::relative_path(p))
        || options.encodings.keys().any(|p| !input::relative_path(p))
        || options
            .web_root
            .as_ref()
            .is_some_and(|p| !p.is_empty() && !input::relative_path(p))
    {
        return Err(CoreError::InputBoundary);
    }
    progress(Progress {
        stage: "enumerate",
        completed: 0,
        total: None,
    });
    let listing = input.enumerate(&options, cancel).map_err(error)?;
    let mut snapshot = AnalysisSnapshot {
        id: String::new(),
        options,
        resources: Vec::new(),
        symbols: Vec::new(),
        relations: Vec::new(),
        evidences: Vec::new(),
        diagnostics: Vec::new(),
        skipped: listing.skipped,
        web_roots: Vec::new(),
        sources: BTreeMap::new(),
    };
    for path in &listing.paths {
        if path.ends_with("/WEB-INF/web.xml") {
            snapshot
                .web_roots
                .push(path.trim_end_matches("/WEB-INF/web.xml").into());
        } else if path == "WEB-INF/web.xml" {
            snapshot.web_roots.push(String::new());
        }
    }
    snapshot.web_roots.sort();
    snapshot.web_roots.dedup();
    if snapshot.options.web_root.is_none() && snapshot.web_roots.len() == 1 {
        snapshot.options.web_root = Some(snapshot.web_roots[0].clone());
    }
    for skip in snapshot.skipped.clone() {
        if skip.reason == "link" {
            diagnostic(
                &mut snapshot,
                "LEG-1002",
                "enumerate",
                Some(&skip.path),
                None,
                "link",
            );
        }
        if skip.reason.starts_with("read:") {
            diagnostic(
                &mut snapshot,
                "LEG-1001",
                "enumerate",
                Some(&skip.path),
                None,
                "unreadable",
            );
        }
    }
    let total = listing.paths.len();
    for (index, path) in listing.paths.iter().enumerate() {
        cancel.check()?;
        progress(Progress {
            stage: "decode",
            completed: index,
            total: Some(total),
        });
        let Some(language) = model::language(path) else {
            continue;
        };
        let id = input::hash(path.as_bytes());
        let mut resource = Resource {
            id: id.clone(),
            path: path.clone(),
            language: language.into(),
            hash: None,
            encoding: None,
            encoding_reason: String::new(),
            read: "failed".into(),
            decode: "failed".into(),
            parse: "unparsed".into(),
        };
        let bytes = input.read(path, cancel);
        match bytes {
            Err(InputError::Cancelled) => return Err(CoreError::Cancelled),
            Err(e) => {
                snapshot.resources.push(resource);
                diagnostic(
                    &mut snapshot,
                    if e == InputError::Boundary {
                        "LEG-1002"
                    } else {
                        "LEG-1001"
                    },
                    "read",
                    Some(path),
                    None,
                    match e {
                        InputError::Missing => "missing",
                        InputError::Access => "access-denied",
                        InputError::Changed => "changed",
                        InputError::Boundary => "boundary",
                        _ => "unknown",
                    },
                );
            }
            Ok(bytes) => {
                resource.hash = Some(input::hash(&bytes));
                resource.read = "ok".into();
                let decoded = decode::decode(&bytes, snapshot.options.encodings.get(path));
                if let Some(decoded) = decoded {
                    resource.encoding = Some(decoded.encoding);
                    resource.encoding_reason = decoded.reason;
                    resource.decode = "ok".into();
                    snapshot.sources.insert(id, decoded.text);
                    snapshot.resources.push(resource);
                    if decoded.ambiguous {
                        diagnostic(
                            &mut snapshot,
                            "LEG-2002",
                            "decode",
                            Some(path),
                            None,
                            "ambiguous",
                        );
                    }
                } else {
                    snapshot.resources.push(resource);
                    diagnostic(
                        &mut snapshot,
                        "LEG-2001",
                        "decode",
                        Some(path),
                        None,
                        "invalid-or-unsupported-encoding",
                    );
                }
            }
        }
    }
    let mut extractions = BTreeMap::new();
    for index in 0..snapshot.resources.len() {
        cancel.check()?;
        progress(Progress {
            stage: "extract",
            completed: index,
            total: Some(total),
        });
        let r = &snapshot.resources[index];
        let Some(source) = snapshot.sources.get(&r.id) else {
            continue;
        };
        let extraction = match r.language.as_str() {
            "java" => syntax::java(syntax::Mapped::identity(source, 0), cancel)?,
            "javascript" => syntax::javascript(source, cancel)?,
            "jsp" => markup::markup(source, false, cancel)?,
            "jspx" | "xhtml" => markup::markup(source, true, cancel)?,
            "xml" => markup::configuration(source, cancel)?,
            "properties" => markup::properties(source, cancel)?,
            _ => Extraction::default(),
        };
        let id = r.id.clone();
        let path = r.path.clone();
        snapshot.resources[index].parse = if extraction.errors.is_empty() {
            "ok"
        } else {
            "partial"
        }
        .into();
        for (s, e, reason) in &extraction.errors {
            diagnostic(
                &mut snapshot,
                "LEG-3001",
                "extract",
                Some(&path),
                Some((*s, *e)),
                reason,
            );
        }
        for d in &extraction.declarations {
            snapshot.symbols.push(Symbol {
                name: d.name.clone(),
                resource_id: id.clone(),
                kind: d.kind.clone(),
                start: position(snapshot.source(&id)?, d.start),
                end: position(snapshot.source(&id)?, d.end),
            });
        }
        // Declarations do not override the actual byte encoding.
        if let Some(encoding) = snapshot.resources[index].encoding.as_deref() {
            let head = snapshot
                .source(&id)?
                .get(..snapshot.source(&id)?.len().min(512))
                .unwrap_or("")
                .to_ascii_lowercase();
            let contradiction = (head.contains("encoding=\"shift_jis\"")
                || head.contains("encoding=\"windows-31j\"")
                || head.contains("pageencoding=\"shift_jis\""))
                && encoding == "UTF-8"
                || head.contains("encoding=\"utf-8\"") && encoding.starts_with("SJIS");
            if contradiction {
                diagnostic(
                    &mut snapshot,
                    "LEG-2002",
                    "decode",
                    Some(&path),
                    None,
                    "declaration-mismatch",
                );
            }
        }
        extractions.insert(id, extraction);
    }
    let resources = snapshot.resources.clone();
    let paths: BTreeMap<_, _> = resources
        .iter()
        .map(|r| (r.path.clone(), r.id.clone()))
        .collect();
    let mut symbols = BTreeMap::<String, Vec<String>>::new();
    for symbol in &snapshot.symbols {
        symbols
            .entry(symbol.name.clone())
            .or_default()
            .push(symbol.resource_id.clone());
    }
    let mut routes = Vec::new();
    for (id, x) in &extractions {
        for route in &x.routes {
            if !snapshot
                .resources
                .iter()
                .any(|r| &r.id == id && r.path.ends_with("WEB-INF/web.xml"))
            {
                continue;
            }
            routes.push((id.clone(), route.clone()));
        }
    }
    for (i, r) in resources.iter().enumerate() {
        progress(Progress {
            stage: "resolve",
            completed: i,
            total: Some(total),
        });
        cancel.check()?;
        let Some(x) = extractions.get(&r.id) else {
            continue;
        };
        for reference in &x.references {
            cancel.check()?;
            let Some(resolved) = resolve(
                reference,
                r,
                x,
                &symbols,
                &paths,
                &routes,
                snapshot.options.web_root.as_deref(),
            ) else {
                continue;
            };
            let (status, targets, reason, additional) = resolved;
            let mut evidence = vec![add_evidence(
                &mut snapshot,
                &r.id,
                reference.start,
                reference.end,
                &reference.rule,
            )?];
            for (id, start, end) in additional {
                evidence.push(add_evidence(
                    &mut snapshot,
                    &id,
                    start,
                    end,
                    "WEBXML-ROUTE",
                )?);
            }
            let key_reference = if status == Resolution::Confirmed {
                String::new()
            } else {
                reference.value.clone()
            };
            if let Some(existing) = snapshot.relations.iter_mut().find(|a| {
                a.from == r.id
                    && a.kind == reference.kind
                    && a.status == status
                    && a.targets == targets
                    && (status == Resolution::Confirmed || a.reference == key_reference)
            }) {
                for id in evidence {
                    if !existing.evidence_ids.contains(&id) {
                        existing.evidence_ids.push(id);
                    }
                }
            } else {
                snapshot.relations.push(Relation {
                    id: String::new(),
                    from: r.id.clone(),
                    kind: reference.kind.clone(),
                    status: status.clone(),
                    targets,
                    reference: reference.value.clone(),
                    reason: reason.into(),
                    evidence_ids: evidence,
                });
            }
            if status != Resolution::Confirmed {
                diagnostic(
                    &mut snapshot,
                    "LEG-4001",
                    "resolve",
                    Some(&r.path),
                    Some((reference.start, reference.end)),
                    reason,
                );
            }
        }
    }
    snapshot.relations.sort_by(|a, b| {
        (&a.from, &a.kind, &a.targets, &a.reference).cmp(&(
            &b.from,
            &b.kind,
            &b.targets,
            &b.reference,
        ))
    });
    for (i, r) in snapshot.relations.iter_mut().enumerate() {
        r.id = format!("r{i}");
    }
    let fingerprint =
        serde_json::to_vec(&(&snapshot.options, &snapshot.resources, &snapshot.skipped))
            .map_err(|_| CoreError::Internal)?;
    snapshot.id = input::hash(&fingerprint);
    cancel.check()?;
    progress(Progress {
        stage: "ready",
        completed: total,
        total: Some(total),
    });
    Ok(snapshot)
}

fn add_evidence(
    s: &mut AnalysisSnapshot,
    id: &str,
    start: usize,
    end: usize,
    rule: &str,
) -> Result<String, CoreError> {
    let source = s.source(id)?;
    if start > end || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return Err(CoreError::InvalidRange);
    }
    if let Some(e) = s.evidences.iter().find(|e| {
        e.resource_id == id && e.start.byte == start && e.end.byte == end && e.rule == rule
    }) {
        return Ok(e.id.clone());
    }
    let evidence = Evidence {
        id: format!("e{}", s.evidences.len()),
        resource_id: id.into(),
        start: position(source, start),
        end: position(source, end),
        rule: rule.into(),
    };
    let id = evidence.id.clone();
    s.evidences.push(evidence);
    Ok(id)
}

fn classification(
    mut targets: Vec<String>,
    force_candidate: bool,
    reason: &'static str,
) -> (Resolution, Vec<String>, &'static str) {
    targets.sort();
    targets.dedup();
    let status = if targets.is_empty() {
        Resolution::Unresolved
    } else if targets.len() == 1 && !force_candidate {
        Resolution::Confirmed
    } else {
        Resolution::Candidate
    };
    let reason = if status == Resolution::Confirmed {
        "unique"
    } else {
        reason
    };
    (status, targets, reason)
}

type Resolved = (
    Resolution,
    Vec<String>,
    &'static str,
    Vec<(String, usize, usize)>,
);

fn resolve(
    reference: &Reference,
    r: &Resource,
    x: &Extraction,
    symbols: &BTreeMap<String, Vec<String>>,
    paths: &BTreeMap<String, String>,
    routes: &[(String, syntax::Route)],
    web_root: Option<&str>,
) -> Option<Resolved> {
    let value = reference.value.trim();
    let mut additional = Vec::new();
    let (status, targets, reason) = match &reference.mode {
        Mode::Dynamic => (Resolution::Unresolved, Vec::new(), "dynamic-or-entity"),
        Mode::JavaWildcard => (Resolution::Unresolved, Vec::new(), "wildcard-not-expanded"),
        Mode::JavaImport => {
            let found = symbols.get(value).cloned().unwrap_or_default();
            classification(found, false, "duplicate-or-external-type")
        }
        Mode::Java => {
            if let Some(found) = symbols.get(value) {
                classification(found.clone(), false, "duplicate-type")
            } else {
                let scoped: Vec<_> = x
                    .declarations
                    .iter()
                    .filter(|d| reference.start >= d.scope_start && reference.end <= d.scope_end)
                    .collect();
                let mut local = Vec::new();
                for d in scoped.iter().rev() {
                    let prefix = d.name.rsplit_once('.').map(|p| p.0).unwrap_or("");
                    for name in [format!("{}.{}", d.name, value), format!("{prefix}.{value}")] {
                        if let Some(ids) = symbols.get(&name) {
                            local.extend(ids.clone());
                        }
                    }
                }
                let mut imported = Vec::new();
                for import in x.references.iter().filter(|a| a.mode == Mode::JavaImport) {
                    let name = &import.value;
                    let short = name.rsplit('.').next().unwrap_or("");
                    let first = value.split('.').next().unwrap_or("");
                    if short == first {
                        let full = format!("{}{}", name, &value[first.len()..]);
                        if let Some(ids) = symbols.get(&full) {
                            imported.extend(ids.clone());
                        }
                    }
                }
                let same = symbols
                    .get(&if x.package.is_empty() {
                        value.into()
                    } else {
                        format!("{}.{value}", x.package)
                    })
                    .cloned()
                    .unwrap_or_default();
                if !local.is_empty() {
                    classification(local, false, "scope-conflict")
                } else if !imported.is_empty() {
                    let mut found = imported;
                    found.extend(same);
                    classification(found, false, "import-conflict")
                } else if !same.is_empty() {
                    classification(same, false, "duplicate-type")
                } else {
                    let mut wildcard = Vec::new();
                    for import in x.references.iter().filter(|a| a.mode == Mode::JavaWildcard) {
                        if let Some(ids) = symbols.get(&format!(
                            "{}.{}",
                            import.value.trim_end_matches(".*"),
                            value
                        )) {
                            wildcard.extend(ids.clone());
                        }
                    }
                    classification(wildcard, true, "classpath-or-wildcard")
                }
            }
        }
        Mode::Literal(known) => {
            let valid_class = value.contains('.')
                && value.split('.').all(|part| {
                    !part.is_empty()
                        && part.chars().enumerate().all(|(i, c)| {
                            c == '_' || c == '$' || c.is_alphabetic() || i > 0 && c.is_ascii_digit()
                        })
                });
            if let Some(ids) = symbols.get(value) {
                classification(ids.clone(), !*known, "unknown-item-string-match")
            } else if model::language(value).is_some() {
                let path = normalize_path(&r.path, value, web_root, false);
                let found = path
                    .and_then(|p| paths.get(&p).cloned())
                    .into_iter()
                    .collect();
                classification(found, !*known, "missing-or-unknown-path")
            } else if *known && valid_class {
                (
                    Resolution::Unresolved,
                    Vec::new(),
                    "external-or-missing-class",
                )
            } else {
                return None;
            }
        }
        Mode::File | Mode::Url | Mode::Js | Mode::JsCandidate => {
            if value.starts_with('#') || value.is_empty() {
                return None;
            }
            if matches!(reference.mode, Mode::Js | Mode::JsCandidate)
                && !value.starts_with('.')
                && !value.starts_with('/')
            {
                return Some((
                    Resolution::Unresolved,
                    Vec::new(),
                    "external-module-or-alias",
                    Vec::new(),
                ));
            }
            let is_url = reference.mode == Mode::Url;
            let Some(path) = normalize_path(&r.path, value, web_root, is_url) else {
                return Some((
                    Resolution::Unresolved,
                    Vec::new(),
                    "external-or-boundary-or-webroot",
                    Vec::new(),
                ));
            };
            let mut matched = Vec::new();
            if is_url {
                if let Some(root) = web_root {
                    let prefix = if root.is_empty() {
                        String::new()
                    } else {
                        format!("{root}/")
                    };
                    if let Some(url) = path.strip_prefix(&prefix) {
                        let url = format!("/{url}");
                        for (id, route) in routes {
                            if let Some(rank) = route_rank(&route.pattern, &url) {
                                matched.push((rank, id, route));
                            }
                        }
                    }
                }
            }
            if !matched.is_empty() {
                let best = matched.iter().map(|m| m.0).max().unwrap();
                let chosen: Vec<_> = matched.into_iter().filter(|m| m.0 == best).collect();
                let mut found = Vec::new();
                let force = chosen.len() != 1;
                for (_, id, route) in chosen {
                    if route.java {
                        found.extend(symbols.get(&route.target).cloned().unwrap_or_default());
                    } else if let Some(p) = normalize_path("", &route.target, web_root, true) {
                        found.extend(paths.get(&p).cloned());
                    }
                    additional.extend(route.evidence.iter().map(|(s, e)| (id.clone(), *s, *e)));
                }
                classification(found, force, "servlet-conflict-or-missing-target")
            } else if let Some(id) = paths.get(&path) {
                classification(
                    vec![id.clone()],
                    reference.mode == Mode::JsCandidate,
                    "local-require",
                )
            } else if matches!(reference.mode, Mode::Js | Mode::JsCandidate) {
                let mut found = Vec::new();
                for extension in ["js", "mjs", "cjs"] {
                    for p in [
                        format!("{path}.{extension}"),
                        format!("{path}/index.{extension}"),
                    ] {
                        if let Some(id) = paths.get(&p) {
                            found.push(id.clone());
                        }
                    }
                }
                classification(found, true, "extension-or-runtime-rule")
            } else {
                (Resolution::Unresolved, Vec::new(), "missing-path")
            }
        }
    };
    Some((status, targets, reason, additional))
}

pub(crate) fn normalize_path(
    from: &str,
    value: &str,
    root: Option<&str>,
    url: bool,
) -> Option<String> {
    let value = if url {
        value.split(['?', '#']).next().unwrap_or("")
    } else {
        value
    };
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < value.len() {
        if url && value.as_bytes()[i] == b'%' {
            let code = value
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())?;
            decoded.push(code);
            i += 3;
        } else {
            decoded.push(value.as_bytes()[i]);
            i += 1;
        }
    }
    let value = String::from_utf8(decoded).ok()?;
    if value.contains([':', '\\', '\0']) || value.starts_with("//") {
        return None;
    }
    let base = if value.starts_with('/') {
        root?
    } else {
        from.rsplit_once('/').map(|p| p.0).unwrap_or("")
    };
    let mut segments: Vec<_> = base.split('/').filter(|s| !s.is_empty()).collect();
    for part in value.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            s => segments.push(s),
        }
    }
    Some(segments.join("/"))
}

fn route_rank(pattern: &str, url: &str) -> Option<(u8, usize)> {
    if pattern == url {
        Some((4, pattern.len()))
    } else if let Some(prefix) = pattern.strip_suffix("/*") {
        if url == prefix || url.starts_with(&format!("{prefix}/")) {
            Some((3, prefix.len()))
        } else {
            None
        }
    } else if let Some(ext) = pattern.strip_prefix("*.") {
        if url
            .rsplit('/')
            .next()
            .unwrap_or("")
            .ends_with(&format!(".{ext}"))
        {
            Some((2, ext.len()))
        } else {
            None
        }
    } else if pattern == "/" {
        Some((1, 0))
    } else {
        None
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct InputChange {
    pub path: String,
    pub kind: &'static str,
}

pub fn verify_inputs(
    snapshot: &AnalysisSnapshot,
    input: &dyn InputProvider,
    cancel: &Cancellation,
) -> Result<Vec<InputChange>, CoreError> {
    let listing = input.enumerate(&snapshot.options, cancel).map_err(error)?;
    let previous: BTreeSet<_> = snapshot.resources.iter().map(|r| r.path.as_str()).collect();
    let current: BTreeSet<_> = listing.paths.iter().map(String::as_str).collect();
    let mut changes = Vec::new();
    for path in previous.difference(&current) {
        changes.push(InputChange {
            path: (*path).into(),
            kind: "deleted",
        });
    }
    for path in current.difference(&previous) {
        changes.push(InputChange {
            path: (*path).into(),
            kind: "added",
        });
    }
    for path in previous.intersection(&current) {
        cancel.check()?;
        let expected = snapshot
            .resources
            .iter()
            .find(|r| r.path == **path)
            .and_then(|r| r.hash.as_ref());
        match input.read(path, cancel) {
            Ok(bytes) if expected == Some(&input::hash(&bytes)) => {}
            Err(InputError::Cancelled) => return Err(CoreError::Cancelled),
            Err(_) => changes.push(InputChange {
                path: (*path).into(),
                kind: "unreadable",
            }),
            _ => changes.push(InputChange {
                path: (*path).into(),
                kind: "changed",
            }),
        }
    }
    if serde_json::to_vec(&listing.skipped).ok() != serde_json::to_vec(&snapshot.skipped).ok() {
        changes.push(InputChange {
            path: String::new(),
            kind: "enumeration-changed",
        });
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}
