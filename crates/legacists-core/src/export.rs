use crate::*;
use serde::Serialize;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Debug)]
pub struct ExportOptions {
    pub app_version: String,
    pub max_bytes: usize,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            app_version: VERSION.into(),
            max_bytes: 800_000,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPart {
    pub index: usize,
    pub bytes: usize,
}

#[derive(Clone, Debug)]
pub struct ExportPlan {
    pub snapshot: Arc<AnalysisSnapshot>,
    pub selected: Vec<String>,
    pub parts: Vec<ExportPart>,
    content: Vec<Vec<u8>>,
}

fn header(version: &str, index: usize, total: usize) -> String {
    format!("LEGACISTS-TXT/1\nPART {:06}/{:06}\nAPP {}\nSCOPE file-level explicit static references; UTF-8/SJIS; dynamic and external references may be unresolved\n",index+1,total,serde_json::to_string(version).unwrap())
}

fn boundary(text: &str, mut end: usize) -> usize {
    end = end.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if end > 0
        && text.as_bytes().get(end - 1) == Some(&b'\r')
        && text.as_bytes().get(end) == Some(&b'\n')
    {
        end -= 1;
    }
    end
}

fn source_record(path: &str, source: &str, start: usize, end: usize) -> String {
    let first = position(source, start);
    let last = position(source, end);
    format!("SOURCE path={} bytes={} start={} end={} lines={}-{} columns={}-{} continuation={}\n{}\nEND-SOURCE\n",serde_json::to_string(path).unwrap(),end-start,start,end,first.line,last.line,first.column,last.column,start>0,&source[start..end])
}

fn meta_record(kind: &str, record: &str, start: usize, end: usize) -> String {
    format!("META kind={kind} bytes={} start={start} end={end} total={} continuation={}\n{}\nEND-META\n",end-start,record.len(),start>0,&record[start..end])
}

fn fitting_end(
    text: &str,
    start: usize,
    available: usize,
    render: impl Fn(usize) -> String,
) -> Result<usize, CoreError> {
    let mut lo = start;
    let mut hi = text.len();
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        let end = boundary(text, mid);
        if render(end).len() <= available {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let mut end = boundary(text, lo);
    if end == start && start < text.len() {
        return Err(CoreError::InvalidLimit);
    }
    if end < text.len() {
        if let Some(line) = text[start..end].rfind('\n') {
            let preferred = start + line + 1;
            if preferred > start {
                end = preferred;
            }
        }
    }
    Ok(end)
}

pub fn plan_export(
    snapshot: Arc<AnalysisSnapshot>,
    selection: &[String],
    options: &ExportOptions,
    cancel: &Cancellation,
) -> Result<ExportPlan, CoreError> {
    if options.max_bytes < 16384
        || options.max_bytes > 1_000_000_000
        || options.app_version.len() > 256
    {
        return Err(CoreError::InvalidLimit);
    }
    let ids: BTreeSet<_> = selection.iter().cloned().collect();
    if ids.is_empty() {
        return Err(CoreError::EmptySelection);
    }
    let mut selected: Vec<_> = ids.into_iter().collect();
    for id in &selected {
        snapshot.source(id)?;
    }
    selected.sort_by(|a, b| {
        snapshot
            .resource(a)
            .unwrap()
            .path
            .cmp(&snapshot.resource(b).unwrap().path)
    });
    let mut records = Vec::<(String, String)>::new();
    records.push((
        "options".into(),
        serde_json::to_string(&snapshot.options).map_err(|_| CoreError::Internal)?,
    ));
    for id in &selected {
        let r = snapshot.resource(id)?;
        records.push(("file".into(), serde_json::to_string(r).unwrap()));
    }
    for r in snapshot
        .relations
        .iter()
        .filter(|r| selected.contains(&r.from) || r.targets.iter().any(|id| selected.contains(id)))
    {
        let evidence:Vec<_>=r.evidence_ids.iter().filter_map(|id|snapshot.evidences.iter().find(|e|&e.id==id)).map(|e|serde_json::json!({"path":snapshot.resource(&e.resource_id).unwrap().path,"start":e.start,"end":e.end,"rule":e.rule})).collect();
        let targets: Vec<_> = r
            .targets
            .iter()
            .map(|id| snapshot.resource(id).unwrap().path.as_str())
            .collect();
        records.push(("relation".into(),serde_json::json!({"from":snapshot.resource(&r.from)?.path,"kind":r.kind,"status":r.status,"targets":targets,"reference":r.reference,"reason":r.reason,"evidence":evidence}).to_string()));
    }
    for d in &snapshot.diagnostics {
        if d.resource_id
            .as_ref()
            .is_some_and(|id| selected.contains(id))
        {
            records.push(("diagnostic".into(), serde_json::to_string(d).unwrap()));
        }
    }
    let reserved = header(&options.app_version, 999998, 999999).len();
    if reserved + 1024 >= options.max_bytes {
        return Err(CoreError::InvalidLimit);
    }
    let capacity = options.max_bytes - reserved;
    let mut chunks = vec![String::new()];
    for (kind, record) in records {
        let mut start = 0;
        loop {
            cancel.check()?;
            let available = capacity - chunks.last().unwrap().len();
            let whole = meta_record(&kind, &record, start, record.len());
            if whole.len() <= available {
                chunks.last_mut().unwrap().push_str(&whole);
                break;
            }
            if !chunks.last().unwrap().is_empty() {
                chunks.push(String::new());
                continue;
            }
            let end = fitting_end(&record, start, available, |end| {
                meta_record(&kind, &record, start, end)
            })?;
            chunks
                .last_mut()
                .unwrap()
                .push_str(&meta_record(&kind, &record, start, end));
            start = end;
            if start == record.len() {
                break;
            }
            chunks.push(String::new());
        }
    }
    for id in &selected {
        cancel.check()?;
        let resource = snapshot.resource(id)?;
        let source = snapshot.source(id)?;
        let mut start = 0;
        loop {
            cancel.check()?;
            let available = capacity - chunks.last().unwrap().len();
            let whole = source_record(&resource.path, source, start, source.len());
            if whole.len() <= available {
                chunks.last_mut().unwrap().push_str(&whole);
                break;
            }
            // Prefer a complete source on a new part before splitting it.
            if !chunks.last().unwrap().is_empty() {
                chunks.push(String::new());
                continue;
            }
            let end = fitting_end(source, start, available, |end| {
                source_record(&resource.path, source, start, end)
            })?;
            chunks
                .last_mut()
                .unwrap()
                .push_str(&source_record(&resource.path, source, start, end));
            start = end;
            if start == source.len() {
                break;
            }
            chunks.push(String::new());
        }
    }
    let total = chunks.len();
    if total > 999999 {
        return Err(CoreError::InvalidLimit);
    }
    let content: Vec<Vec<u8>> = chunks
        .into_iter()
        .enumerate()
        .map(|(i, c)| format!("{}{c}", header(&options.app_version, i, total)).into_bytes())
        .collect();
    if content.iter().any(|c| c.len() > options.max_bytes) {
        return Err(CoreError::Internal);
    }
    // Independent framing validation guards both size and exact source reconstruction.
    let reconstructed = reconstruct_sources(&content)?;
    for id in &selected {
        let path = &snapshot.resource(id)?.path;
        if reconstructed.get(path).map(String::as_str) != Some(snapshot.source(id)?) {
            return Err(CoreError::Internal);
        }
    }
    let parts = content
        .iter()
        .enumerate()
        .map(|(index, c)| ExportPart {
            index,
            bytes: c.len(),
        })
        .collect();
    Ok(ExportPlan {
        snapshot,
        selected,
        parts,
        content,
    })
}

pub fn render_export(plan: &ExportPlan, index: usize) -> Result<&[u8], CoreError> {
    plan.content
        .get(index)
        .map(Vec::as_slice)
        .ok_or(CoreError::InvalidRange)
}

pub fn reconstruct_sources(
    parts: &[Vec<u8>],
) -> Result<std::collections::BTreeMap<String, String>, CoreError> {
    let mut out = std::collections::BTreeMap::<String, String>::new();
    for part in parts {
        let text = std::str::from_utf8(part).map_err(|_| CoreError::Internal)?;
        let mut offset = 0;
        while offset < text.len() {
            let line_end = text[offset..]
                .find('\n')
                .map(|n| offset + n)
                .ok_or(CoreError::Internal)?;
            let line = &text[offset..line_end];
            offset = line_end + 1;
            if line.starts_with("SOURCE ") || line.starts_with("META ") {
                let mut path = None;
                let attributes = if let Some(rest) = line.strip_prefix("SOURCE path=") {
                    let mut decoder =
                        serde_json::Deserializer::from_str(rest).into_iter::<String>();
                    path = Some(
                        decoder
                            .next()
                            .ok_or(CoreError::Internal)?
                            .map_err(|_| CoreError::Internal)?,
                    );
                    &rest[decoder.byte_offset()..]
                } else {
                    line
                };
                let bytes = attributes
                    .split(" bytes=")
                    .nth(1)
                    .and_then(|v| v.split(' ').next())
                    .and_then(|v| v.parse::<usize>().ok())
                    .ok_or(CoreError::Internal)?;
                let body = text
                    .get(offset..offset + bytes)
                    .ok_or(CoreError::Internal)?;
                if let Some(path) = path {
                    out.entry(path).or_default().push_str(body);
                }
                offset += bytes;
                let ending = if line.starts_with("SOURCE ") {
                    "\nEND-SOURCE\n"
                } else {
                    "\nEND-META\n"
                };
                if !text[offset..].starts_with(ending) {
                    return Err(CoreError::Internal);
                }
                offset += ending.len();
            }
        }
    }
    Ok(out)
}
